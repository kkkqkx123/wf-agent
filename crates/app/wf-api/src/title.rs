//! Screen-only title generation reading face.

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

#[derive(Debug, Clone)]
pub struct TitleParams {
    pub session_id: String,
    pub profile_id: String,
}

pub async fn title(ctx: &ApiContext, params: TitleParams) -> ApiResult<Option<String>> {
    title_with_cancel(ctx, params, None).await
}

pub async fn title_with_cancel(
    ctx: &ApiContext,
    params: TitleParams,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> ApiResult<Option<String>> {
    let session_id = params.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(ApiError::Validation(
            "title requires a session id".to_string(),
        ));
    }
    let profile_id = params.profile_id.trim().to_string();
    if profile_id.is_empty() {
        return Err(ApiError::Validation(
            "title requires a profile id".to_string(),
        ));
    }
    let history =
        match crate::agent::agent_message::conversation_history(ctx, &session_id, None).await {
            Ok(history) => history,
            Err(error) => {
                tracing::debug!(
                    target: "wf_api",
                    session_id = %session_id,
                    error = %error,
                    "title history read failed; returning no title"
                );
                return Ok(None);
            }
        };
    Ok(
        wf_execution_shared::title::request_title(&ctx.llm_gateway, &profile_id, &history, cancel)
            .await,
    )
}
