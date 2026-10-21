//! Transport types shared by the draft routes of every definition kind.
//!
//! A draft is validated and promoted the same way whatever kind of
//! definition it carries, so both kinds answer with the same issue shape.

use serde::Serialize;
use utoipa::ToSchema;

/// One validation issue on a draft: dotted field path plus message.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ValidationIssueDoc {
    pub(crate) field: String,
    pub(crate) message: String,
}

impl From<wf_types::ValidationError> for ValidationIssueDoc {
    fn from(view: wf_types::ValidationError) -> Self {
        Self {
            field: view.field,
            message: view.message,
        }
    }
}
