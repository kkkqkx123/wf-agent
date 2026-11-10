//! Native plugin author-side wrapper over the C ABI v1 contract.
//!
//! A plugin author implements [`WfNativePlugin`] on a type and exports it
//! with [`crate::export_plugin!`]; the macro generates every `extern "C"` symbol the
//! host loader resolves (`wf_plugin_supported_abi_versions`, `wf_plugin_get_manifest`,
//! `wf_plugin_on_load`, `wf_plugin_register_contributions`,
//! `wf_plugin_on_activate`, `wf_plugin_on_deactivate`, `wf_plugin_on_unload`,
//! `wf_plugin_on_config_change`, `wf_plugin_dispatch_handler`). The macro
//! must be invoked exactly once per cdylib crate.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::native::{ContributionRegistrarC, PluginContextC, WF_ABI_VERSION_V1};
use crate::{
    PluginCliCommand, PluginCompletionSource, PluginMarkdownRule, PluginSettingField,
    PluginToolSummary,
};

/// Author-side registration handle passed to [`WfNativePlugin::register`].
/// Recorded entries are forwarded into the host registrar; name entries wrap
/// dispatch handlers that call back into the generated
/// `wf_plugin_dispatch_handler`, JSON entries carry declarative CLI payloads
/// validated by the host, so no handler closures are needed here.
/// One recorded contribution: the first C string argument travels in
/// `first`, the optional second C string argument in `second`, and the
/// middleware priority in `priority`.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeRecord {
    pub kind: String,
    pub first: String,
    pub second: String,
    pub priority: i32,
}

pub struct NativeRegistrar {
    names: Mutex<Vec<NativeRecord>>,
}

impl NativeRegistrar {
    pub fn new() -> Self {
        Self {
            names: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, kind: &str, first: &str) {
        self.names
            .lock()
            .expect("registrar poisoned")
            .push(NativeRecord {
                kind: kind.to_owned(),
                first: first.to_owned(),
                second: String::new(),
                priority: 0,
            });
    }

    fn record_double(&self, kind: &str, first: &str, second: &str) {
        self.names
            .lock()
            .expect("registrar poisoned")
            .push(NativeRecord {
                kind: kind.to_owned(),
                first: first.to_owned(),
                second: second.to_owned(),
                priority: 0,
            });
    }

    fn record_json(&self, kind: &str, value: &serde_json::Value) {
        if let Ok(json) = serde_json::to_string(value) {
            self.record(kind, &json);
        }
    }

    pub fn register_node_type(&self, name: &str) {
        self.record("node-type", name);
    }

    pub fn register_tool_type(&self, name: &str) {
        self.record("tool-type", name);
    }

    pub fn register_llm_provider(&self, name: &str) {
        self.record("llm-provider", name);
    }

    /// Register a provider authentication implementation; dispatched on
    /// the `llm-auth` handler type with ops `describe` / `login_start` /
    /// `login_poll` / `login_submit` / `refresh` / `resolve`.
    pub fn register_llm_auth(&self, name: &str) {
        self.record("llm-auth", name);
    }

    /// Register an image generation implementation; dispatched on the
    /// `llm-images` handler type with op `generate_images`.
    pub fn register_llm_images(&self, name: &str) {
        self.record("llm-images", name);
    }

    /// Register a declarative provider definition as generic JSON.
    pub fn register_llm_provider_definition(&self, definition: &serde_json::Value) {
        self.record_json("llm-provider-definition", definition);
    }

    /// Register a declarative tool definition as generic JSON.
    pub fn register_tool(&self, tool: &serde_json::Value) {
        self.record_json("tool", tool);
    }

    /// Register a declarative tool description as generic JSON.
    pub fn register_tool_description(&self, description: &serde_json::Value) {
        self.record_json("tool-description", description);
    }

    /// Register a declarative workflow as generic JSON.
    pub fn register_workflow(&self, workflow: &serde_json::Value) {
        self.record_json("workflow", workflow);
    }

    /// Register a declarative prompt template as generic JSON.
    pub fn register_prompt(&self, prompt: &serde_json::Value) {
        self.record_json("prompt", prompt);
    }

    /// Register a declarative prompt fragment as generic JSON.
    pub fn register_fragment(&self, fragment: &serde_json::Value) {
        self.record_json("fragment", fragment);
    }

    /// Register a declarative agent template as generic JSON.
    pub fn register_agent_template(&self, agent: &serde_json::Value) {
        self.record_json("agent-template", agent);
    }

    /// Register a declarative node template as generic JSON.
    pub fn register_node_template(&self, node: &serde_json::Value) {
        self.record_json("node-template", node);
    }

    /// Register a declarative trigger as generic JSON.
    pub fn register_trigger(&self, trigger: &serde_json::Value) {
        self.record_json("trigger", trigger);
    }

    /// Register compaction advice; dispatched on `compaction`.
    pub fn register_compaction_advisor(&self, name: &str) {
        self.record("compaction-advisor", name);
    }

    /// Register a full-text analyzer; dispatched on `text-analyzer`.
    pub fn register_text_analyzer(&self, name: &str) {
        self.record("text-analyzer", name);
    }

    /// Register retrieval shaping advice; dispatched on `retrieval`.
    pub fn register_retrieval_advisor(&self, name: &str) {
        self.record("retrieval-advisor", name);
    }

    /// Register a vector encoding; dispatched on `vector-encoding`.
    pub fn register_vector_encoding(&self, name: &str) {
        self.record("vector-encoding", name);
    }

    /// Register a snapshot policy document under `name`.
    pub fn register_snapshot_policy(&self, name: &str, policy: &serde_json::Value) {
        if let Ok(json) = serde_json::to_string(policy) {
            self.record_double("snapshot-policy", name, &json);
        }
    }

    pub fn register_event_handler(&self, event_type: &str) {
        self.record("event-handler", event_type);
    }

    pub fn register_middleware(&self, phase: &str, priority: i32) {
        self.names
            .lock()
            .expect("registrar poisoned")
            .push(NativeRecord {
                kind: "middleware".to_owned(),
                first: phase.to_owned(),
                second: String::new(),
                priority,
            });
    }

    pub fn register_cli_command(&self, command: &PluginCliCommand) {
        if let Ok(json) = serde_json::to_string(command) {
            self.record("cli-command", &json);
        }
    }

    pub fn register_completion_source(&self, source: &PluginCompletionSource) {
        if let Ok(json) = serde_json::to_string(source) {
            self.record("completion-provider", &json);
        }
    }

    pub fn register_setting_field(&self, field: &PluginSettingField) {
        if let Ok(json) = serde_json::to_string(field) {
            self.record("setting-field", &json);
        }
    }

    pub fn register_tool_summary(&self, summary: &PluginToolSummary) {
        if let Ok(json) = serde_json::to_string(summary) {
            self.record("tool-summary", &json);
        }
    }

    pub fn register_markdown_rule(&self, rule: &PluginMarkdownRule) {
        if let Ok(json) = serde_json::to_string(rule) {
            self.record("markdown-transformer", &json);
        }
    }

    /// All recorded contributions in registration order.
    pub fn recorded(&self) -> Vec<NativeRecord> {
        self.names.lock().expect("registrar poisoned").clone()
    }
}

impl Default for NativeRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

/// Author-side plugin trait. Lifecycle hooks return `PluginResult`; an `Err`
/// is translated by the generated `extern "C"` symbols into a non-zero code,
/// which the host reports as a native hook failure. Panics inside hooks are
/// caught and reported the same way.
pub trait WfNativePlugin: Send + Sync + 'static {
    /// Construct the plugin instance. Called by the generated `on_load`
    /// before the author hook runs.
    fn new() -> Self
    where
        Self: Sized;

    fn manifest() -> PluginManifest
    where
        Self: Sized;

    fn on_load(&self, _config: &serde_json::Value) -> PluginResult<()> {
        Ok(())
    }

    fn on_activate(&self, _config: &serde_json::Value) -> PluginResult<()> {
        Ok(())
    }

    fn on_deactivate(&self) -> PluginResult<()> {
        Ok(())
    }

    fn on_unload(&self) -> PluginResult<()> {
        Ok(())
    }

    fn on_config_change(&self, _config: &serde_json::Value) -> PluginResult<()> {
        Ok(())
    }

    /// Called when the host invokes a registered handler by (kind, name).
    /// `input` is the JSON the host passed in; the returned JSON is written
    /// back to the host output buffer.
    fn dispatch(
        &self,
        _kind: &str,
        _name: &str,
        _input: &serde_json::Value,
    ) -> PluginResult<serde_json::Value> {
        Err(PluginError::NativeError(
            "plugin does not implement dispatch".into(),
        ))
    }

    /// Declare contributions on the author-side registrar.
    fn register(registrar: &NativeRegistrar)
    where
        Self: Sized;
}

/// Per-cdylib singleton holding the plugin instance between `on_load` and
/// `on_unload`. The host drives hooks sequentially, so a Mutex suffices.
#[doc(hidden)]
pub struct PluginState<P> {
    instance: Mutex<Option<P>>,
}

unsafe impl<P: Send> Sync for PluginState<P> {}

impl<P: WfNativePlugin> PluginState<P> {
    #[doc(hidden)]
    pub const fn new() -> Self {
        Self {
            instance: Mutex::new(None),
        }
    }

    /// Install the plugin instance. Called by the macro-generated `on_load`
    /// export; `#[doc(hidden)]` because it is macro machinery, not author API.
    #[doc(hidden)]
    pub fn set(&self, plugin: P) -> i32 {
        let mut slot = self.instance.lock().expect("plugin state poisoned");
        if slot.is_some() {
            return 1;
        }
        *slot = Some(plugin);
        0
    }

    /// Drop the plugin instance. Called by the macro-generated
    /// `on_load`/`on_unload` exports; `#[doc(hidden)]` as above.
    #[doc(hidden)]
    pub fn clear(&self) {
        *self.instance.lock().expect("plugin state poisoned") = None;
    }

    fn with<R>(&self, f: impl FnOnce(&P) -> PluginResult<R>) -> PluginResult<R> {
        let slot = self.instance.lock().expect("plugin state poisoned");
        let plugin = slot
            .as_ref()
            .ok_or_else(|| PluginError::NativeError("plugin not loaded".into()))?;
        f(plugin)
    }
}

/// # Safety
/// `ctx` must point to a valid `PluginContextC` with valid C strings.
unsafe fn context_config(ctx: *const PluginContextC) -> PluginResult<serde_json::Value> {
    let ctx = unsafe {
        ctx.as_ref()
            .ok_or_else(|| PluginError::NativeError("null plugin context".into()))?
    };
    if ctx.abi_version != WF_ABI_VERSION_V1 {
        return Err(PluginError::NativeError(format!(
            "context ABI version mismatch: plugin={}, host={}",
            ctx.abi_version, WF_ABI_VERSION_V1
        )));
    }
    let raw: String = if ctx.config_json.is_null() {
        "{}".to_owned()
    } else {
        unsafe { CStr::from_ptr(ctx.config_json) }
            .to_string_lossy()
            .into_owned()
    };
    serde_json::from_str(&raw)
        .map_err(|e| PluginError::NativeError(format!("config JSON parse: {}", e)))
}

fn catch_panic<T>(f: impl FnOnce() -> PluginResult<T>) -> PluginResult<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(PluginError::PluginPanic {
            plugin_id: "native".into(),
        })
    })
}

fn code_from_result(result: PluginResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("wf-plugin-sdk: hook failed: {e}");
            1
        }
    }
}

#[doc(hidden)]
pub mod __private {
    use super::*;

    /// Per-cdylib singleton holding the plugin instance between `on_load`
    /// and `on_unload`. The host drives hooks sequentially, so a Mutex
    /// suffices. Re-exported at the crate root for `export_plugin!`.
    /// Generated `on_load`/`on_activate`/`on_config_change` body.
    ///
    /// # Safety
    /// `ctx` must be null or point to a valid `PluginContextC` whose string
    /// fields are valid NUL-terminated C strings living at least for the
    /// call. The host guarantees this for every ABI entry call.
    pub unsafe fn run_config_hook<P, F>(
        state: &PluginState<P>,
        ctx: *const PluginContextC,
        f: F,
    ) -> i32
    where
        P: WfNativePlugin,
        F: FnOnce(&P, serde_json::Value) -> PluginResult<()>,
    {
        let config = match catch_panic(|| unsafe { context_config(ctx) }) {
            Ok(c) => c,
            _ => return 1,
        };
        code_from_result(catch_panic(|| state.with(|p| f(p, config))))
    }

    /// Generated `on_deactivate`/`on_unload` body.
    pub fn run_plain_hook<P, F>(state: &PluginState<P>, f: F) -> i32
    where
        P: WfNativePlugin,
        F: FnOnce(&P) -> PluginResult<()>,
    {
        code_from_result(catch_panic(|| state.with(f)))
    }

    /// Generated `wf_plugin_register_contributions` body: run the author's
    /// `register` and forward recorded names into the host registrar.
    ///
    /// # Safety
    /// `host` must be null or point to a valid `ContributionRegistrarC`
    /// whose callback pointers, when set, are callable with the given
    /// context pointer. The host guarantees this for the ABI entry call.
    pub unsafe fn forward_registrations<P: WfNativePlugin>(
        host: *mut ContributionRegistrarC,
    ) -> i32 {
        let registrar = NativeRegistrar::new();
        if catch_panic(|| {
            P::register(&registrar);
            Ok(())
        })
        .is_err()
        {
            return 1;
        }

        let Some(host) = (unsafe { host.as_ref() }) else {
            return 1;
        };
        for record in registrar.recorded() {
            let Ok(c_first) = CString::new(record.first) else {
                return 1;
            };
            let Ok(c_second) = CString::new(record.second) else {
                return 1;
            };
            let first_ptr = c_first.as_ptr();
            let second_ptr = c_second.as_ptr();
            let ctx = host.context;
            let priority = record.priority;
            let ok = match record.kind.as_str() {
                "node-type" => host.register_node_type.map(|f| f(ctx, first_ptr)),
                "tool-type" => host.register_tool_type.map(|f| f(ctx, first_ptr)),
                "llm-provider" => host.register_llm_provider.map(|f| f(ctx, first_ptr)),
                "llm-auth" => host.register_llm_auth.map(|f| f(ctx, first_ptr)),
                "llm-images" => host.register_llm_images.map(|f| f(ctx, first_ptr)),
                "llm-provider-definition" => host
                    .register_llm_provider_definition
                    .map(|f| f(ctx, first_ptr)),
                "tool" => host.register_tool.map(|f| f(ctx, first_ptr)),
                "tool-description" => host.register_tool_description.map(|f| f(ctx, first_ptr)),
                "workflow" => host.register_workflow.map(|f| f(ctx, first_ptr)),
                "prompt" => host.register_prompt.map(|f| f(ctx, first_ptr)),
                "fragment" => host.register_fragment.map(|f| f(ctx, first_ptr)),
                "agent-template" => host.register_agent_template.map(|f| f(ctx, first_ptr)),
                "node-template" => host.register_node_template.map(|f| f(ctx, first_ptr)),
                "trigger" => host.register_trigger.map(|f| f(ctx, first_ptr)),
                "compaction-advisor" => host.register_compaction_advisor.map(|f| f(ctx, first_ptr)),
                "text-analyzer" => host.register_text_analyzer.map(|f| f(ctx, first_ptr)),
                "retrieval-advisor" => host.register_retrieval_advisor.map(|f| f(ctx, first_ptr)),
                "vector-encoding" => host.register_vector_encoding.map(|f| f(ctx, first_ptr)),
                "snapshot-policy" => host
                    .register_snapshot_policy
                    .map(|f| f(ctx, first_ptr, second_ptr)),
                "event-handler" => host.register_event_handler.map(|f| f(ctx, first_ptr)),
                "middleware" => host
                    .register_middleware
                    .map(|f| f(ctx, first_ptr, priority)),
                "cli-command" => host.register_cli_command.map(|f| f(ctx, first_ptr)),
                "completion-provider" => host.register_completion_source.map(|f| f(ctx, first_ptr)),
                "setting-field" => host.register_setting_field.map(|f| f(ctx, first_ptr)),
                "tool-summary" => host.register_tool_summary.map(|f| f(ctx, first_ptr)),
                "markdown-transformer" => host.register_markdown_rule.map(|f| f(ctx, first_ptr)),
                _ => None,
            };
            if matches!(ok, Some(code) if code != 0) {
                return 1;
            }
        }
        0
    }

    /// Generated `wf_plugin_get_manifest` body: `out == null` means size
    /// query, otherwise copy up to `*len` bytes and report bytes written.
    ///
    /// # Safety
    /// `out`/`len` must be null or point to valid writable memory: `len`
    /// always, `out` for at least `*len` bytes when non-null. The host
    /// guarantees this for the ABI entry call.
    pub unsafe fn write_manifest_bytes<P: WfNativePlugin>(out: *mut u8, len: *mut usize) -> i32 {
        let bytes = match catch_panic(|| {
            toml::to_string_pretty(&P::manifest())
                .map_err(|e| PluginError::NativeError(format!("manifest serialize: {}", e)))
        }) {
            Ok(s) => s.into_bytes(),
            _ => return 1,
        };
        unsafe {
            if out.is_null() {
                *len = bytes.len();
                return 0;
            }
            if *len < bytes.len() {
                *len = bytes.len();
                return 2;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            *len = bytes.len();
        }
        0
    }

    /// Generated `wf_plugin_dispatch_handler` body: parse host input, call
    /// the author's `dispatch`, serialize the result into the output buffer.
    ///
    /// # Safety
    /// The input pointers must be null or point to valid NUL-terminated C
    /// strings; `output_len` must be null or point to valid writable memory
    /// and `output_buf` must be null or writable for at least `*output_len`
    /// bytes. The host guarantees this for the ABI entry call.
    pub unsafe fn dispatch_into_buffer<P: WfNativePlugin>(
        state: &PluginState<P>,
        handler_type: *const c_char,
        handler_name: *const c_char,
        input_json: *const c_char,
        output_buf: *mut u8,
        output_len: *mut usize,
    ) -> i32 {
        if output_len.is_null() {
            return 1;
        }
        let read_str = |p: *const c_char| -> Option<String> {
            if p.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
            }
        };
        let (Some(kind), Some(name)) = (read_str(handler_type), read_str(handler_name)) else {
            return 1;
        };
        let input_raw = read_str(input_json).unwrap_or_else(|| "null".to_owned());
        let input: serde_json::Value = match serde_json::from_str(&input_raw) {
            Ok(v) => v,
            Err(_) => return 1,
        };

        let output = match catch_panic(|| state.with(|p| p.dispatch(&kind, &name, &input))) {
            Ok(v) => v,
            _ => return 1,
        };
        let bytes = match serde_json::to_vec(&output) {
            Ok(b) => b,
            Err(_) => return 1,
        };
        unsafe {
            if *output_len < bytes.len() {
                *output_len = bytes.len();
                return 2;
            }
            if !output_buf.is_null() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), output_buf, bytes.len());
            }
            *output_len = bytes.len();
        }
        0
    }
}

/// Generate host-facing `extern "C"` exports for a [`WfNativePlugin`]
/// implementation. Invoke once per cdylib crate:
///
/// ```ignore
/// struct MyPlugin;
/// impl WfNativePlugin for MyPlugin { /* ... */ }
/// wf_plugin_sdk::export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        const _: fn() = || {
            fn assert_impl<P: $crate::WfNativePlugin>() {}
            assert_impl::<$plugin>();
        };

        static WF_PLUGIN_SDK_STATE: $crate::PluginState<$plugin> = $crate::PluginState::new();

        #[no_mangle]
        pub extern "C" fn wf_plugin_supported_abi_versions(
            out_versions: *mut u32,
            out_len: *mut usize,
        ) -> i32 {
            const SUPPORTED: [u32; 1] = [$crate::native::WF_ABI_VERSION_V1];
            unsafe {
                if out_versions.is_null() {
                    *out_len = SUPPORTED.len();
                    return 0;
                }
                if *out_len < SUPPORTED.len() {
                    *out_len = SUPPORTED.len();
                    return 2;
                }
                std::ptr::copy_nonoverlapping(SUPPORTED.as_ptr(), out_versions, SUPPORTED.len());
                *out_len = SUPPORTED.len();
            }
            0
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_get_manifest(out: *mut u8, len: *mut usize) -> i32 {
            unsafe { $crate::__private::write_manifest_bytes::<$plugin>(out, len) }
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_on_load(ctx: *const $crate::native::PluginContextC) -> i32 {
            // Ensure a clean slot: clear any leftover state from a previous
            // load that failed before reaching on_unload.
            WF_PLUGIN_SDK_STATE.clear();
            // Construct + install the instance, then run the author hook.
            match WF_PLUGIN_SDK_STATE.set(<$plugin as $crate::WfNativePlugin>::new()) {
                0 => {}
                _ => return 1,
            }
            unsafe {
                $crate::__private::run_config_hook(&WF_PLUGIN_SDK_STATE, ctx, |p, config| {
                    p.on_load(&config)
                })
            }
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_on_activate(ctx: *const $crate::native::PluginContextC) -> i32 {
            unsafe {
                $crate::__private::run_config_hook(&WF_PLUGIN_SDK_STATE, ctx, |p, config| {
                    p.on_activate(&config)
                })
            }
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_on_deactivate(
            _ctx: *const $crate::native::PluginContextC,
        ) -> i32 {
            $crate::__private::run_plain_hook(&WF_PLUGIN_SDK_STATE, |p| p.on_deactivate())
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_on_unload(_ctx: *const $crate::native::PluginContextC) -> i32 {
            let code = $crate::__private::run_plain_hook(&WF_PLUGIN_SDK_STATE, |p| p.on_unload());
            WF_PLUGIN_SDK_STATE.clear();
            code
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_on_config_change(
            ctx: *const $crate::native::PluginContextC,
        ) -> i32 {
            unsafe {
                $crate::__private::run_config_hook(&WF_PLUGIN_SDK_STATE, ctx, |p, config| {
                    p.on_config_change(&config)
                })
            }
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_register_contributions(
            host: *mut $crate::native::ContributionRegistrarC,
        ) -> i32 {
            unsafe { $crate::__private::forward_registrations::<$plugin>(host) }
        }

        #[no_mangle]
        pub extern "C" fn wf_plugin_dispatch_handler(
            handler_type: *const std::os::raw::c_char,
            handler_name: *const std::os::raw::c_char,
            input_json: *const std::os::raw::c_char,
            output_buf: *mut u8,
            output_len: *mut usize,
        ) -> i32 {
            unsafe {
                $crate::__private::dispatch_into_buffer(
                    &WF_PLUGIN_SDK_STATE,
                    handler_type,
                    handler_name,
                    input_json,
                    output_buf,
                    output_len,
                )
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    // The macro-generated exports share a single static `WF_PLUGIN_SDK_STATE`.
    // Tests that touch it must run sequentially to avoid cross-test
    // interference on the singleton instance slot.
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock poisoned")
    }

    struct DemoPlugin;

    impl WfNativePlugin for DemoPlugin {
        fn new() -> Self {
            DemoPlugin
        }

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: "demo".into(),
                version: "0.1.0".into(),
                name: None,
                description: None,
                plugin_type: Some(crate::manifest::PluginType::Native),
                entry_point: "libdemo.so".into(),
                versioned_entry_points: Default::default(),
                dependencies: Default::default(),
                optional_dependencies: Default::default(),
                contributions: vec![],
                permissions: vec![],
                config_schema: None,
                config: None,
                hooks: None,
                llm_providers: vec![],
                tools: vec![],
                tool_descriptions: vec![],
                cli_commands: vec![],
                completion_sources: vec![],
                setting_fields: vec![],
                tool_summaries: vec![],
                markdown_rules: vec![],
                wasm: None,
                lua: None,
            }
        }

        fn on_load(&self, config: &serde_json::Value) -> PluginResult<()> {
            if config.get("fail").and_then(|v| v.as_bool()) == Some(true) {
                return Err(PluginError::Internal("requested failure".into()));
            }
            Ok(())
        }

        fn dispatch(
            &self,
            kind: &str,
            name: &str,
            input: &serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Ok(json!({ "kind": kind, "name": name, "echo": input }))
        }

        fn register(registrar: &NativeRegistrar) {
            registrar.register_tool_type("demo_tool");
            registrar.register_node_type("demo_node");
        }
    }
    crate::export_plugin!(DemoPlugin);

    fn make_ctx(config: &serde_json::Value) -> (CString, CString, PluginContextC) {
        let plugin_id = CString::new("demo").unwrap();
        let config_json = CString::new(config.to_string()).unwrap();
        let ctx = PluginContextC {
            abi_version: WF_ABI_VERSION_V1,
            plugin_id: plugin_id.as_ptr(),
            config_json: config_json.as_ptr(),
        };
        (plugin_id, config_json, ctx)
    }

    #[test]
    fn supported_abi_versions_match_contract() {
        let mut len: usize = 0;
        assert_eq!(
            wf_plugin_supported_abi_versions(std::ptr::null_mut(), &mut len),
            0
        );
        assert_eq!(len, 1);
        let mut buf = vec![0u32; len];
        let mut written = buf.len();
        assert_eq!(
            wf_plugin_supported_abi_versions(buf.as_mut_ptr(), &mut written),
            0
        );
        assert_eq!(written, 1);
        buf.truncate(written);
        assert_eq!(buf, vec![WF_ABI_VERSION_V1]);
    }

    #[test]
    fn manifest_export_two_phase() {
        // Phase 1: size query with a null out buffer.
        let mut len: usize = 0;
        let code = wf_plugin_get_manifest(std::ptr::null_mut(), &mut len);
        assert_eq!(code, 0);
        assert!(len > 0);

        // Phase 2: fill and parse back as TOML.
        let mut buf = vec![0u8; len];
        let mut written = buf.len();
        let code = wf_plugin_get_manifest(buf.as_mut_ptr(), &mut written);
        assert_eq!(code, 0);
        buf.truncate(written);
        let manifest: PluginManifest = toml::from_str(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(manifest.id, "demo");
    }

    #[test]
    fn lifecycle_hooks_round_trip() {
        let _guard = lock();
        // Bind the backing strings: `ctx` borrows them, discarding would
        // leave dangling pointers (use-after-free).
        let (_id, _cfg, ctx) = make_ctx(&json!({}));
        assert_eq!(wf_plugin_on_load(&ctx), 0);
        assert_eq!(wf_plugin_on_activate(&ctx), 0);
        assert_eq!(wf_plugin_on_deactivate(&ctx), 0);

        // Dispatch works while loaded.
        let kind = CString::new("tool-type").unwrap();
        let name = CString::new("demo_tool").unwrap();
        let input = CString::new(json!({"x": 1}).to_string()).unwrap();
        let mut buf = vec![0u8; 4096];
        let mut written = buf.len();
        let code = wf_plugin_dispatch_handler(
            kind.as_ptr(),
            name.as_ptr(),
            input.as_ptr(),
            buf.as_mut_ptr(),
            &mut written,
        );
        assert_eq!(code, 0);
        buf.truncate(written);
        let out: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(out["name"], "demo_tool");
        assert_eq!(out["echo"]["x"], 1);

        assert_eq!(wf_plugin_on_unload(&ctx), 0);
    }

    #[test]
    fn failing_hook_reports_nonzero() {
        let _guard = lock();
        let (_id, _cfg, ctx) = make_ctx(&json!({ "fail": true }));
        assert_eq!(wf_plugin_on_load(&ctx), 1);
        // Reset state for other tests: on_load failed but the instance was
        // installed; unload clears it.
        let (_ok_id, _ok_cfg, ok_ctx) = make_ctx(&json!({}));
        wf_plugin_on_unload(&ok_ctx);
    }

    #[test]
    fn dispatch_before_load_fails() {
        let _guard = lock();
        let kind = CString::new("tool-type").unwrap();
        let name = CString::new("demo_tool").unwrap();
        let input = CString::new("null").unwrap();
        let mut written = 0usize;
        let code = wf_plugin_dispatch_handler(
            kind.as_ptr(),
            name.as_ptr(),
            input.as_ptr(),
            std::ptr::null_mut(),
            &mut written,
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn registrar_records_contributions() {
        let registrar = NativeRegistrar::new();
        DemoPlugin::register(&registrar);
        let recorded = registrar.recorded();
        assert_eq!(
            recorded
                .iter()
                .map(|r| (r.kind.as_str(), r.first.as_str()))
                .collect::<Vec<_>>(),
            vec![("tool-type", "demo_tool"), ("node-type", "demo_node")]
        );
    }

    #[test]
    fn registrar_records_llm_auth_contribution() {
        let registrar = NativeRegistrar::new();
        registrar.register_llm_auth("claude-subscription");
        let recorded = registrar.recorded();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].kind, "llm-auth");
        assert_eq!(recorded[0].first, "claude-subscription");
    }

    #[test]
    fn registrar_records_middleware_priority_and_snapshot_policy() {
        let registrar = NativeRegistrar::new();
        registrar.register_middleware("pre", 7);
        registrar.register_snapshot_policy("nightly", &json!({"expect": 1}));
        registrar.register_tool(&json!({"id": "t"}));
        let recorded = registrar.recorded();
        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[0].kind, "middleware");
        assert_eq!(recorded[0].first, "pre");
        assert_eq!(recorded[0].priority, 7);
        assert_eq!(recorded[1].kind, "snapshot-policy");
        assert_eq!(recorded[1].first, "nightly");
        assert!(recorded[1].second.contains("expect"));
        assert_eq!(recorded[2].kind, "tool");
        assert!(recorded[2].first.contains("\"id\""));
    }

    #[test]
    fn header_mirror_covers_new_registrar_symbols() {
        let header = include_str!("../c-api/wf_plugin.h");
        for symbol in [
            "register_llm_images",
            "register_llm_provider_definition",
            "register_workflow",
            "register_prompt",
            "register_fragment",
            "register_agent_template",
            "register_node_template",
            "register_trigger",
            "register_compaction_advisor",
            "register_text_analyzer",
            "register_retrieval_advisor",
            "register_vector_encoding",
            "register_snapshot_policy",
            "WfContributionRegistrar",
            "wf_plugin_dispatch_handler",
        ] {
            assert!(header.contains(symbol), "header missing {symbol}");
        }
    }

    #[test]
    fn forward_registrations_requires_host() {
        assert_eq!(
            unsafe {
                crate::plugin::__private::forward_registrations::<DemoPlugin>(std::ptr::null_mut())
            },
            1
        );
    }
}
