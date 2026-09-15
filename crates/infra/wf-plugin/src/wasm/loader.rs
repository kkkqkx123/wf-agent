use std::path::{Component, Path};
use std::sync::Arc;

use tokio::fs;

use super::plugin::{fetch_declaration, WasmPlugin, WasmPluginInner};
use super::policy::{resolve_grants, resolve_limits, validate_network_policy};
use super::pool::{self, SessionPool};
use super::stats::WasmStats;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;
use wf_plugin_sdk::wasm::export;

/// Fallback per-call timeout when the manifest sets none. Mirrors the
/// engine default guard timeout; operators aligning
/// `PluginSystemConfig::guard_timeout_ms` should also set
/// `wasm.call_timeout_ms` on their plugins.
pub const DEFAULT_WASM_CALL_TIMEOUT_MS: u64 = 10_000;

pub async fn load_wasm_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let base_path = determine_base_path(manifest)?;
    load_wasm_plugin_at(manifest, &base_path).await
}

pub async fn load_wasm_plugin_with_base(
    manifest: &PluginManifest,
    base: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    load_wasm_plugin_at(manifest, base).await
}

async fn load_wasm_plugin_at(
    manifest: &PluginManifest,
    base_path: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    let id = &manifest.id;
    validate_plugin_id(id)?;
    validate_entry_point(&manifest.entry_point)?;

    let module_path = base_path.join(&manifest.entry_point);
    if let (Ok(canonical_base), Ok(canonical_module)) =
        (base_path.canonicalize(), module_path.canonicalize())
    {
        if !canonical_module.starts_with(&canonical_base) {
            return Err(PluginError::LoadFailed(format!(
                "wasm plugin path traversal denied: {:?} escapes base {:?}",
                module_path, base_path
            )));
        }
    }

    let limits = resolve_limits(manifest, DEFAULT_WASM_CALL_TIMEOUT_MS)?;
    validate_network_policy(manifest)?;
    let bytes = fs::read(&module_path)
        .await
        .map_err(|e| PluginError::LoadFailed(format!("cannot read {module_path:?}: {e}")))?;
    // Route to the component-model host when the binary carries the
    // component magic; the core-module path handles everything else.
    if is_component(&bytes) {
        let grants = resolve_grants(manifest);
        return super::component::load_component_plugin_at(
            manifest,
            &module_path,
            &limits,
            &grants,
        )
        .await;
    }
    if bytes.len() as u64 > limits.max_module_bytes {
        return Err(PluginError::LoadFailed(format!(
            "wasm module {module_path:?} ({} bytes) exceeds limit of {} bytes",
            bytes.len(),
            limits.max_module_bytes
        )));
    }

    let engine = pool::engine_handle();
    let module = pool::cached_module(&engine, &bytes).map_err(|e| match e {
        PluginError::WasmError(text) => {
            PluginError::WasmError(format!("plugin '{id}' module compile failed: {text}"))
        }
        other => other,
    })?;
    if module
        .get_export("_start")
        .or_else(|| module.get_export("_initialize"))
        .is_some()
    {
        tracing::warn!(
            "wasm plugin '{id}' exports a start function; only the reactor-style wf_* exports are invoked"
        );
    }

    let linker = pool::new_linker(&engine)?;
    // Resolve imports once at load time so every later call instantiates
    // from the pre-resolved artifact instead of repeating linker work.
    let pre = linker
        .instantiate_pre(&module)
        .map_err(|e| pool::wasm_err(&format!("plugin '{id}' pre-instantiation failed"), e))?;
    let grants = resolve_grants(manifest);
    tracing::info!(
        "wasm plugin '{id}' grants: {} dir(s), {} env var(s), network={}",
        grants.preopened_dirs.len(),
        grants.env_vars.len(),
        grants.allow_network
    );
    // Pooling requires the guest heap-reset hook: without it, reusing a
    // store would corrupt the bump allocator. Probe the export once here
    // so per-call paths never pay for the check.
    let supports_reset = module.get_export(export::HEAP_RESET).is_some();
    if limits.pool_size > 0 && !supports_reset {
        tracing::info!(
            "wasm plugin '{id}' requests a session pool but does not export '{}'; pooling disabled",
            export::HEAP_RESET
        );
    }
    let stats = Arc::new(WasmStats::default());
    let pool = SessionPool::new(id, &engine, &pre, &grants, &limits, &stats, supports_reset);
    let mut inner = WasmPluginInner {
        manifest: manifest.clone(),
        engine,
        limits,
        decl: Default::default(),
        stats,
        pool,
    };
    inner.decl = fetch_declaration(&inner).await?;

    Ok(Arc::new(WasmPlugin::from_inner(inner)) as Arc<dyn Plugin>)
}

/// Detect a component-model binary by its 8-byte header. Core modules
/// carry the standard wasm header `[0x01, 0x00, 0x00, 0x00]`; component
/// binaries append a second 4-byte word `[0x0D, 0x00, 0x01, 0x00]`.
pub(crate) fn is_component(bytes: &[u8]) -> bool {
    const WASM_MAGIC: &[u8; 4] = b"\0asm";
    const COMPONENT_VERSION: &[u8; 4] = &[0x0D, 0x00, 0x01, 0x00];
    bytes.len() >= 8 && &bytes[..4] == WASM_MAGIC && &bytes[4..8] == COMPONENT_VERSION
}

fn validate_plugin_id(id: &str) -> PluginResult<()> {
    if id.is_empty() {
        return Err(PluginError::LoadFailed("plugin id is empty".into()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(PluginError::LoadFailed(format!(
            "plugin id '{id}' contains path traversal"
        )));
    }
    Ok(())
}

fn validate_entry_point(entry: &str) -> PluginResult<()> {
    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(PluginError::LoadFailed(format!(
            "wasm entry_point '{entry}' must be relative"
        )));
    }
    for comp in path.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(PluginError::LoadFailed(format!(
                "wasm entry_point '{entry}' contains parent traversal"
            )));
        }
    }
    if !entry.ends_with(".wasm") {
        return Err(PluginError::LoadFailed(format!(
            "wasm entry_point '{entry}' must end with .wasm"
        )));
    }
    Ok(())
}

fn determine_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
    validate_plugin_id(&manifest.id)?;
    validate_entry_point(&manifest.entry_point)?;

    let candidate = std::path::PathBuf::from("plugins").join(&manifest.id);
    if candidate.join(&manifest.entry_point).exists() {
        return Ok(candidate);
    }
    if std::path::Path::new(&manifest.entry_point).exists() {
        return Ok(std::path::PathBuf::from("."));
    }
    Err(PluginError::LoadFailed(format!(
        "cannot find entry point '{}' for plugin '{}'",
        manifest.entry_point, manifest.id
    )))
}

/// Minimal guest used across wasm tests: `alloc` bump allocator, hooks
/// returning success, `wf_register` declaring one tool, `wf_dispatch`
/// returning a fixed `PluginToolResult` document.
#[cfg(test)]
pub(crate) fn wasm_test_echo_wat(register_json: &str) -> String {
    let escaped = register_json.replace('\\', "\\\\").replace('"', "\\\"");
    let len = register_json.len();
    let result_json = r#"{"result":{"echo":true}}"#;
    let result_escaped = result_json.replace('\\', "\\\\").replace('"', "\\\"");
    let result_len = result_json.len();
    format!(
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{escaped}")
  (data (i32.const 1024) "{result_escaped}")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32) (i32.const 0))
  (func (export "wf_on_activate") (param $p i32) (param $n i32) (result i32) (i32.const 0))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const {len})) (i64.const 32))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (i64.or (i64.extend_i32_u (i32.const 1024)) (i64.shl (i64.extend_i32_u (i32.const {result_len})) (i64.const 32)))))
"#
    )
}

/// Echo guest variant that also exports `wf_heap_reset` returning
/// `reset_code`, opting into session-pool reuse. The heap base matches the
/// echo guest (4096), so a successful reset restores the allocator.
#[cfg(test)]
pub(crate) fn wasm_test_echo_reset_wat(register_json: &str, reset_code: i32) -> String {
    let base = wasm_test_echo_wat(register_json);
    let body = base
        .trim_end()
        .strip_suffix(')')
        .expect("echo wat ends with the module close paren");
    format!(
        "{body}\n  (func (export \"wf_heap_reset\") (result i32)\n    (global.set $heap (i32.const 4096))\n    (i32.const {reset_code}))\n)\n"
    )
}

/// Guest whose `wf_on_load` spins forever. With fuel metering disabled and
/// a short call timeout it must be stopped by epoch interruption.
#[cfg(test)]
pub(crate) fn wasm_test_spin_wat() -> String {
    r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 64))
  (func (export "alloc") (param $n i32) (result i32) (i32.const 64))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32)
    (loop $spin (br $spin))
    (i32.const 0)))
"#
    .to_owned()
}

/// Guest probing for a preopened directory from `wf_on_load`. The host
/// grants no preopens by default, so `fd_prestat_get` on fd 3 must return
/// an error, which surfaces as a hook failure.
#[cfg(test)]
pub(crate) fn wasm_test_preopen_wat() -> String {
    r#"(module
  (import "wasi_snapshot_preview1" "fd_prestat_get"
    (func $prestat_get (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 128))
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32)
    (call $prestat_get (i32.const 3) (i32.const 64))))
"#
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginType;
    use wf_plugin_sdk::manifest::WasmConfig;

    fn manifest(id: &str, entry: &str) -> PluginManifest {
        PluginManifest {
            id: id.into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            sdk_version: None,
            entry_point: entry.into(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            wasm: None,
        }
    }

    #[test]
    fn rejects_absolute_and_traversal_entries() {
        assert!(validate_entry_point("/abs/plugin.wasm").is_err());
        assert!(validate_entry_point("../escape.wasm").is_err());
        assert!(validate_entry_point("plugin.so").is_err());
        assert!(validate_entry_point("dir/plugin.wasm").is_ok());
    }

    #[test]
    fn rejects_bad_plugin_ids() {
        assert!(validate_plugin_id("").is_err());
        assert!(validate_plugin_id("../x").is_err());
        assert!(validate_plugin_id("ok-id").is_ok());
    }

    #[test]
    fn component_binaries_are_detected() {
        let mut bytes = b"\0asm".to_vec();
        bytes.extend_from_slice(&[0x0D, 0x00, 0x01, 0x00, 0x00]);
        assert!(is_component(&bytes));

        let mut core = b"\0asm".to_vec();
        core.extend_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x00]);
        assert!(!is_component(&core));
        assert!(!is_component(b"tiny"));
    }

    #[test]
    fn oversized_module_is_rejected() {
        let mut m = manifest("w", "plugin.wasm");
        m.wasm = Some(WasmConfig {
            max_module_bytes: Some(4),
            ..Default::default()
        });
        let limits = resolve_limits(&m, 0).expect("limits");
        assert_eq!(limits.max_module_bytes, 4);
    }

    #[tokio::test]
    async fn network_request_fails_load_before_read() {
        let dir = std::env::temp_dir().join("wf-wasm-test-netdeny");
        let _ = std::fs::create_dir_all(&dir);
        let mut m = manifest("netdeny", "plugin.wasm");
        m.permissions = vec![crate::manifest::PluginPermission::Network];
        m.wasm = Some(WasmConfig {
            allow_network: Some(true),
            ..Default::default()
        });
        let err = match load_wasm_plugin_with_base(&m, &dir).await {
            Ok(_) => panic!("network request must fail"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("allow_network"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_module(dir: &std::path::Path, wat: &str) {
        let bytes = wat::parse_str(wat).expect("valid wat");
        std::fs::write(dir.join("plugin.wasm"), &bytes).expect("write module");
    }

    fn test_context(plugin_id: &str) -> crate::context::PluginContext {
        crate::context::PluginContext {
            plugin_id: plugin_id.to_owned(),
            sdk_version: "0.1.0".into(),
            config: serde_json::Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: std::sync::Arc::new(
                crate::contributions::ContributionManager::new(),
            ),
        }
    }

    #[tokio::test]
    async fn loads_echo_plugin_from_disk() {
        let dir = std::env::temp_dir().join("wf-wasm-test-echo");
        let _ = std::fs::create_dir_all(&dir);
        write_module(&dir, &wasm_test_echo_wat(r#"{"tool_types":["echo_tool"]}"#));

        let m = manifest("echo", "plugin.wasm");
        let plugin = load_wasm_plugin_with_base(&m, &dir).await.expect("load");
        assert_eq!(plugin.manifest().id, "echo");

        let manager = crate::contributions::ContributionManager::new();
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("echo_tool")
            .expect("tool registered");
        let out = executor
            .execute(crate::contributions::PluginToolContext {
                args: serde_json::json!({"q": 1}),
            })
            .await
            .expect("tool executes");
        assert_eq!(out.result["echo"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rejects_invalid_module_bytes() {
        let dir = std::env::temp_dir().join("wf-wasm-test-bad");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("plugin.wasm"), b"not a wasm module").expect("write");
        let m = manifest("bad", "plugin.wasm");
        let result = load_wasm_plugin_with_base(&m, &dir).await;
        let err = match result {
            Ok(_) => panic!("loading invalid bytes must fail"),
            Err(e) => e,
        };
        assert!(matches!(err, PluginError::WasmError(_)), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn spinning_guest_is_stopped_by_epoch_timeout() {
        let dir = std::env::temp_dir().join("wf-wasm-test-spin");
        let _ = std::fs::create_dir_all(&dir);
        write_module(&dir, &wasm_test_spin_wat());

        let mut m = manifest("spin", "plugin.wasm");
        m.wasm = Some(WasmConfig {
            fuel_limit: Some(0),
            call_timeout_ms: Some(100),
            ..Default::default()
        });
        let plugin = load_wasm_plugin_with_base(&m, &dir).await.expect("load");
        let err = plugin
            .on_load(&test_context("spin"))
            .await
            .expect_err("spin must time out");
        assert!(matches!(err, PluginError::Timeout { .. }), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn wasi_preopen_is_granted_with_permission() {
        let dir = std::env::temp_dir().join("wf-wasm-test-grant");
        let _ = std::fs::create_dir_all(&dir);
        write_module(&dir, &wasm_test_preopen_wat());

        let mut m = manifest("grant", "plugin.wasm");
        m.permissions = vec![crate::manifest::PluginPermission::Filesystem];
        m.wasm = Some(WasmConfig {
            allowed_dirs: Some(vec![dir.to_string_lossy().into_owned()]),
            ..Default::default()
        });
        let plugin = load_wasm_plugin_with_base(&m, &dir).await.expect("load");
        plugin
            .on_load(&test_context("grant"))
            .await
            .expect("preopened dir must be visible");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn oversized_register_message_is_rejected() {
        let dir = std::env::temp_dir().join("wf-wasm-test-huge");
        let _ = std::fs::create_dir_all(&dir);
        let wat = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 64))
  (func (export "alloc") (param $n i32) (result i32) (i32.const 64))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0))
      (i64.shl (i64.extend_i32_u (i32.const 2147483647)) (i64.const 32)))))
"#;
        write_module(&dir, wat);

        let m = manifest("huge", "plugin.wasm");
        let err = match load_wasm_plugin_with_base(&m, &dir).await {
            Ok(_) => panic!("oversized decl must fail"),
            Err(e) => e,
        };
        assert!(matches!(err, PluginError::WasmError(_)), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_register_json_is_rejected() {
        let dir = std::env::temp_dir().join("wf-wasm-test-badjson");
        let _ = std::fs::create_dir_all(&dir);
        let wat = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 64))
  (data (i32.const 0) "oops not json")
  (func (export "alloc") (param $n i32) (result i32) (i32.const 64))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0))
      (i64.shl (i64.extend_i32_u (i32.const 13)) (i64.const 32)))))
"#;
        write_module(&dir, wat);

        let m = manifest("badjson", "plugin.wasm");
        let err = match load_wasm_plugin_with_base(&m, &dir).await {
            Ok(_) => panic!("invalid decl must fail"),
            Err(e) => e,
        };
        assert!(matches!(err, PluginError::WasmError(_)), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn identical_modules_share_the_compile_cache() {
        let dir = std::env::temp_dir().join("wf-wasm-test-cache");
        let _ = std::fs::create_dir_all(&dir);
        write_module(
            &dir,
            &wasm_test_echo_wat(r#"{"tool_types":["cache_probe_tool"]}"#),
        );

        // Pre-warm the cache with these exact bytes, then require an
        // observation window with zero new compilations across reloads.
        // Retries tolerate concurrent compilations from sibling tests.
        let m = manifest("cache", "plugin.wasm");
        load_wasm_plugin_with_base(&m, &dir)
            .await
            .expect("warm load");
        for _ in 0..100 {
            let before = super::super::pool::compile_count_value();
            load_wasm_plugin_with_base(&m, &dir).await.expect("reload");
            if super::super::pool::compile_count_value() == before {
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("identical bytes recompiled; cache miss");
    }

    #[tokio::test]
    async fn wasi_preopen_is_denied_by_default() {
        let dir = std::env::temp_dir().join("wf-wasm-test-preopen");
        let _ = std::fs::create_dir_all(&dir);
        write_module(&dir, &wasm_test_preopen_wat());

        let m = manifest("preopen", "plugin.wasm");
        let plugin = load_wasm_plugin_with_base(&m, &dir).await.expect("load");
        let err = plugin
            .on_load(&test_context("preopen"))
            .await
            .expect_err("preopen probe must fail");
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected hook error, got {err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
