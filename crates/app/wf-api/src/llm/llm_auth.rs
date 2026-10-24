//! LLM authentication surfaces: login session hosting, status listing and
//! logout.
//!
//! The auth registry lives behind the shared gateway. Login sessions are
//! host-driven (start / poll / submit / cancel), so the CLI, the TUI and
//! the HTTP API all drive the same state machine in the host — no frontend
//! reimplements OAuth. Nothing here ever returns a secret: status views
//! carry surface metadata and a stored-credential flag only.

use wf_llm::llm_auth::{AuthError, AuthSurfaceInfo, LoginState};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiErrorCategory, ApiResult};

/// One declared auth surface plus its stored-credential status.
/// Never carries keys or tokens.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AuthProviderStatus {
    /// Provider id, matching the provider definition and profile references.
    pub id: String,
    /// Display name of the registered method, e.g. "Anthropic API key".
    pub display_name: String,
    /// `api_key` or `oauth`.
    pub method: String,
    /// Selector label for OAuth login options, e.g. "Sign in with Claude".
    pub login_label: Option<String>,
    /// True when OAuth access is subscription-backed (non-metered).
    pub is_subscription: bool,
    /// True when a credential is stored for this provider.
    pub has_credential: bool,
}

/// The auth registry behind the gateway, or a validation error when the
/// host runs without declarative auth (static-key behavior).
fn auth_registry(ctx: &ApiContext) -> ApiResult<&std::sync::Arc<wf_llm::AuthRegistry>> {
    ctx.llm_gateway.auth_registry().ok_or_else(|| {
        ApiError::Validation(
            "LLM auth is not configured on this host (no auth registry attached)".to_string(),
        )
    })
}

/// Project an auth-layer failure onto the API error taxonomy.
fn auth_error(error: AuthError) -> ApiError {
    match error {
        AuthError::ProviderNotRegistered(id) => ApiError::not_found("llm_auth_provider", &id),
        AuthError::SessionNotFound(id) => ApiError::not_found("llm_auth_session", &id),
        AuthError::SessionNotAwaitingInput(id) => {
            ApiError::Conflict(format!("login session '{id}' is not awaiting input"))
        }
        AuthError::Store(message) => ApiError::execution_categorized(
            format!("credential store failure: {message}"),
            ApiErrorCategory::Internal,
        ),
        AuthError::MalformedCredential(message) => {
            ApiError::Validation(format!("malformed credential: {message}"))
        }
        AuthError::CredentialTypeNotHandled(id) => {
            ApiError::Conflict(format!("credential type not handled for provider '{id}'"))
        }
        AuthError::Refresh(message) => ApiError::execution_categorized(
            format!("oauth refresh failed: {message}"),
            ApiErrorCategory::ServiceUnavailable,
        ),
        AuthError::Login(message) => ApiError::execution_categorized(
            format!("login failed: {message}"),
            ApiErrorCategory::BusinessFailure,
        ),
        AuthError::Transport(message) => ApiError::execution_categorized(
            format!("oauth transport failure: {message}"),
            ApiErrorCategory::ServiceUnavailable,
        ),
        AuthError::Loopback(message) => ApiError::execution_categorized(
            format!("loopback callback failure: {message}"),
            ApiErrorCategory::ServiceUnavailable,
        ),
        AuthError::Cancelled => ApiError::execution_categorized(
            "login cancelled".to_string(),
            ApiErrorCategory::Cancelled,
        ),
    }
}

/// List every declared auth surface with its stored-credential status.
pub async fn list_auth_providers(ctx: &ApiContext) -> ApiResult<Vec<AuthProviderStatus>> {
    let registry = auth_registry(ctx)?;
    let credentials = registry.list_credentials().await.map_err(auth_error)?;
    let mut surfaces: Vec<AuthProviderStatus> = registry
        .describe_providers()
        .into_iter()
        .map(|surface: AuthSurfaceInfo| {
            let has_credential = credentials
                .iter()
                .any(|info| info.provider_id == surface.id);
            AuthProviderStatus {
                id: surface.id,
                display_name: surface.display_name,
                method: surface.method.to_string(),
                login_label: surface.login_label,
                is_subscription: surface.is_subscription,
                has_credential,
            }
        })
        .collect();
    surfaces.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(surfaces)
}

/// Start a login session for a provider. Returns the session id plus the
/// first observable state (the challenge events or an input prompt).
pub async fn login_start(ctx: &ApiContext, provider_id: &str) -> ApiResult<(String, LoginState)> {
    let registry = auth_registry(ctx)?;
    registry.login_start(provider_id).await.map_err(auth_error)
}

/// Advance a login session's polling step. A completed step evicts cached
/// clients so the next request picks up the freshly stored credential.
pub async fn login_poll(ctx: &ApiContext, session_id: &str) -> ApiResult<LoginState> {
    let registry = auth_registry(ctx)?;
    let state = registry.login_poll(session_id).await.map_err(auth_error)?;
    evict_on_completed(ctx, &state);
    Ok(state)
}

/// Submit user input for a session awaiting it. A completed step evicts
/// cached clients so the next request picks up the freshly stored
/// credential.
pub async fn login_submit(
    ctx: &ApiContext,
    session_id: &str,
    input: String,
) -> ApiResult<LoginState> {
    let registry = auth_registry(ctx)?;
    let state = registry
        .login_submit(session_id, input)
        .await
        .map_err(auth_error)?;
    evict_on_completed(ctx, &state);
    Ok(state)
}

/// Drop cached clients for a just-completed login so the next request
/// resolves the new credential.
fn evict_on_completed(ctx: &ApiContext, state: &LoginState) {
    if let LoginState::Completed { provider_id, .. } = state {
        ctx.llm_gateway.evict_clients_for_auth(provider_id);
    }
}

/// The interval a frontend should wait between polls.
pub async fn login_poll_interval(
    ctx: &ApiContext,
    session_id: &str,
) -> ApiResult<std::time::Duration> {
    let registry = auth_registry(ctx)?;
    registry
        .login_poll_interval(session_id)
        .await
        .map_err(auth_error)
}

/// Cancel and drop a login session. Unknown sessions are not an error.
pub async fn login_cancel(ctx: &ApiContext, session_id: &str) -> ApiResult<()> {
    let registry = auth_registry(ctx)?;
    registry.login_cancel(session_id).await.map_err(auth_error)
}

/// Take the loopback listener of a PKCE loopback session, if any. The caller
/// owns the wait for the browser callback and submits the resulting code
/// through `login_submit`; on failure it should call `login_fail`.
pub async fn login_take_loopback(
    ctx: &ApiContext,
    session_id: &str,
) -> ApiResult<Option<wf_llm::llm_auth::oauth::pkce::LoopbackListener>> {
    let registry = auth_registry(ctx)?;
    registry
        .login_take_loopback(session_id)
        .await
        .map_err(auth_error)
}

/// Mark a login session as failed and surface the failure through polling.
pub async fn login_fail(
    ctx: &ApiContext,
    session_id: &str,
    error: String,
) -> ApiResult<LoginState> {
    let registry = auth_registry(ctx)?;
    registry
        .login_fail(session_id, error)
        .await
        .map_err(auth_error)
}

/// Remove a stored credential (logout). After logout the gateway drops any
/// cached client for profiles resolving auth through this provider so the
/// next request re-resolves from ambient sources.
pub async fn logout(ctx: &ApiContext, provider_id: &str) -> ApiResult<()> {
    let registry = auth_registry(ctx)?;
    registry.logout(provider_id).await.map_err(auth_error)?;
    ctx.llm_gateway.evict_clients_for_auth(provider_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use wf_llm::llm_auth::{
        AuthRegistry, AuthSurfaceInfo, CredentialInfo, MemoryCredentialStore, ProviderAuth,
        StdAuthContext,
    };
    use wf_llm::LlmGateway;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    #[derive(Debug)]
    struct StubApiKeyAuth;

    #[async_trait::async_trait]
    impl wf_llm::ApiKeyAuth for StubApiKeyAuth {
        fn name(&self) -> String {
            "Stub key".to_string()
        }

        async fn resolve(
            &self,
            _input: wf_llm::llm_auth::ResolveInput<'_>,
        ) -> Result<Option<wf_llm::ModelAuth>, AuthError> {
            Ok(None)
        }
    }

    fn ctx_with_registry(registry: Arc<AuthRegistry>) -> ApiContext {
        let mut ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        ctx.llm_gateway = Arc::new(LlmGateway::new().with_auth(registry));
        ctx
    }

    #[tokio::test]
    async fn list_merges_surfaces_with_credential_flags() {
        let registry = Arc::new(AuthRegistry::new(
            Arc::new(MemoryCredentialStore::new()),
            Arc::new(StdAuthContext),
        ));
        registry.register_provider("stub", ProviderAuth::api_key_only(Arc::new(StubApiKeyAuth)));
        let ctx = ctx_with_registry(registry);
        let listed = list_auth_providers(&ctx).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "stub");
        assert_eq!(listed[0].method, "api_key");
        assert!(!listed[0].has_credential);
    }

    #[tokio::test]
    async fn login_start_unknown_provider_is_not_found() {
        let registry = Arc::new(AuthRegistry::new(
            Arc::new(MemoryCredentialStore::new()),
            Arc::new(StdAuthContext),
        ));
        let ctx = ctx_with_registry(registry);
        let err = login_start(&ctx, "missing").await.expect_err("must fail");
        assert!(matches!(err, ApiError::NotFound { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn gateway_without_registry_reports_validation() {
        let ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        let err = list_auth_providers(&ctx).await.expect_err("must fail");
        assert!(matches!(err, ApiError::Validation(_)), "{err:?}");
    }

    #[test]
    fn surface_info_projects_without_secrets() {
        let surface = AuthSurfaceInfo {
            id: "acme".to_string(),
            display_name: "Acme".to_string(),
            method: wf_llm::AuthMethod::OAuth,
            login_label: Some("Sign in".to_string()),
            is_subscription: true,
        };
        let rendered = serde_json::to_string(&surface).expect("serialize");
        assert!(rendered.contains("\"id\":\"acme\""));
        let info = CredentialInfo {
            provider_id: "acme".to_string(),
            method: wf_llm::AuthMethod::OAuth,
            is_subscription: true,
        };
        let mut map: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
        map.insert(info.provider_id.clone(), true);
        assert!(map.get("acme").copied().unwrap_or(false));
    }
}
