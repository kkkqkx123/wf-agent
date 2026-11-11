//! Execution lifecycle controls: cancel, pause and resume.

use crate::error::{CliError, CliResult};
use wf_client::WfClient;

pub(crate) async fn cancel_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.cancel_execution(id).await {
        return Ok(data);
    }
    client
        .cancel_agent_loop(id)
        .await
        .map_err(|e| CliError::Business(format!("execution [{id}] cancel failed: {e}")))
}

pub(crate) async fn pause_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.pause_execution(id).await {
        return Ok(data);
    }
    client
        .pause_agent_loop(id)
        .await
        .map_err(|e| CliError::Business(format!("execution [{id}] pause failed: {e}")))
}

pub(crate) async fn resume_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.resume_execution(id).await {
        return Ok(data);
    }
    client
        .resume_agent_loop(id)
        .await
        .map_err(|e| CliError::Business(format!("execution [{id}] resume failed: {e}")))
}
