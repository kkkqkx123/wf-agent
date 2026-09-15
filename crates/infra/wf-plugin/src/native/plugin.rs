use std::ffi::CString;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::MiddlewarePhase;

use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::native::abi::{
    load_abi_info, ptr_to_string, ContributionRegistrarC, DispatchFn, PluginAbiResult,
    PluginContextC, WF_PLUGIN_ABI_VERSION,
};
use crate::plugin::Plugin;

pub struct NativePlugin {
    manifest: PluginManifest,
    _lib: Arc<libloading::Library>,
    abi: PluginAbiResult,
}

impl NativePlugin {
    pub fn new(manifest: PluginManifest, lib: libloading::Library) -> PluginResult<Self> {
        let abi = load_abi_info(&lib)?;
        let loaded_manifest: PluginManifest =
            toml::from_str(&String::from_utf8_lossy(&abi.manifest_bytes))
                .map_err(|e| PluginError::NativeError(format!("manifest parse: {}", e)))?;

        if manifest.id != loaded_manifest.id {
            return Err(PluginError::NativeError(format!(
                "manifest id mismatch: '{}' vs '{}'",
                manifest.id, loaded_manifest.id
            )));
        }

        Ok(Self {
            manifest,
            _lib: Arc::new(lib),
            abi,
        })
    }
}

#[async_trait]
impl Plugin for NativePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

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

    async fn on_config_change(&self, config: &serde_json::Value) -> PluginResult<()> {
        call_config_hook_fn(self.abi.on_config_change, self.manifest.id.as_str(), config)
    }

    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        let func = match self.abi.register_contributions {
            Some(f) => f,
            None => return Ok(()),
        };

        let dispatch = self.abi.dispatch_handler;

        let registrar_with_dispatch = RegistrarWithDispatch {
            registrar,
            dispatch,
            first_error: None,
        };
        let boxed = Box::new(registrar_with_dispatch);
        let context_ptr = Box::into_raw(boxed) as *mut std::ffi::c_void;

        let c_registrar = ContributionRegistrarC {
            abi_version: WF_PLUGIN_ABI_VERSION,
            context: context_ptr,
            register_node_type: Some(ffi_register_node_type),
            register_tool_type: Some(ffi_register_tool_type),
            register_llm_provider: Some(ffi_register_llm_provider),
            register_formatter: Some(ffi_register_formatter),
            register_event_handler: Some(ffi_register_event_handler),
            register_middleware: Some(ffi_register_middleware),
        };

        let result =
            func(&c_registrar as *const ContributionRegistrarC as *mut ContributionRegistrarC);
        let first_error = unsafe {
            let owned = Box::from_raw(context_ptr as *mut RegistrarWithDispatch);
            owned.first_error
        };
        if result != 0 {
            return Err(PluginError::NativeError(format!(
                "wf_plugin_register_contributions returned non-zero: {result}"
            )));
        }
        if let Some(e) = first_error {
            return Err(e);
        }
        Ok(())
    }
}

struct RegistrarWithDispatch<'a> {
    registrar: &'a mut dyn ContributionRegistrar,
    dispatch: Option<DispatchFn>,
    /// First contribution registration failure observed across the FFI
    /// calls of one `register_contributions` invocation. The C ABI reports
    /// per-call status codes, so the error is captured here and surfaced
    /// to the engine once the guest returns.
    first_error: Option<PluginError>,
}

impl RegistrarWithDispatch<'_> {
    fn record(&mut self, result: PluginResult<()>) -> i32 {
        match result {
            Ok(()) => 0,
            Err(e) => {
                if self.first_error.is_none() {
                    self.first_error = Some(e);
                }
                2
            }
        }
    }
}

unsafe fn registrar_from_ctx(
    ctx: *mut std::ffi::c_void,
) -> &'static mut RegistrarWithDispatch<'static> {
    &mut *(ctx as *mut RegistrarWithDispatch)
}

fn call_config_hook_fn(
    hook: Option<extern "C" fn(*const PluginContextC) -> i32>,
    plugin_id: &str,
    config: &serde_json::Value,
) -> PluginResult<()> {
    match hook {
        None => Ok(()),
        Some(func) => {
            let config_json = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
            let plugin_id_c = CString::new(plugin_id)
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
            let config_c = CString::new(config_json)
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
            let c_ctx = PluginContextC {
                abi_version: WF_PLUGIN_ABI_VERSION,
                plugin_id: plugin_id_c.as_ptr(),
                config_json: config_c.as_ptr(),
            };
            let result = func(&c_ctx as *const PluginContextC);
            if result != 0 {
                return Err(PluginError::NativeError(format!(
                    "on_config_change returned {}",
                    result
                )));
            }
            Ok(())
        }
    }
}

fn call_hook_fn(
    hook: Option<extern "C" fn(*const PluginContextC) -> i32>,
    ctx: &PluginContext,
) -> PluginResult<()> {
    match hook {
        None => Ok(()),
        Some(func) => {
            let config_json =
                serde_json::to_string(&ctx.config).unwrap_or_else(|_| "{}".to_string());
            let plugin_id_c = CString::new(ctx.plugin_id.as_str())
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
            let config_c = CString::new(config_json)
                .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;

            let c_ctx = PluginContextC {
                abi_version: WF_PLUGIN_ABI_VERSION,
                plugin_id: plugin_id_c.as_ptr(),
                config_json: config_c.as_ptr(),
            };

            let result = func(&c_ctx as *const PluginContextC);
            if result != 0 {
                return Err(PluginError::NativeError(format!(
                    "hook returned {}",
                    result
                )));
            }
            Ok(())
        }
    }
}

fn dispatch_call(
    dispatch: Option<DispatchFn>,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let func = match dispatch {
        Some(f) => f,
        None => {
            return Err(PluginError::NativeError(format!(
                "native plugin '{}' does not export wf_plugin_dispatch_handler",
                handler_name
            )))
        }
    };

    let c_type = CString::new(handler_type)
        .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
    let c_name = CString::new(handler_name)
        .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;
    let c_input = CString::new(input_json)
        .map_err(|e| PluginError::NativeError(format!("CString error: {}", e)))?;

    let mut buf: Vec<u8> = vec![0u8; 65536];
    let mut written: usize = buf.len();

    let result = func(
        c_type.as_ptr(),
        c_name.as_ptr(),
        c_input.as_ptr(),
        buf.as_mut_ptr(),
        &mut written as *mut usize,
    );

    if result != 0 {
        return Err(PluginError::NativeError(format!(
            "dispatch '{}' failed for '{}'",
            handler_type, handler_name
        )));
    }

    buf.truncate(written);
    Ok(buf)
}

// ============================================================
// FFI registration stubs — pass dispatch function to handlers
// ============================================================

extern "C" fn ffi_register_node_type(
    ctx_ptr: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
) -> i32 {
    if ctx_ptr.is_null() || name.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let name_str = unsafe { ptr_to_string(name) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_node_type(
        &name_str.clone(),
        Arc::new(NativeNodeHandler {
            type_name: name_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

extern "C" fn ffi_register_tool_type(
    ctx_ptr: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
) -> i32 {
    if ctx_ptr.is_null() || name.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let name_str = unsafe { ptr_to_string(name) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_tool_type(
        &name_str.clone(),
        Arc::new(NativeToolExecutor {
            type_name: name_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

extern "C" fn ffi_register_llm_provider(
    ctx_ptr: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
) -> i32 {
    if ctx_ptr.is_null() || name.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let name_str = unsafe { ptr_to_string(name) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_llm_provider(
        &name_str.clone(),
        Arc::new(NativeLlmFormatter {
            name: name_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

extern "C" fn ffi_register_formatter(
    ctx_ptr: *mut std::ffi::c_void,
    name: *const std::os::raw::c_char,
) -> i32 {
    if ctx_ptr.is_null() || name.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let name_str = unsafe { ptr_to_string(name) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_formatter(
        &name_str.clone(),
        Arc::new(NativeLlmFormatter {
            name: name_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

extern "C" fn ffi_register_event_handler(
    ctx_ptr: *mut std::ffi::c_void,
    event_type: *const std::os::raw::c_char,
) -> i32 {
    if ctx_ptr.is_null() || event_type.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let event_str = unsafe { ptr_to_string(event_type) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_event_handler(
        &event_str.clone(),
        Arc::new(NativeEventHandler {
            event_type: event_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

extern "C" fn ffi_register_middleware(
    ctx_ptr: *mut std::ffi::c_void,
    phase: *const std::os::raw::c_char,
    priority: i32,
) -> i32 {
    if ctx_ptr.is_null() || phase.is_null() {
        return 1;
    }
    let registrar_ctx = unsafe { registrar_from_ctx(ctx_ptr) };
    let phase_str = unsafe { ptr_to_string(phase) };
    let dispatch = registrar_ctx.dispatch;
    let result = registrar_ctx.registrar.register_middleware(
        MiddlewarePhase::from(phase_str.as_str()),
        priority,
        Arc::new(NativeMiddlewareHandler {
            phase: phase_str,
            dispatch,
        }),
    );
    registrar_ctx.record(result)
}

// ============================================================
// Handler structs with dispatch
// ============================================================

pub struct NativeNodeHandler {
    type_name: String,
    dispatch: Option<DispatchFn>,
}

pub struct NativeToolExecutor {
    type_name: String,
    dispatch: Option<DispatchFn>,
}

pub struct NativeLlmFormatter {
    name: String,
    dispatch: Option<DispatchFn>,
}

pub struct NativeEventHandler {
    event_type: String,
    dispatch: Option<DispatchFn>,
}

pub struct NativeMiddlewareHandler {
    phase: String,
    dispatch: Option<DispatchFn>,
}

#[async_trait]
impl PluginNodeHandler for NativeNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let input_json = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::NativeError(format!("serialize ctx: {}", e)))?;
        let output = dispatch_call(self.dispatch, "node", &self.type_name, &input_json)?;
        serde_json::from_slice::<PluginNodeResult>(&output)
            .map_err(|e| PluginError::NativeError(format!("deserialize result: {}", e)))
    }
}

#[async_trait]
impl PluginToolExecutor for NativeToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input_json = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::NativeError(format!("serialize ctx: {}", e)))?;
        let output = dispatch_call(self.dispatch, "tool", &self.type_name, &input_json)?;
        serde_json::from_slice::<PluginToolResult>(&output)
            .map_err(|e| PluginError::NativeError(format!("deserialize result: {}", e)))
    }
}

#[async_trait]
impl PluginLlmFormatter for NativeLlmFormatter {
    async fn format(&self, request: PluginLlmRequest) -> PluginResult<PluginLlmResponse> {
        let input_json = serde_json::to_string(&request)
            .map_err(|e| PluginError::NativeError(format!("serialize request: {}", e)))?;
        let output = dispatch_call(self.dispatch, "llm", &self.name, &input_json)?;
        serde_json::from_slice::<PluginLlmResponse>(&output)
            .map_err(|e| PluginError::NativeError(format!("deserialize result: {}", e)))
    }
}

#[async_trait]
impl PluginEventHandler for NativeEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input_json = serde_json::to_string(&event)
            .map_err(|e| PluginError::NativeError(format!("serialize event: {}", e)))?;
        dispatch_call(self.dispatch, "event", &self.event_type, &input_json)?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for NativeMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let ctx_json = serde_json::to_string(&context)
            .map_err(|e| PluginError::NativeError(format!("serialize context: {}", e)))?;
        let output = dispatch_call(self.dispatch, "mw", &self.phase, &ctx_json)?;
        // Middleware answers with a boolean (legacy) or a `{proceed,
        // context}` envelope; the rewritten context threads downstream.
        let response: Value = serde_json::from_slice(&output).unwrap_or(Value::Null);
        let outcome = parse_middleware_outcome(&response, &context);
        if outcome.proceed {
            next(outcome.context).await
        } else {
            Ok(outcome.context)
        }
    }
}
