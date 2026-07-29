use std::ffi::CStr;
use std::os::raw::c_char;

use crate::error::PluginResult;

#[repr(C)]
pub struct PluginContextC {
    pub plugin_id: *const c_char,
    pub config_json: *const c_char,
}

#[repr(C)]
pub struct ContributionRegistrarC {
    pub context: *mut std::ffi::c_void,
    pub register_node_type: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_tool_type: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_llm_provider: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_formatter: Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_event_handler: Option<extern "C" fn(ctx: *mut std::ffi::c_void, event_type: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_hook_handler: Option<extern "C" fn(ctx: *mut std::ffi::c_void, hook_type: *const c_char, payload_json: *const c_char) -> i32>,
    pub register_middleware: Option<extern "C" fn(ctx: *mut std::ffi::c_void, phase: *const c_char, priority: i32, payload_json: *const c_char) -> i32>,
}

pub fn load_manifest(
    get_manifest: extern "C" fn(*mut u8, *mut usize) -> i32,
) -> PluginResult<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut len: usize = buf.capacity();
    let result = get_manifest(buf.as_mut_ptr(), &mut len as *mut usize);
    if result != 0 {
        return Err(crate::error::PluginError::NativeError("wf_plugin_get_manifest failed".into()));
    }
    unsafe { buf.set_len(len); }
    Ok(buf)
}

pub fn load_abi_info(lib: &libloading::Library) -> PluginResult<PluginAbiResult> {
    let get_manifest: libloading::Symbol<extern "C" fn(*mut u8, *mut usize) -> i32> = unsafe {
        lib.get(b"wf_plugin_get_manifest")
            .map_err(|e| crate::error::PluginError::NativeError(format!("missing wf_plugin_get_manifest: {}", e)))?
    };
    let manifest_bytes = load_manifest(*get_manifest)?;

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

    Ok(PluginAbiResult {
        manifest_bytes,
        on_load,
        register_contributions,
        on_activate,
        on_deactivate,
        on_unload,
    })
}

#[derive(Debug)]
pub struct PluginAbiResult {
    pub manifest_bytes: Vec<u8>,
    pub on_load: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub register_contributions: Option<extern "C" fn(*mut ContributionRegistrarC) -> i32>,
    pub on_activate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_deactivate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_unload: Option<extern "C" fn(*const PluginContextC) -> i32>,
}

/// # Safety
/// `ptr` must be a valid null-terminated C string pointer.
pub unsafe fn ptr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}
