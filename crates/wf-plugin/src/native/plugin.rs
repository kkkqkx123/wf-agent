use std::ffi::CString;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::native::abi::{load_abi_info, ptr_to_string, ContributionRegistrarC, PluginAbiResult, PluginContextC};
use crate::plugin::Plugin;

pub struct NativePlugin {
    manifest: PluginManifest,
    _lib: Arc<libloading::Library>,
    abi: PluginAbiResult,
}

impl NativePlugin {
    pub fn new(manifest: PluginManifest, lib: libloading::Library) -> PluginResult<Self> {
        let abi = load_abi_info(&lib)?;
        let loaded_manifest: PluginManifest = toml::from_str(
            &String::from_utf8_lossy(&abi.manifest_bytes)
        ).map_err(|e| PluginError::NativeError(format!("manifest parse: {}", e)))?;

        if manifest.id != loaded_manifest.id {
            return Err(PluginError::NativeError(
                format!("manifest id mismatch: '{}' vs '{}'", manifest.id, loaded_manifest.id)
            ));
        }

        Ok(Self { manifest, _lib: Arc::new(lib), abi })
    }
}

#[async_trait]
impl Plugin for NativePlugin {
    fn manifest(&self) -> &PluginManifest { &self.manifest }

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        call_hook_fn(self.abi.on_load, ctx)
    }

    async fn on_unload(&self, ctx: &PluginContext) -> PluginResult<()> {
        call_hook_fn(self.abi.on_unload, ctx)
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        call_hook_fn(self.abi.on_activate, ctx)
    }

    async fn on_deactivate(&self, ctx: &PluginContext) -> PluginResult<()> {
        call_hook_fn(self.abi.on_deactivate, ctx)
    }

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
        let func = match self.abi.register_contributions {
            Some(f) => f,
            None => return,
        };

        let erased: *mut (dyn ContributionRegistrar + 'static) = unsafe {
            let raw: *mut dyn ContributionRegistrar = registrar;
            std::mem::transmute(raw)
        };
        let boxed = Box::new(RegistrarFatPtr(erased));
        let context_ptr = Box::into_raw(boxed) as *mut std::ffi::c_void;

        let c_registrar = ContributionRegistrarC {
            context: context_ptr,
            register_node_type: Some(ffi_register_node_type),
            register_tool_type: Some(ffi_register_tool_type),
            register_llm_provider: Some(ffi_register_llm_provider),
            register_formatter: Some(ffi_register_formatter),
            register_event_handler: Some(ffi_register_event_handler),
            register_hook_handler: Some(ffi_register_hook_handler),
            register_middleware: Some(ffi_register_middleware),
        };

        let result = func(&c_registrar as *const ContributionRegistrarC as *mut ContributionRegistrarC);
        unsafe { drop(Box::from_raw(context_ptr as *mut RegistrarFatPtr)); }
        if result != 0 {
            tracing::warn!("wf_plugin_register_contributions returned non-zero: {}", result);
        }
    }
}

struct RegistrarFatPtr(*mut dyn ContributionRegistrar);

fn call_hook_fn(hook: Option<extern "C" fn(*const PluginContextC) -> i32>, ctx: &PluginContext) -> PluginResult<()> {
    match hook {
        None => Ok(()),
        Some(func) => {
            let config_json = serde_json::to_string(&ctx.config).unwrap_or_else(|_| "{}".to_string());
            let plugin_id_c = CString::new(ctx.plugin_id.as_str())
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
            let config_c = CString::new(config_json)
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;

            let c_ctx = PluginContextC {
                plugin_id: plugin_id_c.as_ptr(),
                config_json: config_c.as_ptr(),
            };

            let result = func(&c_ctx as *const PluginContextC);
            if result != 0 {
                return Err(PluginError::NativeError(format!("hook returned {}", result)));
            }
            Ok(())
        }
    }
}

unsafe fn registrar_from_ctx(ctx: *mut std::ffi::c_void) -> &'static mut dyn ContributionRegistrar {
    let fat = &mut *(ctx as *mut RegistrarFatPtr);
    &mut *fat.0
}

extern "C" fn ffi_register_node_type(
    ctx: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || name.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let name_str = unsafe { ptr_to_string(name) };
    registrar.register_node_type(&name_str, Arc::new(NativeNodeHandler { type_name: name_str.clone() }));
    0
}

extern "C" fn ffi_register_tool_type(
    ctx: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || name.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let name_str = unsafe { ptr_to_string(name) };
    registrar.register_tool_type(&name_str, Arc::new(NativeToolExecutor { type_name: name_str.clone() }));
    0
}

extern "C" fn ffi_register_llm_provider(
    ctx: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || name.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let name_str = unsafe { ptr_to_string(name) };
    registrar.register_llm_provider(&name_str, Arc::new(NativeLLMFormatter { name: name_str.clone() }));
    0
}

extern "C" fn ffi_register_formatter(
    ctx: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || name.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let name_str = unsafe { ptr_to_string(name) };
    registrar.register_formatter(&name_str, Arc::new(NativeLLMFormatter { name: name_str.clone() }));
    0
}

extern "C" fn ffi_register_event_handler(
    ctx: *mut std::ffi::c_void,
    event_type: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || event_type.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let event_str = unsafe { ptr_to_string(event_type) };
    registrar.register_event_handler(&event_str, Arc::new(NativeEventHandler { event_type: event_str.clone() }));
    0
}

extern "C" fn ffi_register_hook_handler(
    ctx: *mut std::ffi::c_void,
    hook_type: *const std::os::raw::c_char,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || hook_type.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let hook_str = unsafe { ptr_to_string(hook_type) };
    registrar.register_hook_handler(&hook_str, Arc::new(NativeHookHandler { hook_type: hook_str.clone() }));
    0
}

extern "C" fn ffi_register_middleware(
    ctx: *mut std::ffi::c_void,
    phase: *const std::os::raw::c_char,
    priority: i32,
    _payload: *const std::os::raw::c_char,
) -> i32 {
    if ctx.is_null() || phase.is_null() { return 1; }
    let registrar = unsafe { registrar_from_ctx(ctx) };
    let phase_str = unsafe { ptr_to_string(phase) };
    registrar.register_middleware(&phase_str, priority, Arc::new(NativeMiddlewareHandler { phase: phase_str.clone() }));
    0
}

pub struct NativeNodeHandler { type_name: String }
pub struct NativeToolExecutor { type_name: String }
pub struct NativeLLMFormatter { name: String }
pub struct NativeEventHandler { event_type: String }
pub struct NativeHookHandler { hook_type: String }
pub struct NativeMiddlewareHandler { phase: String }

#[async_trait]
impl PluginNodeHandler for NativeNodeHandler {
    async fn execute(&self, _ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        Err(PluginError::NativeError(format!("node type '{}' stub", self.type_name)))
    }
}

#[async_trait]
impl PluginToolExecutor for NativeToolExecutor {
    async fn execute(&self, _ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        Err(PluginError::NativeError(format!("tool type '{}' stub", self.type_name)))
    }
}

#[async_trait]
impl PluginLLMFormatter for NativeLLMFormatter {
    async fn format(&self, _request: PluginLLMRequest) -> PluginResult<PluginLLMResponse> {
        Err(PluginError::NativeError(format!("llm formatter '{}' stub", self.name)))
    }
}

#[async_trait]
impl PluginEventHandler for NativeEventHandler {
    async fn handle(&self, _event: PluginEventData) -> PluginResult<()> {
        Err(PluginError::NativeError(format!("event handler '{}' stub", self.event_type)))
    }
}

#[async_trait]
impl PluginHookHandler for NativeHookHandler {
    async fn handle(&self, _context: Value) -> PluginResult<()> {
        Err(PluginError::NativeError(format!("hook handler '{}' stub", self.hook_type)))
    }
}

#[async_trait]
impl PluginMiddlewareHandler for NativeMiddlewareHandler {
    async fn handle(&self, _context: Value, _next: Box<dyn FnOnce() -> PluginResult<()> + Send>) -> PluginResult<()> {
        Err(PluginError::NativeError(format!("middleware '{}' stub", self.phase)))
    }
}
