//! Management command handlers, grouped by domain.
//!
//! - agent / workflow execution: `execution` (UX facade resolving both),
//!   `workflow`
//! - trigger: `trigger` (templates + firing ledger)
//! - template: `template`
//! - llm: `llm` (profiles / providers), `tool`, `script`
//! - entity: `message`, `variable`, `task`, `skill`
//! - checkpoint: `checkpoint` (records / files / agent loops), `approval`
//!   (change approvals over the file-checkpoint approval API)
//! - observation: `analysis` (plus the shared data builders reused by the
//!   `execution` convenience arms), `audit`, `query`, `search`
//! - system: `event`, `metrics`, `diagnostics`
//!
//! Every handler resolves a [`ClientHandle`](crate::connection::ClientHandle)
//! and serves through `wf-server` HTTP; output rendering stays local so
//! established envelope names and text layouts do not change.
pub mod analysis;
pub mod approval;
pub mod audit;
pub mod checkpoint;
pub mod config;
pub mod diagnostics;
pub mod event;
pub mod execution;
pub mod execution_logs;
pub mod execution_stream;
pub mod execution_view;
pub mod llm;
pub mod message;
pub mod metrics;
pub mod query;
pub mod render;
pub mod script;
pub mod search;
pub mod skill;
pub mod task;
pub mod template;
pub mod tool;
pub mod trigger;
pub mod variable;
pub mod workflow;
