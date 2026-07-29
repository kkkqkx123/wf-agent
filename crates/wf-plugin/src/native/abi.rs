use std::ffi::CStr;
use std::os::raw::c_char;

use crate::error::PluginResult;

pub const WF_PLUGIN_ABI_VERSION: u32 = 1;

#[repr(C)]
pub struct PluginContextC {
    pub abi_version: u32,
    pub plugin_id: *const c_char,
    pub config_json: *const c_char,
}

#[repr(C)]
pub struct ContributionRegistrarC {
    pub abi_version: u32,
    pub context: *mut std::ffi::c_void,
    pub register_node_type: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_tool_type: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_llm_provider: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_formatter: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_event_handler: Option<extern "C" fn(ctx: *mut std::ffi::c_void, event_type: *const c_char) -> i32>,
    pub register_hook_handler: Option<extern "C" fn(ctx: *mut std::ffi::c_void, hook_type: *const c_char) -> i32>,
    pub register_middleware: Option<extern "C" fn(ctx: *mut std::ffi::c_void, phase: *const c_char, priority: i32) -> i32>,
}

/// Host dispatch function: calls a registered handler by type and name.
/// Returns 0 on success, non-zero on error.
/// `output_len` is in/out: on input it holds buffer capacity, on output it holds bytes written.
pub type DispatchFn = extern "C" fn(
    handler_type: *const c_char,
    handler_name: *const c_char,
    input_json: *const c_char,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32;

/// Safe two-phase manifest loading: query size first, then fill buffer.
pub fn load_manifest(
    get_manifest: extern "C" fn(*mut u8, *mut usize) -> i32,
) -> PluginResult<Vec<u8>> {
    let mut len: usize = 0;
    let result = get_manifest(std::ptr::null_mut(), &mut len as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError("wf_plugin_get_manifest failed (size query)".into()));
    }
    if len == 0 {
        return Err(crate::error::PluginError::NativeError("wf_plugin_get_manifest returned zero size".into()));
    }

    let mut buf: Vec<u8> = vec![0u8; len];
    let mut written: usize = buf.len();
    let result = get_manifest(buf.as_mut_ptr(), &mut written as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError("wf_plugin_get_manifest failed (fill)".into()));
    }
    buf.truncate(written);
    Ok(buf)
}

pub fn load_abi_info(lib: &libloading::Library) -> PluginResult<PluginAbiResult> {
    let get_manifest: libloading::Symbol<extern "C" fn(*mut u8, *mut usize) -> i32> = unsafe {
        lib.get(b"wf_plugin_get_manifest")
            .map_err(|e| crate::error::PluginError::NativeError(format!("missing wf_plugin_get_manifest: {}", e)))?
    };
    let manifest_bytes = load_manifest(*get_manifest)?;

    check_abi_version(lib)?;

    let dispatch_handler: Option<DispatchFn> = unsafe {
        lib.get(b"wf_plugin_dispatch_handler").ok().map(|s: libloading::Symbol<DispatchFn>| *s)
    };

    let on_load: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_load").ok().map(|s: libloading::Symbol<_>| *s)
    };
    let register_contributions: Option<extern "C" fn(*mut ContributionRegistrarC) -> i32> = unsafe {
        lib.get(b"wf_plugin_register_contributions").ok().map(|s: libloading::Symbol<_>| *s)
    };
    let on_activate: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_activate").ok().map(|s: libloading::Symbol<_>| *s)
    };
    let on_deactivate: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_deactivate").ok().map(|s: libloading::Symbol<_>| *s)
    };
    let on_unload: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_unload").ok().map(|s: libloading::Symbol<_>| *s)
    };
    let on_config_change: Option<extern "C" fn(*const PluginContextC) -> i32> = unsafe {
        lib.get(b"wf_plugin_on_config_change").ok().map(|s: libloading::Symbol<_>| *s)
    };

    Ok(PluginAbiResult {
        manifest_bytes,
        dispatch_handler,
        on_load,
        register_contributions,
        on_activate,
        on_deactivate,
        on_unload,
        on_config_change,
    })
}

fn check_abi_version(lib: &libloading::Library) -> PluginResult<()> {
    let version_fn: Option<libloading::Symbol<extern "C" fn() -> u32>> = unsafe {
        lib.get(b"wf_plugin_abi_version").ok()
    };
    match version_fn {
        Some(sym) => {
            let version = sym();
            if version != WF_PLUGIN_ABI_VERSION {
                Err(crate::error::PluginError::NativeError(
                    format!("ABI version mismatch: plugin={}, host={}", version, WF_PLUGIN_ABI_VERSION)
                ))
            } else {
                Ok(())
            }
        }
        None => Err(crate::error::PluginError::NativeError(
            "plugin does not export wf_plugin_abi_version (host ABI requires >= 1)".into()
        )),
    }
}

#[derive(Debug, Clone)]
pub struct PluginAbiResult {
    pub manifest_bytes: Vec<u8>,
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


