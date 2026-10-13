//! Raw bindings to the subset of the NDI C API used by this utility.
//!
//! Layouts mirror `Processing.NDI.structs.h`, `Processing.NDI.Find.h` and
//! `Processing.NDI.Recv.h` from NDI SDK v6. C unions whose members share a
//! size are represented by their first (current) member.

use std::ffi::{c_char, c_float, c_int};

pub type FindInstance = *mut std::ffi::c_void;
pub type RecvInstance = *mut std::ffi::c_void;

pub const FRAME_TYPE_VIDEO: c_int = 1;
pub const FRAME_TYPE_ERROR: c_int = 4;

pub const RECV_COLOR_FORMAT_UYVY_BGRA: c_int = 1;
pub const RECV_BANDWIDTH_HIGHEST: c_int = 100;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub ndi_name: *const c_char,
    pub url_address: *const c_char,
}

#[repr(C)]
pub struct FindCreate {
    pub show_local_sources: bool,
    pub groups: *const c_char,
    pub extra_ips: *const c_char,
}

#[repr(C)]
pub struct RecvCreateV3 {
    pub source_to_connect_to: Source,
    pub color_format: c_int,
    pub bandwidth: c_int,
    pub allow_video_fields: bool,
    pub ndi_recv_name: *const c_char,
}

#[repr(C)]
pub struct VideoFrameV2 {
    pub xres: c_int,
    pub yres: c_int,
    pub four_cc: u32,
    pub frame_rate_n: c_int,
    pub frame_rate_d: c_int,
    pub picture_aspect_ratio: c_float,
    pub frame_format_type: c_int,
    pub timecode: i64,
    pub data: *mut u8,
    pub line_stride_in_bytes: c_int,
    pub metadata: *const c_char,
    pub timestamp: i64,
}

unsafe extern "C" {
    pub fn NDIlib_initialize() -> bool;
    pub fn NDIlib_destroy();

    pub fn NDIlib_find_create_v2(settings: *const FindCreate) -> FindInstance;
    pub fn NDIlib_find_destroy(instance: FindInstance);
    pub fn NDIlib_find_wait_for_sources(instance: FindInstance, timeout_in_ms: u32) -> bool;
    pub fn NDIlib_find_get_current_sources(
        instance: FindInstance,
        source_count: *mut u32,
    ) -> *const Source;

    pub fn NDIlib_recv_create_v3(settings: *const RecvCreateV3) -> RecvInstance;
    pub fn NDIlib_recv_destroy(instance: RecvInstance);
    pub fn NDIlib_recv_capture_v2(
        instance: RecvInstance,
        video: *mut VideoFrameV2,
        audio: *mut std::ffi::c_void,
        metadata: *mut std::ffi::c_void,
        timeout_in_ms: u32,
    ) -> c_int;
    pub fn NDIlib_recv_free_video_v2(instance: RecvInstance, video: *const VideoFrameV2);
}
