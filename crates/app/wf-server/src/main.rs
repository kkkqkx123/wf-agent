use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::info;

use wf_runtime::bootstrap::{InfraSourceConfig, Runtime, RuntimeConfig};
use wf_types::config::user_git::UserGitConfig;

#[derive(Debug, Parser)]
#[command(
    name = "wf-server",
    about = "wf-agent HTTP server (standalone)",
    version
)]
struct Args {
    /// Listen address, e.g. 127.0.0.1:3000. Wins over `WF_SERVER_BIND_ADDR`
    /// env and the `server.toml` file layer.
    #[arg(long)]
    addr: Option<SocketAddr>,

    /// Project root for project-layer config (`.wf/` deltas). Versioned
    /// service defaults are not read from here; see `deploy/README.md`.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Explicit global settings directory (mcp-settings.json,
    /// skill-settings.json, llm-auth/). Absent means project-only:
    /// the server never reads ambient home-directory state.
    #[arg(long)]
    settings_dir: Option<PathBuf>,

    /// Web frontend build directory served with SPA fallback. Wins over
    /// `WF_SERVER_STATIC_DIR` env and the `server.toml` file layer.
    #[arg(long)]
    static_dir: Option<PathBuf>,

    /// Single-machine default workspace: directory-less requests resolve
    /// to this directory instead of failing. Absent keeps the fail-loud
    /// behavior for multi-tenant serving.
    #[arg(long)]
    directory: Option<PathBuf>,

    /// Storage backend spec: memory | sqlite:<path> | sqlite | postgres:<conn>
    #[arg(long)]
    storage: Option<String>,

    /// Log level: trace|debug|info|warn|error
    #[arg(long)]
    log_level: Option<String>,
}

fn build_runtime_config(args: &Args) -> (RuntimeConfig, Option<InfraSourceConfig>) {
    let mut config = RuntimeConfig::default();

    if let Some(spec) = args.storage.as_deref() {
        if let Some(storage) = wf_config::storage_spec::parse_storage_spec(spec) {
            config.storage = storage;
        }
    }

    // Workspace-bound domains (fs, git, pty, mentions) carry no server-side
    // default workspace: every request names its directory explicitly and
    // directory-less requests fail loud at the handler. This switch only
    // flips the domain kill-switch; `WF_USER_GIT_ENABLED=false` locks every
    // workspace-bound route down to an explicit rejection.
    let workspace_enabled = match std::env::var("WF_USER_GIT_ENABLED") {
        Ok(raw) if raw.trim().eq_ignore_ascii_case("false") => false,
        Ok(raw) if raw.trim().eq_ignore_ascii_case("0") => false,
        Ok(raw) if raw.trim().eq_ignore_ascii_case("no") => false,
        _ => true,
    };
    config.user_git = Some(UserGitConfig {
        enabled: workspace_enabled,
        ..Default::default()
    });

    if let Some(level) = args.log_level.as_deref() {
        let lower = level.to_ascii_lowercase();
        let normalized = match lower.as_str() {
            "warning" => "warn",
            other => other,
        };
        config.log_config = config.log_config.with_level(normalized.to_string());
    }

    let source = match args.config.clone() {
        None => None,
        project_root => Some(InfraSourceConfig {
            project_root,
            settings_dir: args.settings_dir.clone(),
            ..Default::default()
        }),
    };

    (config, source)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Some(spec) = args.storage.as_deref() {
        if let Err(e) = wf_config::storage_spec::parse_storage_spec_result(spec) {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    if let Some(level) = args.log_level.as_deref() {
        let lower = level.to_ascii_lowercase();
        match lower.as_str() {
            "trace" | "debug" | "info" | "warn" | "warning" | "error" => {}
            _ => {
                eprintln!("invalid --log-level '{level}': expected trace|debug|info|warn|error");
                std::process::exit(2);
            }
        }
    }

    let (runtime_config, infra_source) = build_runtime_config(&args);
    // The server owns its listen address: CLI flag wins over
    // `WF_SERVER_BIND_ADDR` env and the `server.toml` file layer. A present
    // but invalid file fails startup instead of silently binding elsewhere.
    let server_config = match wf_server::ServerConfig::resolve(
        args.config.as_deref(),
        args.settings_dir.as_deref(),
        args.addr,
        args.static_dir,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("server config resolution failed: {e}");
            std::process::exit(1);
        }
    };
    let runtime = match infra_source {
        Some(source) => Runtime::bootstrap_with_source(runtime_config, source).await,
        None => Runtime::bootstrap(runtime_config).await,
    };
    let runtime = match runtime {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("runtime bootstrap failed: {e}");
            std::process::exit(1);
        }
    };

    let ctx: Arc<wf_api::ApiContext> = runtime.api_context_arc();

    let metrics_registry = runtime.metrics().map(|m| m.registry().clone());

    // Multi-directory serving: every workspace-bound request names its
    // directory; the allowlist gates explicit directories, and
    // directory-less requests fail loud at the handler unless the
    // operator pinned a single-machine default with `--directory`.
    let dirs = Arc::new(match args.directory.as_deref() {
        Some(default) => {
            match wf_server::directory::DirectoryContext::with_default(
                server_config.directory.clone(),
                default,
            ) {
                Ok(ctx) => ctx,
                Err(e) => {
                    eprintln!("invalid --directory {}: {e}", default.display());
                    std::process::exit(2);
                }
            }
        }
        None => wf_server::directory::DirectoryContext::new(server_config.directory.clone()),
    });
    info!(
        allowed_roots = server_config.directory.allowed_roots.len(),
        cached_directories = dirs.entry_count(),
        default_directory = dirs
            .default_scope()
            .map(|scope| scope.directory.to_string_lossy().into_owned())
            .unwrap_or_default(),
        "directory scope ready"
    );

    let addr = server_config.bind_addr;
    // An empty allowlist on a non-loopback socket serves every path the
    // process can read to any key holder. Flag the fail-open default so
    // remote deployments pin `allowed_roots` instead of discovering it.
    if server_config.directory.allowed_roots.is_empty() && !addr.ip().is_loopback() {
        tracing::warn!(
            %addr,
            "serving without a directory allowlist on a non-loopback address; set allowed_roots to bound the served paths"
        );
    }

    let handle = if let Some(registry) = metrics_registry.clone() {
        info!(%addr, "starting wf-server with metrics");
        match wf_server::serve_full_with_config(
            registry,
            ctx.clone(),
            &server_config,
            Arc::clone(&dirs),
        )
        .await
        {
            Ok(h) => h,
            Err(e) => {
                eprintln!("bind failed at {addr}: {e}");
                std::process::exit(1);
            }
        }
    } else {
        info!(%addr, "starting wf-server");
        match wf_server::serve_api_with_config(ctx.clone(), &server_config, Arc::clone(&dirs)).await
        {
            Ok(h) => h,
            Err(e) => {
                eprintln!("bind failed at {addr}: {e}");
                std::process::exit(1);
            }
        }
    };

    println!("wf-server listening on {}", handle.addr());
    info!(addr = %handle.addr(), "wf-server listening");

    let runtime_holder = Arc::new(tokio::sync::Mutex::new(Some(runtime)));

    #[cfg(unix)]
    let mut term = {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate()).expect("signal setup failed")
    };

    #[cfg(unix)]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("SIGINT received, shutting down");
            }
            _ = term.recv() => {
                info!("SIGTERM received, shutting down");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        info!("SIGINT received, shutting down");
    }

    handle.shutdown().await;

    let rt_opt = {
        let mut guard = runtime_holder.lock().await;
        guard.take()
    };
    if let Some(rt) = rt_opt {
        if let Err(e) = rt.shutdown().await {
            eprintln!("runtime shutdown error: {e}");
        }
    }
}
