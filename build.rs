//! Links against the NDI runtime shipped inside the NDI SDK.
//!
//! The SDK is not installed system-wide, so the library directory is added to
//! the link search path. The binary's rpath points at the same directory,
//! unless `NDI_RUNTIME_DIR` names the directory the runtime is installed to
//! (the Debian package bundles it privately, see `packaging/`).

const DEFAULT_SDK_DIR: &str = "/home/pi/NDI SDK for Linux";
const SDK_LIBRARY_SUBDIR: &str = "lib/aarch64-rpi4-linux-gnueabi";

fn main() {
    println!("cargo:rerun-if-env-changed=NDI_SDK_DIR");
    println!("cargo:rerun-if-env-changed=NDI_RUNTIME_DIR");

    let sdk_dir = std::env::var("NDI_SDK_DIR").unwrap_or_else(|_| DEFAULT_SDK_DIR.to_string());
    let library_dir = format!("{sdk_dir}/{SDK_LIBRARY_SUBDIR}");
    let runtime_dir = std::env::var("NDI_RUNTIME_DIR").unwrap_or_else(|_| library_dir.clone());

    println!("cargo:rustc-link-search=native={library_dir}");
    println!("cargo:rustc-link-lib=dylib=ndi");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{runtime_dir}");
}
