use super::ffi;

/// Proof that the NDI library is initialized. Finders and receivers borrow it
/// so the library cannot be torn down while they are alive.
pub struct NdiRuntime {
    _private: (),
}

impl NdiRuntime {
    pub fn initialize() -> Result<Self, String> {
        // SAFETY: NDIlib_initialize has no preconditions.
        let initialized = unsafe { ffi::NDIlib_initialize() };
        if initialized {
            Ok(Self { _private: () })
        } else {
            Err("NDI failed to initialize; this CPU may be unsupported".to_string())
        }
    }
}

impl Drop for NdiRuntime {
    fn drop(&mut self) {
        // SAFETY: Paired with the successful NDIlib_initialize in `initialize`.
        unsafe { ffi::NDIlib_destroy() };
    }
}
