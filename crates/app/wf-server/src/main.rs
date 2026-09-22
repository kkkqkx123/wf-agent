use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::info;

use wf_runtime::bootstrap::{InfraSourceConfig, Runtime, RuntimeConfig};

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

    /// Project root for file-layer config (configs/infrastructure)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Web frontend build directory served with SPA fallback. Wins over
    /// `WF_SERVER_STATIC_DIR` env and the `server.toml` file layer.
    #[arg(long)]
    static_dir: Option<PathBuf>,

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

    if let Some(level) = args.log_level.as_deref() {
        let lower = level.to_ascii_lowercase();
        let normalized = match lower.as_str() {
            "warning" => "warn",
            other => other,
        };
        config.log_config = config.log_config.with_level(normalized.to_string());
    }

    let source = args.config.clone().map(|path| InfraSourceConfig {
        project_root: Some(path),
        ..Default::default()
    });

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
    // `WF_SERVER_BIND_ADDR` env and the `server.toml` file layer.
    let server_config =
        wf_server::ServerConfig::resolve(args.config.as_deref(), args.addr, args.static_dir);
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

    let addr = server_config.bind_addr;

    let handle = if let Some(registry) = metrics_registry.clone() {
        info!(%addr, "starting wf-server with metrics");
        match wf_server::serve_full_with_config(registry, ctx.clone(), &server_config).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("bind failed at {addr}: {e}");
                std::process::exit(1);
            }
        }
    } else {
        info!(%addr, "starting wf-server");
        match wf_server::serve_api_with_config(ctx.clone(), &server_config).await {
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
