//! Background data fetching for the TUI screens.
//!
//! Each screen kind maps to an async fetch function that produces a
//! [`ScreenData`] model. The fetch runs on a background task against
//! `wf-server`; the draw path only sees the cached result.

mod catalog;
mod checkpoints;
mod common;
mod dashboard;
mod detail;
mod file_workspace;
mod sidebar;

pub use catalog::fetch_search_rows;
pub use checkpoints::{format_checkpoint_chain, format_checkpoint_detail};
pub use common::format_ts;
pub use detail::{fetch_exec_detail, fetch_workflow_detail};
pub use sidebar::{fetch_sidebar, SidebarFetch};

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{ExecStatusFilter, ScreenData, ScreenKind};

use catalog::{fetch_catalog, fetch_git, fetch_settings};
use checkpoints::fetch_checkpoints;
use dashboard::{fetch_dashboard, fetch_executions, fetch_insights, fetch_workflows};
use file_workspace::fetch_file_workspace;

/// Fetch data for the given screen kind. The flag reports server-side
/// truncation (`has_more`) so list titles stay honest about capped pages.
/// `execution_id` is the active session's execution, used by the
/// file-workspace screen to resolve its `agent:{id}` actor partition.
pub async fn fetch_for(
    client: &WfClient,
    kind: ScreenKind,
    filter: ExecStatusFilter,
    execution_id: Option<&str>,
) -> CliResult<(ScreenData, bool)> {
    match kind {
        ScreenKind::Dashboard => Ok((fetch_dashboard(client).await?, false)),
        ScreenKind::Workflow => fetch_workflows(client).await,
        ScreenKind::Executions => fetch_executions(client, filter).await,
        ScreenKind::Insights => Ok((fetch_insights(client).await?, false)),
        ScreenKind::Checkpoints => fetch_checkpoints(client).await,
        ScreenKind::Settings => Ok((fetch_settings(client).await?, false)),
        ScreenKind::Catalog => fetch_catalog(client).await,
        ScreenKind::Git => fetch_git(client).await,
        ScreenKind::FileWorkspace => fetch_file_workspace(client, execution_id).await,
        ScreenKind::Interactive => Ok((ScreenData::None, false)),
    }
}
