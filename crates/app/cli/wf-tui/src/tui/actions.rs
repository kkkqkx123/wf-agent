//! Destructive / mutating actions dispatched from key routes: delete
//! workflow, pause/resume and cancel executions, default model selection,
//! checkpoint detail/chain/restore/delete/create/purge/undo-last-input,
//! file-workspace submit/merge/undo/redo/gc/reindex, edit-session
//! begin/rollback.

mod checkpoint;
mod conflict;
mod display;
mod file_sync;
mod workflow_approval;
mod workspace;

#[cfg(test)]
mod test;
