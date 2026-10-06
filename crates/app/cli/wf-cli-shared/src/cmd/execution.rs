//! Execution UX facade over the agent and workflow domains.
//!
//! Listing, show/status and lifecycle arms resolve both
//! `wf_api::agent::agent_loop_registry` and
//! `wf_api::workflow::{execution, workflow_execution}` (agent first,
//! workflow fallback); the `performance` / `bottleneck` / `errors` /
//! `compare` / `progress` arms delegate to the shared builders in
//! `crate::cmd::analysis` so both surfaces stay in sync. The
//! `hierarchy` / `subtree` / `history` arms read the cross-engine queries
//! in `wf_api::{execution_hierarchy, execution_history}`, which resolve the
//! owning engine from the id itself.

use crate::args::{Cli, ExecutionSub};
use crate::error::CliResult;

use super::execution_local::run_embedded;
use super::execution_remote::run_remote;

pub async fn run(cli: &Cli, sub: &ExecutionSub) -> CliResult<()> {
    let domain = crate::domain::DomainHandle::from_cli(cli, crate::mode::CliMode::Run).await?;
    if let Some(remote) = domain.as_remote() {
        return run_remote(cli, sub, remote.client()).await;
    }
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let result = run_embedded(cli, ctx, &domain, sub).await;
    domain.shutdown().await?;
    result
}
