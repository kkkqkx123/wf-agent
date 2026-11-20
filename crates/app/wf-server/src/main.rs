use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::info;

use wf_runtime::bootstrap::{InfraSourceConfig, Runtime, RuntimeConfig};
use wf_runtime::{LogFormat, LogLevel};
use wf_types::config::user_git::UserGitConfig;

fn parse_log_level(raw: &str) -> Result<LogLevel, String> {
    LogLevel::parse(raw).map_err(|err| err.to_string())
}

fn parse_log_format(raw: &str) -> Result<LogFormat, String> {
    LogFormat::parse(raw).map_err(|err| err.to_string())
}

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

    /// Log level: trace|debug|info|warn|warning|error|off
    #[arg(long, value_parser = parse_log_level)]
    log_level: Option<LogLevel>,

    /// Log format: full|compact|json|pretty
    #[arg(long, value_parser = parse_log_format)]
    log_format: Option<LogFormat>,

    /// Fixed Tokio worker count. Overrides `WF_SERVER_WORKER_THREADS`.
    #[arg(long)]
    worker_threads: Option<usize>,
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
}

fn build_runtime_config(args: &Args) -> (RuntimeConfig, Option<InfraSourceConfig>) {
    let mut config = RuntimeConfig::default();
    let storage_override = args
        .storage
        .as_deref()
        .and_then(wf_config::storage_spec::parse_storage_spec);

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

    if let Some(level) = args.log_level {
        config.log_config = config.log_config.with_level(level);
    }
    if let Some(format) = args.log_format {
        config.log_config = config.log_config.with_format(format);
    }

    let mut source = InfraSourceConfig {
        project_root: args.config.clone(),
        settings_dir: args.settings_dir.clone(),
        ..Default::default()
    };
    source.overrides.storage = storage_override;
    let source = if args.config.is_some() || source.overrides.storage.is_some() {
        Some(source)
    } else {
        None
    };

    (config, source)
}

fn main() {
    let args = Args::parse();
    let worker_threads = args
        .worker_threads
        .or_else(|| env_usize("WF_SERVER_WORKER_THREADS"));
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if let Some(count) = worker_threads {
        builder.worker_threads(count);
    }
    let runtime = builder
        .enable_all()
        .build()
        .expect("tokio runtime setup failed");
    runtime.block_on(run(args, worker_threads));
}

async fn run(args: Args, worker_threads: Option<usize>) {
    #[cfg(all(feature = "remote-node", feature = "backends"))]
    if true {
        eprintln!("remote-node must not enable the backends management domain");
        std::process::exit(2);
    }
    if let Some(spec) = args.storage.as_deref() {
        if let Err(e) = wf_config::storage_spec::parse_storage_spec_result(spec) {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let (mut runtime_config, infra_source) = build_runtime_config(&args);
    if worker_threads.is_some() {
        runtime_config.worker_threads = worker_threads;
    }
    if let Some(capacity) = env_usize("WF_SERVER_EVENT_BUS_CAPACITY") {
        runtime_config.event_bus_capacity = Some(capacity);
    }
    if let Some(capacity) = env_usize("WF_SERVER_ENTITY_CACHE_CAPACITY") {
        runtime_config.entity_cache_capacity = Some(capacity as u64);
    }
    if let Some(ttl) = env_usize("WF_SERVER_ENTITY_CACHE_TTL_SECS") {
        runtime_config.entity_cache_ttl_secs = Some(ttl as u64);
    }
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

    if let Some(statik) = server_config.static_config.as_ref() {
        info!(
            dir = %statik.dir.display(),
            index = %statik.index,
            fallback = statik.fallback,
            immutable_max_age_secs = statik.immutable_max_age_secs,
            "static frontend hosting enabled"
        );
    }

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

    let registry = metrics_registry.clone().unwrap_or_else(|| {
        tracing::info!(
            "metrics not configured; serving observability endpoints with an empty registry"
        );
        std::sync::Arc::new(wf_metrics::MetricsRegistry::default())
    });
    let handle = {
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
