//! Destructive / mutating actions dispatched from key routes: delete
//! workflow, pause/resume and cancel executions, default model selection,
//! checkpoint detail/chain/restore/delete/create/purge/undo-last-input,
//! file-workspace submit/merge/undo/redo/gc/reindex, edit-session
//! begin/rollback.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::data_cache::DataResult;
use super::TuiApp;

mod checkpoint;
mod conflict;
mod display;
mod file_sync;
mod workflow_approval;
mod workspace;

#[cfg(test)]
mod test;
