use tracing::{info, warn};

/// Resolve the configured code-context transport into the effective
/// service config plus an optional supervised sidecar.
///
/// External transport passes through after an advisory readiness probe
/// (a missed probe only warns; per-call degradation still applies).
/// Managed transport starts a local service process and rewrites the
/// config to its loopback address, so downstream consumers only ever
/// observe external configs. Any managed failure warns and keeps the
/// original config: bootstrap continues with per-call skips.
pub async fn resolve_code_context_transport(
    config: Option<wf_integration::CodeContextConfig>,
) -> (
    Option<wf_integration::CodeContextConfig>,
    Option<wf_integration::RunningSidecar>,
) {
    use wf_integration::TransportMode;
    let Some(config) = config else {
        return (None, None);
    };
    if !config.is_usable() {
        return (Some(config), None);
    }
    match config.transport.transport_mode {
        TransportMode::External => {
            if let Some(url) = config.external_base_url() {
                let healthy = wf_integration::probe_http(&format!("{url}/api/health"), 5_000).await;
                if !healthy {
                    warn!(
                        "Code-context service at {url} is unreachable at startup; folding and retrieval will skip until it responds"
                    );
                }
            }
            (Some(config), None)
        }
        TransportMode::Managed => match start_managed_code_context(&config).await {
            Ok((effective, sidecar)) => {
                info!(
                    "Managed code-context service ready at {}",
                    effective
                        .external_base_url()
                        .unwrap_or_else(|| "unknown address".into())
                );
                (Some(effective), Some(sidecar))
            }
            Err(e) => {
                warn!("Managed code-context service unavailable, degrading to skips: {e}");
                (Some(config), None)
            }
        },
    }
}

/// Start the supervised local code-context server for managed transport.
async fn start_managed_code_context(
    config: &wf_integration::CodeContextConfig,
) -> Result<
    (
        wf_integration::CodeContextConfig,
        wf_integration::RunningSidecar,
    ),
    String,
> {
    let port = if config.transport.managed_port == 0 {
        wf_integration::pick_loopback_port()
            .map_err(|e| format!("no free loopback port for managed service: {e}"))?
    } else {
        config.transport.managed_port
    };
    let config_path = wf_integration::write_cce_server_config("127.0.0.1", port)
        .map_err(|e| format!("managed service config not writable: {e}"))?;
    let (program, args, env) =
        wf_integration::cce_server_command(&config.transport.managed_binary, &config_path);
    let base_url = format!("http://127.0.0.1:{port}");
    let sidecar = wf_integration::start_sidecar(wf_integration::SidecarSpec {
        program,
        args,
        env,
        base_url: base_url.clone(),
        readiness_url: format!("{base_url}/api/health"),
        startup_timeout_ms: config.transport.managed_startup_timeout_ms,
        max_restarts: 3,
        temp_files: vec![config_path],
    })
    .await?;
    Ok((config.with_managed_address(base_url), sidecar))
}
