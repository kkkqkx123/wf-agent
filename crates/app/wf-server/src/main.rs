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
    /// Listen address, e.g. 127.0.0.1:3000
    #[arg(long, default_value = "127.0.0.1:3000")]
    addr: SocketAddr,

    /// Project root for file-layer config (configs/infrastructure)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Storage backend spec: memory | sqlite:<path> | sqlite | postgres:<conn>
    #[arg(long)]
    storage: Option<String>,

    /// Log level: trace|debug|info|warn|error
    #[arg(long)]
    log_level: Option<String>,
}

fn parse_storage_config(spec: Option<&str>) -> Option<wf_types::config::storage::StorageConfig> {
    let spec = spec?;
    if spec == "memory" {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        });
    }
    if spec == "sqlite" {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: String::new(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    if let Some(path) = spec.strip_prefix("sqlite:") {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: path.to_string(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    if let Some(conn) = spec.strip_prefix("postgres:") {
        let host = if conn.is_empty() || spec.starts_with("postgres://") {
            spec.to_string()
        } else {
            format!("postgres:{conn}")
        };
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Postgres,
            sqlite: None,
            postgres: Some(wf_types::config::storage::PostgresStorageConfig {
                host,
                port: 5432,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl: false,
                pool_size: None,
                min_connections: None,
                idle_timeout: None,
                connection_timeout: None,
                max_uses: None,
            }),
            app_name: None,
        });
    }
    if spec.starts_with("postgres://") {
        return Some(wf_types::config::storage::StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Postgres,
            sqlite: None,
            postgres: Some(wf_types::config::storage::PostgresStorageConfig {
                host: spec.to_string(),
                port: 5432,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl: false,
                pool_size: None,
                min_connections: None,
                idle_timeout: None,
                connection_timeout: None,
                max_uses: None,
            }),
            app_name: None,
        });
    }
    None
}

fn build_runtime_config(args: &Args) -> RuntimeConfig {
    let mut config = RuntimeConfig::default();

    if let Some(storage) = parse_storage_config(args.storage.as_deref()) {
        config.storage = storage;
    }

    if let Some(level) = args.log_level.as_deref() {
        let lower = level.to_ascii_lowercase();
        let normalized = match lower.as_str() {
            "warning" => "warn",
            other => other,
        };
        config.log_config = config.log_config.with_level(normalized.to_string());
    }

    if let Some(path) = args.config.clone() {
        config.infra = Some(InfraSourceConfig {
            project_root: Some(path),
            ..Default::default()
        });
    }

    // Ensure metrics http_addr reflects CLI --addr when no infra metrics is set,
    // and bind address always comes from CLI.
    // The runtime metrics config will be used to decide serve_full vs serve_api.
    // We keep config.metrics None unless infra provides it; the binary's addr
    // is always the listener addr regardless of metrics.

    config
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Some(spec) = args.storage.as_deref() {
        if parse_storage_config(Some(spec)).is_none() {
            eprintln!("invalid --storage '{spec}': expected 'memory' or 'sqlite:<path>' or 'postgres:<conn>'");
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

    let runtime_config = build_runtime_config(&args);
    let runtime = match Runtime::bootstrap(runtime_config).await {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("runtime bootstrap failed: {e}");
            std::process::exit(1);
        }
    };

    let ctx: Arc<wf_api::ApiContext> = runtime.api_context_arc();

    let metrics_registry = runtime.metrics().map(|m| m.registry().clone());

    let addr = args.addr;

    let handle = if let Some(registry) = metrics_registry.clone() {
        info!(%addr, "starting wf-server with metrics");
        match wf_server::serve_full(registry, ctx.clone(), addr).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("bind failed at {addr}: {e}");
                std::process::exit(1);
            }
        }
    } else {
        info!(%addr, "starting wf-server");
        match wf_server::serve_api(ctx.clone(), addr).await {
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
