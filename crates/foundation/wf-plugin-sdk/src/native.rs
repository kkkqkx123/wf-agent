//! Native plugin C ABI v1 definitions (author-visible contract).
//!
//! Data structures and constants only — host-side loading logic
//! (`load_manifest`, `load_abi_info`, symbol resolution) stays in the host
//! crate `wf-plugin`. Plugin authors consume these types through the
//! `export_plugin!` macro (SDK-1) rather than implementing the ABI by hand.

use std::os::raw::c_char;

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
    pub register_node_type:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_tool_type:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_llm_provider:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_formatter:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_event_handler:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, event_type: *const c_char) -> i32>,
    pub register_middleware: Option<
        extern "C" fn(ctx: *mut std::ffi::c_void, phase: *const c_char, priority: i32) -> i32,
    >,
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
