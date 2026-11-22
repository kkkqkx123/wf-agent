//! End-to-end smoke coverage for the Lua bridge against mlua 0.12.
//!
//! The 0.9 -> 0.12 migration removed lifetime parameters from `Value`/`Table`
//! and reduced `Function::call` / `Table::get` to a single type parameter.
//! Compilation alone does not prove the return types still resolve correctly,
//! so these tests exercise real Lua state and real calls.

#![cfg(feature = "lua")]

use std::sync::{Arc, Mutex};

use wf_plugin::lua::bridge;
use wf_plugin::lua::pool;
use wf_plugin::PluginNodeHandler;

fn node_lua() -> mlua::Lua {
    pool::create_state(
        r#"
        plugin = {
          execute = function(ctx)
            return { outputs = { sum = ctx.inputs.a + ctx.inputs.b, tag = ctx.config.tag } }
          end
        }
        "#,
    )
    .expect("lua state must build")
}

#[test]
fn value_round_trip_preserves_types() {
    let lua = mlua::Lua::new();
    let json = serde_json::json!({
        "s": "hi",
        "n": 7,
        "f": 1.5,
        "b": true,
        "arr": [1, 2, 3],
        "obj": { "k": "v" }
    });
    let converted = bridge::to_lua_value(&lua, &json);
    let back = bridge::from_lua_value(converted);
    assert_eq!(back, json);

    // Top-level null (a Lua Nil value, not a table key set to nil) survives.
    let null_back = bridge::from_lua_value(bridge::to_lua_value(&lua, &serde_json::Value::Null));
    assert_eq!(null_back, serde_json::Value::Null);
}

#[tokio::test]
async fn node_handler_call_and_get_resolve_return_types() {
    let lua = node_lua();
    // Register the Lua function in the registry, exactly as a loaded plugin does.
    let func: mlua::Function = lua
        .globals()
        .get::<mlua::Table>("plugin")
        .unwrap()
        .get("execute")
        .unwrap();
    let func_key = lua.create_registry_value(func).unwrap();

    let handler = wf_plugin::lua::handlers::LuaNodeHandler {
        lua: Arc::new(Mutex::new(lua)),
        func_key: Arc::new(func_key),
        limits: pool::LuaExecutionLimits::default(),
    };
    let ctx = wf_plugin::contributions::PluginExecutionContext {
        node_id: "n1".into(),
        inputs: serde_json::json!({ "a": 2, "b": 3 }),
        config: serde_json::json!({ "tag": "ok" }),
    };
    let result = handler.execute(ctx).await.expect("node call must succeed");
    assert_eq!(result.outputs, serde_json::json!({ "sum": 5, "tag": "ok" }));
}

#[test]
fn protection_hook_allows_execution_and_times_out() {
    let lua = mlua::Lua::new();
    // A hook that simply continues must let normal execution finish.
    pool::set_protection_hook(&lua, None, usize::MAX).expect("hook must install");
    let value: i64 = lua.load("return 40 + 2").eval().unwrap();
    assert_eq!(value, 42);

    // An already-expired deadline must abort a long-running chunk.
    let lua2 = mlua::Lua::new();
    pool::set_protection_hook(
        &lua2,
        Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
        usize::MAX,
    )
    .expect("hook must install");
    let err = lua2
        .load("local x = 0; for i = 1, 1000000 do x = x + i end; return x")
        .exec()
        .unwrap_err();
    assert!(
        err.to_string().contains("timed out"),
        "expected timeout error, got: {err}"
    );
}
