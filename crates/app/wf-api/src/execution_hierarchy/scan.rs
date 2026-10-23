use wf_storage::domain::store::QueryFilter;
use wf_storage::domain::ExecutionIndexError;
use wf_storage::domain::ExecutionIndexRow;
use wf_storage::error::StorageError;
use wf_types::execution::ExecutionType;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub(super) async fn scan_rows(
    ctx: &ApiContext,
    kind: &ExecutionType,
    filter: &QueryFilter,
) -> ApiResult<(Vec<ExecutionIndexRow>, Vec<(String, ExecutionIndexError)>)> {
    let raw = match kind {
        ExecutionType::Workflow => {
            ctx.storage
                .workflow_execution
                .entity_store()
                .list_metadata(Some(filter))
                .await?
        }
        // Chat sessions are `agent_execution` rows, so the agent store
        // answers for both executors; the row kind is the owning store,
        // the true executor comes off the record itself.
        ExecutionType::Agent | ExecutionType::Chat => {
            ctx.storage
                .agent_execution
                .entity_store()
                .list_metadata(Some(filter))
                .await?
        }
    };
    let mut rows = Vec::new();
    let mut rejected = Vec::new();
    for (id, meta) in raw {
        match ExecutionIndexRow::parse(id.clone(), *kind, &meta) {
            Ok(row) => rows.push(row),
            Err(err) => rejected.push((id, err)),
        }
    }
    Ok((rows, rejected))
}

/// Read the indexed rows of every execution matching `filter`, across both
/// record kinds.
pub(super) async fn scan_all_rows(
    ctx: &ApiContext,
    filter: &QueryFilter,
) -> ApiResult<(Vec<ExecutionIndexRow>, Vec<(String, ExecutionIndexError)>)> {
    let mut rows = Vec::new();
    let mut rejected = Vec::new();
    for kind in KINDS {
        let (kind_rows, kind_rejected) = scan_rows(ctx, &kind, filter).await?;
        rows.extend(kind_rows);
        rejected.extend(kind_rejected);
    }
    Ok((rows, rejected))
}

/// Turn rows a scan could not read into their reported form.
pub(super) fn rejected_rows(rejected: Vec<(String, ExecutionIndexError)>) -> Vec<RejectedRow> {
    rejected
        .into_iter()
        .map(|(execution_id, err)| RejectedRow {
            reason: err.to_string(),
            execution_id,
        })
        .collect()
}

/// Read the rows of one entity kind for a query whose answer is about a single
/// execution whose own record has to be readable. An unreadable row is then
/// the failure rather than an omission, and reports as a conflict naming the
/// record.
pub(super) async fn read_rows(
    ctx: &ApiContext,
    kind: &ExecutionType,
    filter: &QueryFilter,
) -> ApiResult<Vec<ExecutionIndexRow>> {
    let (rows, rejected) = scan_rows(ctx, kind, filter).await?;
    if let Some((_, err)) = rejected.into_iter().next() {
        return Err(StorageError::from(err).into());
    }
    Ok(rows)
}

/// Read one execution's indexed row from the store of the given kind. A miss
/// means the record is absent, not that it lacks a hierarchy.
pub async fn read_row(
    ctx: &ApiContext,
    kind: &ExecutionType,
    id: &str,
) -> ApiResult<Option<ExecutionIndexRow>> {
    let filter = QueryFilter::new().with_id(id);
    Ok(read_rows(ctx, kind, &filter)
        .await?
        .into_iter()
        .find(|row| row.id() == id))
}
