//! Code-context maintenance surface: dead letters and the manual reindex.
//!
//! Reading and repairing a broken index is an operator action, never an
//! automatic one: the runtime reports what the service gave up on and lets a
//! human decide. This module owns the API-layer mapping only, the protocol
//! stays in `wf-integration` and the transport in `wf-server`.
//!
//! Maintenance deliberately works while retrieval does not: a project that is
//! still indexing sold its keyword and semantic channels and a project whose
//! index failed sold everything, yet both are exactly what needs inspecting.

use crate::infra::context::ApiContext;
use crate::infra::diagnostics::{self, CodeContextDiagnostic};
use crate::infra::error::{ApiError, ApiErrorCategory, ApiResult};

use wf_integration::{DeadLetterAction, DeadLetterList, OperationError, SharedCodeContextService};

/// Attached code-context service, or a transport-visible failure.
fn service(ctx: &ApiContext) -> ApiResult<&SharedCodeContextService> {
    ctx.code_context().ok_or_else(|| {
        ApiError::execution_categorized(
            "code-context service is not attached to this runtime",
            ApiErrorCategory::ServiceUnavailable,
        )
    })
}

/// Map a maintenance failure onto the API taxonomy: a local precondition stays
/// a validation problem (retrying cannot fix it), a service failure reports as
/// unavailable so callers back off instead of retrying immediately.
fn api_error(error: OperationError) -> ApiError {
    match error {
        OperationError::Rejected(reason) => ApiError::Validation(reason),
        OperationError::Service(reason) => {
            ApiError::execution_categorized(reason, ApiErrorCategory::ServiceUnavailable)
        }
    }
}

/// Dead letters of the workspace project.
pub async fn dead_letters(ctx: &ApiContext) -> ApiResult<DeadLetterList> {
    service(ctx)?.dead_letters().await.map_err(api_error)
}

/// Retry dead letters; an empty `files` list retries every candidate.
pub async fn retry_dead_letters(
    ctx: &ApiContext,
    files: Vec<String>,
) -> ApiResult<DeadLetterAction> {
    service(ctx)?
        .retry_dead_letters(files)
        .await
        .map_err(api_error)
}

/// Acknowledge one dead letter, for one module when named.
pub async fn acknowledge_dead_letter(
    ctx: &ApiContext,
    file_path: String,
    module: Option<String>,
) -> ApiResult<DeadLetterAction> {
    service(ctx)?
        .acknowledge_dead_letter(file_path, module)
        .await
        .map_err(api_error)
}

/// Run the workspace index again on operator request and report the state the
/// caller will observe next.
pub async fn retry_index(ctx: &ApiContext) -> ApiResult<CodeContextDiagnostic> {
    service(ctx)?.retry_index().await.map_err(api_error)?;
    Ok(diagnostics::code_context(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use wf_integration::{CodeContextConfig, CodeContextService, ServiceTransport};
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn ctx_with(service: Option<CodeContextService>) -> Arc<ApiContext> {
        let ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        Arc::new(match service {
            Some(service) => ctx.with_code_context(Arc::new(service)),
            None => ctx,
        })
    }

    /// External transport with the deployment owning the project: the
    /// configured id is the only project the runtime knows.
    fn external(base_url: &str) -> CodeContextService {
        let mut config = CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some(base_url.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        config.retrieval.default_project_id = Some(4);
        CodeContextService::external(config)
    }

    #[tokio::test]
    async fn unattached_service_reports_unavailable() {
        let ctx = ctx_with(None);
        let error = dead_letters(&ctx).await.expect_err("no service");
        assert!(
            matches!(error, ApiError::Execution { .. }),
            "expected an execution error, got {error:?}"
        );
    }

    #[tokio::test]
    async fn disabled_service_rejects_maintenance() {
        let ctx = ctx_with(Some(CodeContextService::disabled()));
        let error = dead_letters(&ctx).await.expect_err("disabled service");
        assert!(error.to_string().contains("disabled"), "{error}");
    }

    #[tokio::test]
    async fn reindex_is_rejected_without_runtime_supply() {
        // The deployment owns the project here, so a runtime reindex would
        // race with whoever actually indexes it.
        let ctx = ctx_with(Some(external("http://127.0.0.1:1")));
        let error = retry_index(&ctx).await.expect_err("no supply");
        assert!(error.to_string().contains("external"), "{error}");
    }

    #[tokio::test]
    async fn dead_letters_round_trip_through_the_service() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let server = tokio::spawn(async move { serve_dead_letters(listener).await });
        let ctx = ctx_with(Some(external(&format!("http://127.0.0.1:{port}"))));

        let list = dead_letters(&ctx).await.expect("list must decode");
        assert_eq!(list.project_id, 4);
        assert_eq!(list.files.len(), 1);
        assert_eq!(list.files[0].file_path, "src/big.rs");

        let action = retry_dead_letters(&ctx, vec!["src/big.rs".to_string()])
            .await
            .expect("retry must decode");
        assert!(action.success);
        assert_eq!(action.affected, 1);
        server.abort();
    }

    /// Minimal service: health, project list, dead-letter list and retry.
    async fn serve_dead_letters(listener: tokio::net::TcpListener) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let body = if request.contains("/api/health") {
                r#"{"healthy":true}"#.to_string()
            } else if request.contains("dead-letters/retry") {
                r#"{"success":true,"affected":1,"message":"retried 1 file"}"#.to_string()
            } else if request.contains("dead-letters") {
                r#"{"project_id":4,"files":[{"file_path":"src/big.rs","version":2,"updated_at":"2024-05-01T00:00:00Z","modules":[{"module":"embedding","retry_count":3,"truncated":false,"acknowledged":false}]}]}"#.to_string()
            } else if request.contains("/api/project") {
                r#"{"projects":[{"id":"4","root_path":"/tmp/wf-api-cce","last_indexed":"2024-05-01T00:00:00Z"}]}"#.to_string()
            } else {
                r#"{}"#.to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    }
}
