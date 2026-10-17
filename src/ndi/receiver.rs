use std::ptr;

use super::ffi;
use super::finder::SourceDescriptor;
use super::runtime::NdiRuntime;

/// A connection to one NDI source. Its frames are taken through a `FrameSync`.
pub struct Receiver<'runtime> {
    instance: ffi::RecvInstance,
    _runtime: &'runtime NdiRuntime,
}

impl<'runtime> Receiver<'runtime> {
    pub fn connect(
        runtime: &'runtime NdiRuntime,
        source: &SourceDescriptor,
    ) -> Result<Self, String> {
        let settings = ffi::RecvCreateV3 {
            source_to_connect_to: source.as_raw(),
            // UYVY is what NDI decodes to natively and what SDL can upload
            // without conversion; BGRA only appears for sources with alpha.
            color_format: ffi::RECV_COLOR_FORMAT_UYVY_BGRA,
            bandwidth: ffi::RECV_BANDWIDTH_HIGHEST,
            allow_video_fields: false,
            ndi_recv_name: ptr::null(),
        };
        // SAFETY: `settings` and the source strings outlive the call; NDI
        // copies them during creation.
        let instance = unsafe { ffi::NDIlib_recv_create_v3(&settings) };
        if instance.is_null() {
            return Err(format!(
                "NDI failed to create a receiver for {}",
                source.name()
            ));
        }
        Ok(Self {
            instance,
            _runtime: runtime,
        })
    }

    pub(super) fn instance(&self) -> ffi::RecvInstance {
        self.instance
    }
}

impl Drop for Receiver<'_> {
    fn drop(&mut self) {
        // SAFETY: `instance` came from NDIlib_recv_create_v3 and is destroyed
        // once; the borrow checker has already dropped the frame-sync using it.
        unsafe { ffi::NDIlib_recv_destroy(self.instance) };
    }
}
