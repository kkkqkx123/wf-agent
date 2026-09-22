//! End-to-end host verification: load a built wasm guest through the real
//! plugin loader, run its lifecycle, register its contributions, and dispatch
//! the `echo` tool.
//!
//! Guests are build outputs and are not committed, so build the example first
//! (see each example's Makefile / README), then point this binary at its
//! directory. A non-zero exit code means at least one guest failed.
//!
//! Usage: cargo run --offline -- <plugin-dir> [<plugin-dir> ...]

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use wf_plugin::context::{PluginContext, PluginLogger};
use wf_plugin::contributions::{ContributionManager, PluginToolContext};
use wf_plugin::manifest::PluginManifest;
use wf_plugin::wasm::load_wasm_plugin_with_base;

fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(src) = cur {
        out.push_str("\n     caused by: ");
        out.push_str(&src.to_string());
        cur = src.source();
    }
    out
}

async fn verify(dir: PathBuf) -> Result<(), String> {
    let toml_text =
        std::fs::read_to_string(dir.join("plugin.toml")).map_err(|e| format!("plugin.toml: {e}"))?;
    let manifest: PluginManifest =
        toml::from_str(&toml_text).map_err(|e| format!("manifest parse: {e}"))?;
    println!("== {} ({}) entry={}", manifest.id, dir.display(), manifest.entry_point);

    let plugin = load_wasm_plugin_with_base(&manifest, &dir)
        .await
        .map_err(|e| format!("load: {}", chain(&e)))?;

    let manager = Arc::new(ContributionManager::new());
    let ctx = PluginContext {
        plugin_id: manifest.id.clone(),
        sdk_version: "0.1.0".into(),
        config: json!({}),
        logger: PluginLogger,
        contribution_manager: manager.clone(),
    };

    plugin.on_load(&ctx).await.map_err(|e| format!("on_load: {e}"))?;
    plugin.on_activate(&ctx).await.map_err(|e| format!("on_activate: {e}"))?;

    manager.start_registration(&manifest.id);
    {
        let mut registrar = manager.as_registrar();
        plugin
            .register_contributions(&mut registrar)
            .map_err(|e| format!("register_contributions: {e}"))?;
    }
    println!("   registered tools: {:?}", manager.all_tool_types());

    let executor = manager
        .get_tool_executor("echo")
        .ok_or_else(|| "tool 'echo' was not registered".to_string())?;
    let out = executor
        .execute(PluginToolContext { args: json!({}) })
        .await
        .map_err(|e| format!("dispatch echo: {e}"))?;
    println!("   echo -> {}", serde_json::to_string(&out).map_err(|e| e.to_string())?);

    plugin.on_deactivate(&ctx).await.map_err(|e| format!("on_deactivate: {e}"))?;
    plugin.on_unload(&ctx).await.map_err(|e| format!("on_unload: {e}"))?;
    manager.unregister_all(&manifest.id);
    println!("== OK {}", manifest.id);
    Ok(())
}

#[tokio::main]
async fn main() {
    let dirs: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if dirs.is_empty() {
        eprintln!("usage: wasm-verify <plugin-dir> [<plugin-dir> ...]");
        std::process::exit(2);
    }
    let mut failed = false;
    for dir in dirs {
        if let Err(e) = verify(dir.clone()).await {
            eprintln!("!! FAIL {}: {e}", dir.display());
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
