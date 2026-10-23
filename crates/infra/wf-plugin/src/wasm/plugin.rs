use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde_json::Value;
use wasmtime::Engine;
use wf_plugin_sdk::wasm::{export, WasmContributionDecl};
use wf_types::MiddlewarePhase;

use super::abi;
use super::policy::WasmLimits;
use super::pool::{self, PooledSession, SessionPool};
use super::stats::WasmStats;
use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::contributions::NextFn;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub mod abi_call;
pub mod codec;
pub mod handlers;
pub mod runtime;

pub use runtime::WasmPlugin;

pub(crate) use abi_call::*;
pub(crate) use codec::*;
pub(crate) use handlers::*;
pub(crate) use runtime::*;

pub use super::policy;
pub use super::stats;
pub use super::stdio;

#[cfg(test)]
mod tests {
    use super::super::policy::{resolve_limits, WasiGrants};
    use super::*;
    use crate::manifest::PluginType;

    #[tokio::test]
    async fn stats_count_calls_and_fuel() {
        let wat = super::super::loader::wasm_test_echo_wat(r#"{"tool_types":["stat_tool"]}"#);
        let bytes = wat::parse_str(&wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
            id: "stat".into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            entry_point: "plugin.wasm".into(),
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
            wasm: None,
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
        let engine_clone = engine.clone();
        let grants = WasiGrants::default();
        let stats = Arc::new(WasmStats::default());
        let pool = SessionPool::new(
            "stat",
            &engine_clone,
            &pre,
            &grants,
            &limits,
            &stats,
            module.get_export(export::HEAP_RESET).is_some(),
        );
        let inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: RwLock::new(Default::default()),
            stats,
            pool,
        };
        let decl = fetch_declaration(&inner).await.expect("decl");
        *wf_common::lock::write_ok(inner.decl.write()) = decl;
        let plugin = WasmPlugin::from_inner(inner);

        let ctx = PluginContext {
            plugin_id: "stat".into(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(crate::contributions::ContributionManager::new()),
        };
        plugin.on_load(&ctx).await.expect("hook");
        let executor = PluginToolExecutorBridge(&plugin, "stat_tool");
        executor.call().await.expect("dispatch");

        let snap = plugin.stats();
        assert_eq!(snap.calls, 3);
        assert_eq!(snap.ok, 3);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.timeouts, 0);
        assert!(snap.fuel_consumed > 0, "metered calls consume fuel");
    }

    struct PluginToolExecutorBridge<'a>(&'a WasmPlugin, &'a str);

    impl PluginToolExecutorBridge<'_> {
        async fn call(&self) -> PluginResult<PluginToolResult> {
            let handler = WasmToolExecutor {
                inner: self.0.inner.clone(),
                type_name: self.1.to_owned(),
            };
            handler
                .execute(PluginToolContext {
                    args: serde_json::json!({}),
                })
                .await
        }
    }

    /// Build a `WasmPlugin` directly from WAT plus a pool size, mirroring
    /// the loader path (probe reset export, build pool, fetch declaration).
    async fn build_pooled_plugin(id: &str, wat: &str, pool_size: u32) -> WasmPlugin {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = wat::parse_str(wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
            id: id.into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            entry_point: "plugin.wasm".into(),
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
            wasm: Some(WasmConfig {
                store_pool_size: Some(pool_size),
                ..Default::default()
            }),
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
        let grants = WasiGrants::default();
        let stats = Arc::new(WasmStats::default());
        let pool = SessionPool::new(
            id,
            &engine,
            &pre,
            &grants,
            &limits,
            &stats,
            module.get_export(export::HEAP_RESET).is_some(),
        );
        let inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: RwLock::new(Default::default()),
            stats,
            pool,
        };
        let decl = fetch_declaration(&inner).await.expect("decl");
        *wf_common::lock::write_ok(inner.decl.write()) = decl;
        WasmPlugin::from_inner(inner)
    }

    fn hook_context(plugin_id: &str) -> PluginContext {
        PluginContext {
            plugin_id: plugin_id.to_owned(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(crate::contributions::ContributionManager::new()),
        }
    }

    #[tokio::test]
    async fn pool_reuses_session_after_successful_reset() {
        use super::super::loader::wasm_test_echo_reset_wat;

        let wat = wasm_test_echo_reset_wat(r#"{"tool_types":["pooled_tool"]}"#, 0);
        let plugin = build_pooled_plugin("pooled", &wat, 2).await;
        assert!(plugin.inner.pool.enabled());
        // The load-time register call already returned its session.
        assert_eq!(plugin.stats().pool_misses, 1);

        let ctx = hook_context("pooled");
        plugin.on_load(&ctx).await.expect("first hook");
        assert_eq!(plugin.stats().pool_hits, 1);
        PluginToolExecutorBridge(&plugin, "pooled_tool")
            .call()
            .await
            .expect("dispatch on reused session");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 2);
        assert_eq!(snap.pool_misses, 1);
        assert_eq!(snap.pool_drops, 0);
        assert_eq!(snap.calls, 3);
        assert_eq!(snap.ok, 3);
    }

    #[tokio::test]
    async fn pool_stays_disabled_without_reset_export() {
        use super::super::loader::wasm_test_echo_wat;

        let wat = wasm_test_echo_wat(r#"{"tool_types":[]}"#);
        let plugin = build_pooled_plugin("noreset", &wat, 2).await;
        assert!(!plugin.inner.pool.enabled());

        let ctx = hook_context("noreset");
        plugin.on_load(&ctx).await.expect("hook");
        plugin.on_load(&ctx).await.expect("hook");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 0);
        assert_eq!(snap.pool_misses, 0);
        assert_eq!(snap.pool_drops, 0);
        assert_eq!(snap.calls, 3);
    }

    #[tokio::test]
    async fn refused_reset_discards_session() {
        use super::super::loader::wasm_test_echo_reset_wat;

        let wat = wasm_test_echo_reset_wat(r#"{"tool_types":[]}"#, 1);
        let plugin = build_pooled_plugin("refuse", &wat, 2).await;
        assert!(plugin.inner.pool.enabled());
        // Load-time register already refused once.
        assert_eq!(plugin.stats().pool_drops, 1);

        plugin
            .on_load(&hook_context("refuse"))
            .await
            .expect("hook still succeeds");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 0);
        assert_eq!(snap.pool_drops, 2);
        assert_eq!(snap.ok, snap.calls);
    }

    #[tokio::test]
    async fn failed_call_discards_session() {
        let wat = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{\"tool_types\":[\"t\"]}")
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_heap_reset") (result i32)
    (global.set $heap (i32.const 4096))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 20)) (i64.const 32))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (unreachable)))
"#;
        let plugin = build_pooled_plugin("trap", wat, 2).await;
        // Register succeeded, so one session was pooled at load.
        assert_eq!(plugin.stats().pool_drops, 0);

        let err = PluginToolExecutorBridge(&plugin, "t")
            .call()
            .await
            .expect_err("trapping dispatch must fail");
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected wasm error, got {err:?}"
        );
        let snap = plugin.stats();
        assert_eq!(
            snap.pool_hits, 1,
            "failed call checked out the idle session"
        );
        assert_eq!(snap.pool_drops, 1, "post-trap session is discarded");
        assert_eq!(snap.failed, 1);
    }

    const ABI_PROBE_BASE_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32) (i32.const 0))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32)))))
"#;

    /// Build one live session from WAT and run the ABI version probe.
    async fn negotiate_probe(wat: &str) -> PluginResult<u32> {
        let bytes = wat::parse_str(wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
            id: "abi-probe".into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            entry_point: "plugin.wasm".into(),
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
            wasm: None,
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
        let grants = WasiGrants::default();
        let mut session = pool::new_session(&engine, &pre, "abi-probe", &grants, &limits).await?;
        super::abi::negotiate_abi_version(&mut session, &engine, "abi-probe", &limits).await
    }

    fn with_supported_versions(base: &str, versions: &[u32]) -> String {
        let bytes: Vec<u8> = versions.iter().flat_map(|v| v.to_le_bytes()).collect();
        let escaped: String = bytes.iter().map(|b| format!("\\{b:02X}")).collect();
        let count = versions.len();
        let stripped = base.trim_end().strip_suffix(')').expect("wat ends");
        format!(
            "{stripped}\n  (data (i32.const 16384) \"{escaped}\")\n  (func (export \"wf_supported_abi_versions\") (result i64)\n    (i64.or (i64.extend_i32_u (i32.const 16384)) (i64.shl (i64.extend_i32_u (i32.const {count})) (i64.const 32))))\n)\n"
        )
    }

    fn without_supported_export(base: &str) -> String {
        base.replace(
            "  (func (export \"wf_supported_abi_versions\") (result i64)\n    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))",
            "",
        )
        .replace("  (data (i32.const 8192) \"\\01\\00\\00\\00\")", "")
    }

    #[tokio::test]
    async fn abi_version_supported_v1_accepts() {
        negotiate_probe(ABI_PROBE_BASE_WAT)
            .await
            .expect("v1 is compatible");
    }

    #[tokio::test]
    async fn abi_version_missing_fails_load() {
        let wat = without_supported_export(ABI_PROBE_BASE_WAT);
        let err = negotiate_probe(&wat)
            .await
            .expect_err("missing export must fail");
        assert!(
            err.to_string().contains("wf_supported_abi_versions"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn abi_version_mismatch_fails_load() {
        let wat = with_supported_versions(&without_supported_export(ABI_PROBE_BASE_WAT), &[99]);
        let err = negotiate_probe(&wat).await.expect_err("v99 must fail");
        assert!(
            err.to_string().contains("no compatible version"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn abi_version_wrong_signature_fails_load() {
        let base = without_supported_export(ABI_PROBE_BASE_WAT);
        let stripped = base.trim_end().strip_suffix(')').expect("wat ends");
        let wat = format!(
            "{stripped}\n  (func (export \"wf_supported_abi_versions\") (param i32) (result i64) (i64.extend_i32_u (local.get 0)))\n)\n"
        );
        let err = negotiate_probe(&wat)
            .await
            .expect_err("wrong signature must fail");
        assert!(
            err.to_string().contains("unexpected signature"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn abi_version_empty_list_fails_load() {
        let wat = with_supported_versions(&without_supported_export(ABI_PROBE_BASE_WAT), &[]);
        let err = negotiate_probe(&wat)
            .await
            .expect_err("empty list must fail");
        assert!(err.to_string().contains("empty"), "got: {err}");
    }

    const HOOK_FAIL_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "bad config")
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32) (i32.const 7))
  (func (export "wf_last_error") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 10)) (i64.const 32)))))
"#;

    const HOOK_FAIL_NO_DETAIL_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32) (i32.const 7)))
"#;

    #[tokio::test]
    async fn hook_failure_carries_last_error_detail() {
        let plugin = build_pooled_plugin("detail", HOOK_FAIL_WAT, 0).await;
        let err = plugin
            .on_load(&hook_context("detail"))
            .await
            .expect_err("hook code 7 must fail");
        let text = err.to_string();
        assert!(text.contains("detail"), "got: {text}");
        assert!(text.contains("code 7"), "got: {text}");
        assert!(text.contains("bad config"), "got: {text}");
    }

    #[tokio::test]
    async fn hook_failure_without_last_error_has_code_only() {
        let plugin = build_pooled_plugin("nocode-detail", HOOK_FAIL_NO_DETAIL_WAT, 0).await;
        let err = plugin
            .on_load(&hook_context("nocode-detail"))
            .await
            .expect_err("hook code 7 must fail");
        let text = err.to_string();
        assert!(text.contains("code 7"), "got: {text}");
    }

    /// Guest whose `wf_dispatch` always answers with `response_json`.
    /// Used to drive middleware envelope cases without a real chain.
    fn mw_response_wat(response_json: &str) -> String {
        let escaped = response_json.replace('\\', "\\\\").replace('"', "\\\"");
        let len = response_json.len();
        format!(
            r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{{}}")
  (data (i32.const 1024) "{escaped}")
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 2)) (i64.const 32))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (i64.or (i64.extend_i32_u (i32.const 1024)) (i64.shl (i64.extend_i32_u (i32.const {len})) (i64.const 32)))))
"#
        )
    }

    async fn drive_middleware(
        id: &str,
        response_json: &str,
        input: Value,
        next: NextFn,
    ) -> PluginResult<Value> {
        let plugin = build_pooled_plugin(id, &mw_response_wat(response_json), 0).await;
        let handler = WasmMiddlewareHandler {
            inner: plugin.inner.clone(),
            phase: "pre_tool".to_owned(),
        };
        handler.handle(input, next).await
    }

    #[tokio::test]
    async fn middleware_envelope_rewrites_downstream_context() {
        use std::sync::Mutex;

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_next = seen.clone();
        let next: NextFn = Box::new(move |ctx| {
            seen_next.lock().expect("lock").push(ctx.clone());
            Box::pin(async move { Ok(ctx) })
        });
        let out = drive_middleware(
            "mw-rewrite",
            r#"{"proceed":true,"context":{"patched":1}}"#,
            serde_json::json!({"a": 1}),
            next,
        )
        .await
        .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"patched": 1}));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![serde_json::json!({"patched": 1})]
        );
    }

    #[tokio::test]
    async fn middleware_envelope_short_circuits_chain() {
        let next: NextFn = Box::new(|_| {
            Box::pin(async move {
                panic!("short-circuited chain must not call next");
            })
        });
        let out = drive_middleware(
            "mw-stop",
            r#"{"proceed":false,"context":{"stopped":true}}"#,
            serde_json::json!({"a": 1}),
            next,
        )
        .await
        .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"stopped": true}));
    }

    #[tokio::test]
    async fn middleware_legacy_bool_still_proceeds() {
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = drive_middleware("mw-legacy", "true", serde_json::json!({"a": 1}), next)
            .await
            .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"a": 1}));
    }

    /// Guest whose declaration grows after its `toggle` tool runs: a
    /// mutable flag selects between two static declaration documents.
    /// Requires a pooled session (plus the heap-reset hook): without reuse
    /// every call would see a fresh flag and the change would never stick.
    fn redeclaring_guest_wat() -> String {
        let before = r#"{"tool_types":["toggle"]}"#;
        let after = r#"{"tool_types":["toggle","extra"]}"#;
        let result = r#"{"result":{}}"#;
        let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let (before_escaped, after_escaped, result_escaped) =
            (escape(before), escape(after), escape(result));
        format!(
            r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (global $flipped (mut i32) (i32.const 0))
  (data (i32.const 0) "{before_escaped}")
  (data (i32.const 128) "{after_escaped}")
  (data (i32.const 256) "{result_escaped}")
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_heap_reset") (result i32)
    (global.set $heap (i32.const 4096))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (if (result i64) (i32.eqz (global.get $flipped))
      (then (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const {before_len})) (i64.const 32))))
      (else (i64.or (i64.extend_i32_u (i32.const 128)) (i64.shl (i64.extend_i32_u (i32.const {after_len})) (i64.const 32))))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (global.set $flipped (i32.const 1))
    (i64.or (i64.extend_i32_u (i32.const 256)) (i64.shl (i64.extend_i32_u (i32.const {result_len})) (i64.const 32)))))
"#,
            before_len = before.len(),
            after_len = after.len(),
            result_len = result.len(),
        )
    }

    #[tokio::test]
    async fn reload_reports_unchanged_for_static_guest() {
        use super::super::loader::wasm_test_echo_wat;

        let wat = wasm_test_echo_wat(r#"{"tool_types":[]}"#);
        let plugin = build_pooled_plugin("static-decl", &wat, 2).await;
        assert!(!plugin.reload_declaration().await.expect("reload"));
        assert!(plugin.declaration().tool_types.is_empty());
    }

    #[tokio::test]
    async fn reload_adopts_grown_declaration() {
        let plugin = build_pooled_plugin("redecl", &redeclaring_guest_wat(), 2).await;
        assert!(plugin.inner.pool.enabled());
        assert_eq!(plugin.declaration().tool_types, vec!["toggle".to_owned()]);
        assert!(!plugin.reload_declaration().await.expect("reload"));

        PluginToolExecutorBridge(&plugin, "toggle")
            .call()
            .await
            .expect("toggle flips the guest flag");
        assert!(plugin.reload_declaration().await.expect("reload"));
        assert_eq!(
            plugin.declaration().tool_types,
            vec!["toggle".to_owned(), "extra".to_owned()]
        );

        // The newly declared contribution registers and runs.
        let manager = crate::contributions::ContributionManager::new();
        manager.start_registration("redecl");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        manager
            .get_tool_executor("extra")
            .expect("extra tool registered");
    }

    /// Guest calling the `wf_host::log` import from `wf_on_load`, then
    /// succeeding. Proves the host namespace resolves and the call traps
    /// nothing.
    const HOST_LOG_WAT: &str = r#"(module
  (import "wf_host" "log" (func $hlog (param i32 i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 128))
  (data (i32.const 0) "{}")
  (data (i32.const 64) "hello-host")
  (data (i32.const 8192) "\01\00\00\00")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_supported_abi_versions") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 8192)) (i64.shl (i64.extend_i32_u (i32.const 1)) (i64.const 32))))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32)
    (call $hlog (i32.const 2) (i32.const 64) (i32.const 10))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 2)) (i64.const 32)))))
"#;

    #[tokio::test]
    async fn guest_host_log_import_resolves_and_runs() {
        let dir = std::env::temp_dir().join("wf-wasm-test-hostlog");
        let _ = std::fs::create_dir_all(&dir);
        let bytes = wat::parse_str(HOST_LOG_WAT).expect("valid wat");
        std::fs::write(dir.join("plugin.wasm"), &bytes).expect("write module");
        let manifest = PluginManifest {
            id: "hostlog".into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            entry_point: "plugin.wasm".into(),
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
            wasm: None,
            lua: None,
        };
        let plugin = super::super::loader::load_wasm_plugin_with_base(&manifest, &dir)
            .await
            .expect("load with host-log import");
        plugin
            .on_load(&hook_context("hostlog"))
            .await
            .expect("host log call succeeds");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
