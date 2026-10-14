use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use super::ffi;
use super::query::SourceQuery;
use super::runtime::NdiRuntime;
use super::scope::DiscoveryScope;

/// An NDI source copied out of the finder, so it outlives the finder's buffers.
pub struct SourceDescriptor {
    name: CString,
    url_address: CString,
}

impl SourceDescriptor {
    pub fn name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }

    /// Borrowed view for the C API; only valid while `self` is alive.
    pub(super) fn as_raw(&self) -> ffi::Source {
        ffi::Source {
            ndi_name: self.name.as_ptr(),
            url_address: self.url_address.as_ptr(),
        }
    }

    /// Returns `None` when NDI has not filled in both strings yet.
    ///
    /// # Safety
    /// Non-null pointers in `raw` must be valid NUL-terminated strings.
    unsafe fn copy_from(raw: &ffi::Source) -> Option<Self> {
        if raw.ndi_name.is_null() || raw.url_address.is_null() {
            return None;
        }
        // SAFETY: Both pointers are non-null and guaranteed valid by the caller.
        let (name, url_address) = unsafe {
            (
                CStr::from_ptr(raw.ndi_name),
                CStr::from_ptr(raw.url_address),
            )
        };
        Some(Self {
            name: name.to_owned(),
            url_address: url_address.to_owned(),
        })
    }

    fn satisfies(&self, query: &SourceQuery) -> bool {
        query.matches(&self.name())
    }

    fn is_within(&self, scope: DiscoveryScope) -> bool {
        scope.includes(&self.url_address.to_string_lossy())
    }
}

/// Discovers NDI sources using NDI's own discovery (mDNS, or a discovery
/// server if NDI is configured to use one), and reports only those within
/// its scope.
pub struct SourceFinder<'runtime> {
    instance: ffi::FindInstance,
    scope: DiscoveryScope,
    _runtime: &'runtime NdiRuntime,
}

impl<'runtime> SourceFinder<'runtime> {
    pub fn new(runtime: &'runtime NdiRuntime, scope: DiscoveryScope) -> Result<Self, String> {
        let extra_ips = scope.extra_ips();
        let settings = ffi::FindCreate {
            show_local_sources: true,
            groups: ptr::null(),
            extra_ips: extra_ips.as_ref().map_or(ptr::null(), |ips| ips.as_ptr()),
        };
        // SAFETY: `settings` and the strings it points to outlive the call;
        // NDI copies them during creation. Null strings select NDI's defaults.
        let instance = unsafe { ffi::NDIlib_find_create_v2(&settings) };
        if instance.is_null() {
            return Err("NDI failed to create a source finder".to_string());
        }
        Ok(Self {
            instance,
            scope,
            _runtime: runtime,
        })
    }

    /// Waits up to `timeout` for the first source matching `query`.
    pub fn find_source(
        &self,
        query: &SourceQuery,
        timeout: Duration,
    ) -> Result<SourceDescriptor, String> {
        let deadline = Instant::now() + timeout;
        let mut seen_names = Vec::new();
        while Instant::now() < deadline {
            let sources = self.current_sources();
            seen_names = unique_names(&sources);
            if let Some(source) = sources.into_iter().find(|source| source.satisfies(query)) {
                return Ok(source);
            }
            self.wait_for_change(deadline);
        }
        Err(self.not_found_message(query, timeout, &seen_names))
    }

    /// Always waits the full `duration`: NDI cannot tell when every source on
    /// the network has answered.
    pub fn source_names_after(&self, duration: Duration) -> Vec<String> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.wait_for_change(deadline);
        }
        unique_names(&self.current_sources())
    }

    fn wait_for_change(&self, deadline: Instant) {
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis();
        let timeout_ms = u32::try_from(remaining_ms).unwrap_or(u32::MAX);
        // SAFETY: `instance` is a live finder.
        unsafe { ffi::NDIlib_find_wait_for_sources(self.instance, timeout_ms) };
    }

    fn current_sources(&self) -> Vec<SourceDescriptor> {
        let mut count = 0u32;
        // SAFETY: `instance` is a live finder; the returned array holds
        // `count` entries valid until the next call on this finder.
        let raw_sources =
            unsafe { ffi::NDIlib_find_get_current_sources(self.instance, &mut count) };
        if raw_sources.is_null() {
            return Vec::new();
        }
        // SAFETY: See above; entries are copied before the buffer can change.
        let raw_sources = unsafe { std::slice::from_raw_parts(raw_sources, count as usize) };
        raw_sources
            .iter()
            .filter_map(|raw| unsafe { SourceDescriptor::copy_from(raw) })
            .filter(|source| source.is_within(self.scope))
            .collect()
    }

    fn not_found_message(
        &self,
        query: &SourceQuery,
        timeout: Duration,
        seen_names: &[String],
    ) -> String {
        let location = self.scope.location_suffix();
        let not_found = format!("Did not find {query}{location} within {timeout:?}");
        if seen_names.is_empty() {
            return format!("{not_found}. No NDI sources were discovered{location}");
        }
        format!(
            "{not_found}. Sources discovered{location}: {}",
            seen_names.join(", ")
        )
    }
}

/// De-duplicated and sorted, because NDI can list one source more than once
/// (e.g. once per network interface).
fn unique_names(sources: &[SourceDescriptor]) -> Vec<String> {
    let names: BTreeSet<String> = sources.iter().map(SourceDescriptor::name).collect();
    names.into_iter().collect()
}

impl Drop for SourceFinder<'_> {
    fn drop(&mut self) {
        // SAFETY: `instance` came from NDIlib_find_create_v2 and is destroyed once.
        unsafe { ffi::NDIlib_find_destroy(self.instance) };
    }
}
