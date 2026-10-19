use std::ffi::CStr;
use std::os::raw::c_char;

use crate::error::PluginResult;

// ABI v1 constants and data structures moved to `wf-plugin-sdk`; re-exported
// here so existing `wf_plugin::native::abi` paths stay valid. Host-side
// loading logic (manifest retrieval, symbol resolution, version check)
// remains in this module.
pub use wf_plugin_sdk::native::{
    ContributionRegistrarC, DispatchFn, PluginContextC, WF_ABI_VERSION_V1,
    WF_SUPPORTED_ABI_VERSIONS,
};

/// Safe two-phase manifest loading: query size first, then fill buffer.
pub fn load_manifest(
    get_manifest: extern "C" fn(*mut u8, *mut usize) -> i32,
) -> PluginResult<Vec<u8>> {
    let mut len: usize = 0;
    let result = get_manifest(std::ptr::null_mut(), &mut len as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError(
            "wf_plugin_get_manifest failed (size query)".into(),
        ));
    }
    if len == 0 {
        return Err(crate::error::PluginError::NativeError(
            "wf_plugin_get_manifest returned zero size".into(),
        ));
    }

    let mut buf: Vec<u8> = vec![0u8; len];
    let mut written: usize = buf.len();
    let result = get_manifest(buf.as_mut_ptr(), &mut written as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError(
            "wf_plugin_get_manifest failed (fill)".into(),
        ));
    }
    buf.truncate(written);
    Ok(buf)
}

pub fn load_abi_info(lib: &libloading::Library) -> PluginResult<PluginAbiResult> {
    let get_manifest: libloading::Symbol<extern "C" fn(*mut u8, *mut usize) -> i32> = unsafe {
        lib.get(b"wf_plugin_get_manifest").map_err(|e| {
            crate::error::PluginError::NativeError(format!("missing wf_plugin_get_manifest: {}", e))
        })?
    };
    let manifest_bytes = load_manifest(*get_manifest)?;

    let negotiated_abi_version = check_abi_version(lib)?;

    let dispatch_handler: Option<DispatchFn> = unsafe {
        lib.get(b"wf_plugin_dispatch_handler")
            .ok()
            .map(|s: libloading::Symbol<DispatchFn>| *s)
    };

    let on_load: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_load")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };
    let register_contributions: Option<extern "C" fn(*mut ContributionRegistrarC) -> i32> = unsafe {
        lib.get(b"wf_plugin_register_contributions")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };
    let on_activate: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_activate")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };
    let on_deactivate: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_deactivate")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };
    let on_unload: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_unload")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };
    let on_config_change: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_config_change")
            .ok()
            .map(|s: libloading::Symbol<_>| *s)
    };

    Ok(PluginAbiResult {
        manifest_bytes,
        negotiated_abi_version,
        dispatch_handler,
        on_load,
        register_contributions,
        on_activate,
        on_deactivate,
        on_unload,
        on_config_change,
    })
}

fn check_abi_version(lib: &libloading::Library) -> PluginResult<u32> {
    let supported_fn: Option<libloading::Symbol<extern "C" fn(*mut u32, *mut usize) -> i32>> =
        unsafe { lib.get(b"wf_plugin_supported_abi_versions").ok() };
    let Some(sym) = supported_fn else {
        return Err(crate::error::PluginError::NativeError(
            "plugin does not export wf_plugin_supported_abi_versions; it must declare the ABI major versions it supports. Update the plugin to implement the current handshake.".into(),
        ));
    };
    check_supported_abi_versions(*sym)
}

fn check_supported_abi_versions(
    supported_fn: extern "C" fn(*mut u32, *mut usize) -> i32,
) -> PluginResult<u32> {
    let mut len: usize = 0;
    let result = supported_fn(std::ptr::null_mut(), &mut len as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError(
            "wf_plugin_supported_abi_versions failed (size query)".into(),
        ));
    }
    if len == 0 {
        return Err(crate::error::PluginError::NativeError(
            "plugin exported empty supported abi versions list".into(),
        ));
    }
    let mut buf = vec![0u32; len];
    let mut written = buf.len();
    let result = supported_fn(buf.as_mut_ptr(), &mut written as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError(
            "wf_plugin_supported_abi_versions failed (fill)".into(),
        ));
    }
    buf.truncate(written);
    let mut common: Vec<u32> = buf
        .iter()
        .copied()
        .filter(|v| WF_SUPPORTED_ABI_VERSIONS.contains(v))
        .collect();
    common.sort_unstable();
    match common.pop() {
        Some(version) => Ok(version),
        None => Err(crate::error::PluginError::NativeError(format!(
            "plugin supports abi versions {buf:?}, host supports {WF_SUPPORTED_ABI_VERSIONS:?}; \
             no compatible version found. Update the plugin or upgrade the host."
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct PluginAbiResult {
    pub manifest_bytes: Vec<u8>,
    pub negotiated_abi_version: u32,
    pub dispatch_handler: Option<DispatchFn>,
    pub on_load: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub register_contributions: Option<extern "C" fn(*mut ContributionRegistrarC) -> i32>,
    pub on_activate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_deactivate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_unload: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_config_change: Option<extern "C" fn(*const PluginContextC) -> i32>,
}

/// # Safety
/// `ptr` must be a valid null-terminated C string pointer.
pub unsafe fn ptr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}
