    use super::super::loader::is_component;
    use super::super::policy::{resolve_grants, resolve_limits};
    use super::*;
    use crate::contributions::ContributionManager;
    use crate::manifest::PluginType;

    const TEST_WIT: &str = include_str!("../../../wit/plugin.wit");

    /// Hand-written core-module guest implementing the `wf:plugin/plugin`
    /// world with canonical-ABI exports. `register` declares three tools;
    /// `dispatch` routes on the handler name: `echo` returns a fixed tool
    /// result, `boom` returns a guest error, `spin` never returns.
    /// Lifecycle hooks always succeed.
    ///
    /// Canonical ABI note: exported functions with more flat results than
    /// fit in one value return a single pointer to a guest-written return
    /// area instead, so every function below writes its results to the
    /// static area at `RET` and returns `RET`.
    const TEST_GUEST_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 8192))
  (global $ret i32 (i32.const 2048))
  (data (i32.const 0) "echo")
  (data (i32.const 8) "boom")
  (data (i32.const 16) "spin")
  (data (i32.const 32) "{\"result\":{\"echo\":true}}")
  (data (i32.const 64) "boom failed")
  (data (i32.const 128) "\00\00\00\00\04\00\00\00\08\00\00\00\04\00\00\00\10\00\00\00\04\00\00\00")
  (func (export "cabi_realloc")
    (param $old i32) (param $old_size i32) (param $align i32) (param $new_size i32)
    (result i32)
    (local $p i32)
    (global.set $heap
      (i32.and
        (i32.add (global.get $heap) (i32.sub (local.get $align) (i32.const 1)))
        (i32.xor (i32.sub (local.get $align) (i32.const 1)) (i32.const -1))))
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $new_size)))
    (local.get $p))
  (func $streq (param $p1 i32) (param $l1 i32) (param $p2 i32) (param $l2 i32) (result i32)
    (local $i i32)
    (if (i32.ne (local.get $l1) (local.get $l2))
      (then (i32.const 0) (return)))
    (local.set $i (i32.const 0))
    (block $done
      (loop $cmp
        (br_if $done (i32.ge_u (local.get $i) (local.get $l1)))
        (if (i32.ne
              (i32.load8_u (i32.add (local.get $p1) (local.get $i)))
              (i32.load8_u (i32.add (local.get $p2) (local.get $i))))
          (then (i32.const 0) (return)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $cmp)))
    (i32.const 1))
  (func $ok (result i32)
    (i32.store (global.get $ret) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 0))
    (global.get $ret))
  (func (export "wf:plugin/lifecycle@1.0.0#on-load")
    (param i32 i32 i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@1.0.0#on-activate")
    (param i32 i32 i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@1.0.0#on-deactivate")
    (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@1.0.0#on-unload")
    (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@1.0.0#on-config-change")
    (param i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/contributions@1.0.0#register")
    (result i32)
    (i32.store (global.get $ret) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 128))
    (i32.store (i32.add (global.get $ret) (i32.const 12)) (i32.const 3))
    (i32.store (i32.add (global.get $ret) (i32.const 16)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 20)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 24)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 28)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 32)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 36)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 40)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 44)) (i32.const 0))
    (global.get $ret))
  (func (export "wf:plugin/contributions@1.0.0#dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32)
    (result i32)
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 16) (i32.const 4))
      (then (loop $spin (br $spin))))
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 8) (i32.const 4))
      (then
        (i32.store (global.get $ret) (i32.const 1))
        (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 64))
        (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 11)))
      (else
        (i32.store (global.get $ret) (i32.const 0))
        (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 32))
        (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 24))))
    (global.get $ret)))
"#;

    fn write_u32_leb(value: u32, out: &mut Vec<u8>) {
        let mut v = value;
        loop {
            let mut byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    /// Embed a `metadata::encode` fragment as one opaque
    /// `component-type:*` custom section right after the module header.
    /// The fragment is itself a tiny component carrying the world type,
    /// which is exactly what the encoder decodes back.
    fn embed_component_type(module: &[u8], section_name: &str, fragment: &[u8]) -> Vec<u8> {
        let mut inner = Vec::new();
        write_u32_leb(section_name.len() as u32, &mut inner);
        inner.extend_from_slice(section_name.as_bytes());
        inner.extend_from_slice(fragment);
        let mut section = vec![0u8];
        write_u32_leb(inner.len() as u32, &mut section);
        section.extend_from_slice(&inner);
        let mut full = module.to_vec();
        full.splice(8..8, section);
        full
    }

    /// Encode a core-module WAT guest into a component binary for the
    /// given WIT world: parse WIT, embed the world type, then run the
    /// component encoder.
    fn encode_component_with_wit(
        core_wat: &str,
        wit: &str,
        wit_path: &str,
        world: &str,
    ) -> Vec<u8> {
        let module = wat::parse_str(core_wat).expect("valid core guest wat");
        let mut resolve = wit_parser::Resolve::new();
        let pkg = resolve.push_str(wit_path, wit).expect("wit parses");
        let world = resolve
            .select_world(&[pkg], Some(world))
            .expect("world resolves");
        let fragment = wit_component::metadata::encode(
            &resolve,
            world,
            wit_component::StringEncoding::UTF8,
            None,
        )
        .expect("metadata encodes");
        let full = embed_component_type(&module, "component-type:plugin", &fragment);
        wit_component::ComponentEncoder::default()
            .module(&full)
            .expect("encoder accepts module")
            .validate(true)
            .encode()
            .expect("component encodes")
    }

    /// Encode the standard test guest for the `wf:plugin/plugin` world.
    fn encode_test_component(core_wat: &str) -> Vec<u8> {
        encode_component_with_wit(core_wat, TEST_WIT, "plugin.wit", "plugin")
    }

    fn test_manifest(id: &str, wasm: wf_plugin_sdk::manifest::WasmConfig) -> PluginManifest {
        PluginManifest {
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
            wasm: Some(wasm),
            lua: None,
        }
    }

    /// Load a component through the real load path (size check, compile
    /// cache, pre-resolve, declaration fetch), mirroring the loader.
    async fn load_test_component(
        id: &str,
        bytes: &[u8],
        wasm: wf_plugin_sdk::manifest::WasmConfig,
        guard_timeout_ms: u64,
    ) -> PluginResult<Arc<dyn PluginTrait>> {
        let manifest = test_manifest(id, wasm);
        let limits = resolve_limits(&manifest, guard_timeout_ms);
        let grants = resolve_grants(&manifest);
        load_component_plugin_at(&manifest, bytes, &limits, &grants).await
    }

    fn hook_context(plugin_id: &str) -> PluginContext {
        PluginContext {
            plugin_id: plugin_id.to_owned(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(ContributionManager::new()),
        }
    }

    #[tokio::test]
    async fn component_cache_is_bounded() {
        // Distinct guests keep the encoded bytes distinct: the replacement
        // preserves the 11-byte payload length the guest hardcodes.
        let mut resolve = wit_parser::Resolve::new();
        let pkg = resolve
            .push_str("plugin.wit", TEST_WIT)
            .expect("wit parses");
        let world = resolve
            .select_world(&[pkg], Some("plugin"))
            .expect("world resolves");
        let engine = super::super::pool::engine_handle();
        let cap = super::super::pool::MAX_CACHED_ARTIFACTS;
        for i in 0..(cap + 8) {
            let core_wat = TEST_GUEST_WAT.replace("boom failed", &format!("boomfail{i:03}"));
            let module = wat::parse_str(&core_wat).expect("valid core guest wat");
            let fragment = wit_component::metadata::encode(
                &resolve,
                world,
                wit_component::StringEncoding::UTF8,
                None,
            )
            .expect("metadata encodes");
            let full = embed_component_type(&module, "component-type:plugin", &fragment);
            let bytes = wit_component::ComponentEncoder::default()
                .module(&full)
                .expect("encoder accepts module")
                .validate(true)
                .encode()
                .expect("component encodes");
            cached_component(&engine, &bytes).expect("component caches");
        }
        super::component_cache().run_pending_tasks();
        assert!(
            super::component_cache().entry_count() <= cap,
            "cache holds {} entries, cap is {cap}",
            super::component_cache().entry_count()
        );
    }

    #[test]
    fn component_wit_version_is_pinned() {
        assert!(
            TEST_WIT.contains(WF_COMPONENT_WORLD_VERSION),
            "wit world must carry the pinned version {WF_COMPONENT_WORLD_VERSION}"
        );
        assert!(
            TEST_WIT.contains("wf:plugin"),
            "wit world must declare the wf:plugin package"
        );
        let wit_major = TEST_WIT
            .split("wf:plugin@")
            .nth(1)
            .and_then(|rest| rest.split(['.', '"', ';']).next())
            .and_then(|major| major.trim().parse::<u64>().ok());
        assert_eq!(
            wit_major,
            world_major(WF_COMPONENT_WORLD_VERSION),
            "wit world major must match the pinned host major"
        );
    }

    #[test]
    fn component_world_major_gate_rejects_skew() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let id_major = host_world_major() as u32;
        assert!(guest_world_majors(&bytes).contains(&id_major));
        assert!(check_component_world_version("skew-probe", &bytes).is_ok());
        let synthetic = b"prefix wf:plugin/lifecycle@99.0.0 suffix".to_vec();
        assert_eq!(guest_world_majors(&synthetic), vec![99]);
        assert!(check_component_world_version("skew-probe", &synthetic).is_err());
        assert!(check_component_world_version("empty-probe", b"no world here").is_ok());
    }
    #[test]
    fn encoded_guest_is_detected_as_component() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        assert!(is_component(&bytes));
    }
    #[test]
    fn encoded_guest_exports_plugin_world() {
        use wasmparser::{Parser, Payload};

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(&bytes) {
            if let Payload::ComponentExportSection(reader) = payload.expect("parses") {
                for export in reader {
                    names.push(export.expect("export").name.0.to_string());
                }
            }
        }
        assert!(
            names.contains(&"wf:plugin/lifecycle@1.0.0".to_string()),
            "exports: {names:?}"
        );
        assert!(
            names.contains(&"wf:plugin/contributions@1.0.0".to_string()),
            "exports: {names:?}"
        );
    }

    /// A reactor guest whose `_initialize` (run during instantiation) walks a
    /// bounded loop. The shared engine enables epoch interruption, so the
    /// store's default deadline is already elapsed; without a parked deadline
    /// the loop's back-edge traps before the guest ever handles a call. This
    /// mirrors language runtimes (Go/Python) whose startup contains loops.
    fn reactor_init_component_wat() -> String {
        let module = TEST_GUEST_WAT.trim_end();
        let body = module.strip_suffix(')').expect("module closes");
        format!(
            "{body}
  (func (export \"_initialize\") (local $i i32)
    (local.set $i (i32.const 0))
    (block $done
      (loop $init
        (br_if $done (i32.ge_u (local.get $i) (i32.const 4)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $init)))
))"
        )
    }

    #[tokio::test]
    async fn reactor_initialize_loop_survives_instantiation() {
        let bytes = encode_test_component(&reactor_init_component_wat());
        let plugin = load_test_component("comp-reactor-init", &bytes, Default::default(), 10_000)
            .await
            .expect("reactor guest with an init loop must load");
        plugin
            .on_load(&hook_context("comp-reactor-init"))
            .await
            .expect("on-load after reactor init");
    }

    #[tokio::test]
    async fn component_full_lifecycle_end_to_end() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-e2e", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");

        let ctx = hook_context("comp-e2e");
        plugin.on_load(&ctx).await.expect("on-load");
        plugin.on_activate(&ctx).await.expect("on-activate");

        let manager = ContributionManager::new();
        manager.start_registration("comp-e2e");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("echo")
            .expect("echo tool registered");
        let result = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect("dispatch");
        assert_eq!(result.result, serde_json::json!({"echo": true}));

        plugin.on_deactivate(&ctx).await.expect("on-deactivate");
        plugin.on_unload(&ctx).await.expect("on-unload");

        // Every lifecycle hook plus the tool dispatch succeeded; a repeat
        // dispatch over the same plugin keeps working.
        let again = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect("second dispatch");
        assert_eq!(again.result, serde_json::json!({"echo": true}));
    }

    #[tokio::test]
    async fn component_dispatch_error_surfaces_as_wasm_error() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-boom", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-boom");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("boom")
            .expect("boom tool registered");
        let err = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect_err("guest Err must fail");
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected wasm error, got {err:?}"
        );
        let text = err.to_string();
        assert!(
            text.contains("comp-boom"),
            "error must name the plugin: {text}"
        );
        assert!(
            text.contains("boom failed"),
            "guest detail must surface: {text}"
        );
    }

    /// The standard test guest answers dispatch with a fixed JSON object
    /// (neither a boolean nor a middleware envelope), so the middleware
    /// handler must pass the context through unchanged (legacy behavior).
    #[tokio::test]
    async fn component_middleware_legacy_output_passes_context_through() {
        use crate::contributions::NextFn;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest("comp-mw", Default::default());
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-mw", &engine, &pre, &grants, &limits, &stats);
        let inner = Arc::new(ComponentPluginInner {
            manifest,
            engine,
            limits,
            decl: std::sync::RwLock::new(WasmContributionDecl::default()),
            stats,
            pool,
        });
        let handler = ComponentMiddlewareHandler {
            inner,
            phase: "pre_tool".to_owned(),
        };
        let input = serde_json::json!({"a": 1});
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = handler
            .handle(input.clone(), next)
            .await
            .expect("middleware runs");
        assert_eq!(out, input);
    }

    /// Test guest variant that calls the `host-log` world import from
    /// `on-load` before succeeding. The import line matches the world
    /// import name; the call passes `(level, ptr, len)` of a static message.
    fn host_log_component_wat() -> String {
        TEST_GUEST_WAT
            .replacen(
                "(module",
                "(module\n  (import \"wf:plugin/host-log@1.0.0\" \"log\" (func $host_log (param i32 i32 i32)))\n  (data (i32.const 4096) \"hello-host\")",
                1,
            )
            .replacen(
                "(func (export \"wf:plugin/lifecycle@1.0.0#on-load\")\n    (param i32 i32 i32 i32) (result i32)\n    (call $ok))",
                "(func (export \"wf:plugin/lifecycle@1.0.0#on-load\")\n    (param i32 i32 i32 i32) (result i32)\n    (call $host_log (i32.const 2) (i32.const 4096) (i32.const 10))\n    (call $ok))",
                1,
            )
    }

    #[tokio::test]
    async fn component_host_log_import_resolves_and_runs() {
        let bytes = encode_test_component(&host_log_component_wat());
        let plugin = load_test_component("comp-hostlog", &bytes, Default::default(), 10_000)
            .await
            .expect("component with host-log import loads");
        plugin
            .on_load(&hook_context("comp-hostlog"))
            .await
            .expect("host log call succeeds");
    }

    #[tokio::test]
    async fn component_register_call_records_fuel() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest("comp-fuel", Default::default());
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-fuel", &engine, &pre, &grants, &limits, &stats);
        let inner = ComponentPluginInner {
            manifest,
            engine,
            limits,
            decl: std::sync::RwLock::new(WasmContributionDecl::default()),
            stats: stats.clone(),
            pool,
        };
        fetch_component_declaration(&inner)
            .await
            .expect("register works");
        let snap = stats.snapshot();
        assert!(snap.calls >= 1, "register call is counted");
        assert!(
            snap.fuel_consumed > 0,
            "metered component calls consume fuel, got {snap:?}"
        );
    }

    #[tokio::test]
    async fn component_spin_guest_times_out() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component(
            "comp-spin",
            &bytes,
            WasmConfig {
                fuel_limit: Some(0),
                call_timeout_ms: Some(200),
                ..Default::default()
            },
            0,
        )
        .await
        .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-spin");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("spin")
            .expect("spin tool registered");
        let err = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect_err("spinning guest must time out");
        assert!(
            matches!(err, PluginError::Timeout { .. }),
            "expected timeout, got {err:?}"
        );
    }

    #[tokio::test]
    async fn component_pool_reuses_idle_session() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest(
            "comp-pool",
            WasmConfig {
                store_pool_size: Some(2),
                ..Default::default()
            },
        );
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-pool", &engine, &pre, &grants, &limits, &stats);
        assert!(pool.enabled());

        let session = pool.acquire().await.expect("first acquire builds");
        assert_eq!(stats.snapshot().pool_misses, 1);
        pool.release(session, true).await;
        let reused = pool.acquire().await.expect("second acquire reuses");
        assert_eq!(stats.snapshot().pool_hits, 1);
        pool.release(reused, false).await;
        assert_eq!(stats.snapshot().pool_drops, 1);
    }

    #[tokio::test]
    async fn component_pooled_dispatch_roundtrips() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component(
            "comp-pooled-e2e",
            &bytes,
            WasmConfig {
                store_pool_size: Some(2),
                ..Default::default()
            },
            10_000,
        )
        .await
        .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-pooled-e2e");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("echo")
            .expect("echo tool registered");
        for _ in 0..2 {
            let result = executor
                .execute(PluginToolContext {
                    args: serde_json::json!({}),
                })
                .await
                .expect("pooled dispatch");
            assert_eq!(result.result, serde_json::json!({"echo": true}));
        }
    }

    #[tokio::test]
    async fn component_reload_reports_no_change_for_static_guest() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-reload", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");
        assert!(!plugin.reload_declaration().await.expect("reload"));
    }

    #[tokio::test]
    async fn component_missing_exports_rejected() {
        // A component from an empty world carries no lifecycle or
        // contribution exports, so host binding must fail at load.
        let bytes = encode_component_with_wit(
            r#"(module
  (memory (export "memory") 1)
  (func (export "cabi_realloc")
    (param i32 i32 i32 i32) (result i32)
    (i32.const 1024)))
"#,
            "package wf:empty@0.1.0;\nworld hollow {\n}\n",
            "empty.wit",
            "hollow",
        );
        assert!(is_component(&bytes));

        let err = match load_test_component("comp-hollow", &bytes, Default::default(), 10_000).await
        {
            Ok(_) => panic!("exports missing, load must fail"),
            Err(e) => e,
        };
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected bind failure, got {err:?}"
        );
    }
