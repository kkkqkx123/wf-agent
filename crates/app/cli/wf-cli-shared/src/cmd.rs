//! Management command handlers, grouped by `wf-api` domain.
//!
//! - agent / workflow execution: `execution` (UX facade resolving both),
//!   `workflow`
//! - trigger: `trigger` (templates + firing ledger)
//! - template: `template`
//! - llm: `llm` (profiles / providers), `tool`, `script`
//! - entity: `message`, `variable`, `task`, `skill`
//! - checkpoint: `checkpoint` (records / files / agent loops), `approval`
//!   (change approvals over `wf_api::checkpoint::approval`)
//! - observation: `analysis` (plus the shared data builders reused by the
//!   `execution` convenience arms), `audit`, `query`, `search`
//! - system: `event`, `metrics`, `diagnostics`
//!
//! Every handler resolves its domain through
//! [`DomainHandle`](crate::domain::DomainHandle): `workflow` / `execution`
//! serve remote via `run_remote`, the rest require embedded and fail loudly
//! on `--remote` instead of silently running locally.
pub mod analysis;
pub mod approval;
pub mod audit;
pub mod checkpoint;
pub mod diagnostics;
pub mod event;
pub mod execution;
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
