use serde::{Deserialize, Serialize};

use crate::errors::{ErrorKind, ErrorType};

/// Node-level config key marking a node as an error suspend point: entering
/// it through an error route parks the execution for external handling
/// instead of continuing immediately.
pub const ERROR_SUSPEND_POINT_KEY: &str = "error_suspend_point";

/// Node-level config key marking a node as an explicit error-branch merge
/// point: when a node carrying this flag runs inside an active error branch,
/// isolated branch writes are flushed back to the main path (last-writer-wins
/// with audit) and the error namespace is reclaimed.
pub const ERROR_MERGE_POINT_KEY: &str = "error_merge_point";

/// Prefix of the read-only error namespace injected while an error branch
/// runs (`error.message`, `error.category`, `error.source_node`,
/// `error.attempts`, plus the `error` object itself). This is business-visible
/// read-only data for branch nodes, not engine machinery.
pub const ERROR_NAMESPACE: &str = "error";

/// Summary characters kept in audit payloads and hook metadata. Error routing
/// events only carry summaries, never full context.
pub const ERROR_SUMMARY_CHARS: usize = 160;

/// Terminal failure category of a node. Only terminal failures route;
/// transient failures spent inside handler retries never reach the table.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeErrorCategory {
    /// Transport and timeout failures after the retry channel is exhausted.
    TransportTimeout,
    /// Node business failures (script non-zero exit, tool errors, LLM
    /// refusals and other handler-level rejections).
    BusinessFailure,
    /// Cancellation and interruption (external stop, budget fuse).
    CancelledInterrupted,
    /// Compression failures (existing compression failure event wording).
    CompressionFailure,
    /// Resource/quota exhaustion after the retry channel is exhausted:
    /// rate limiting, upstream service overload, context-capacity pressure.
    Resource,
}

impl NodeErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TransportTimeout => "transport_timeout",
            Self::BusinessFailure => "business_failure",
            Self::CancelledInterrupted => "cancelled_interrupted",
            Self::CompressionFailure => "compression_failure",
            Self::Resource => "resource",
        }
    }

    /// Lossy projection of the shared error taxonomy onto the routing
    /// category. This is the single mapping both engines route and record by:
    /// quota/upstream pressure collapses into `Resource`, and every type
    /// without a dedicated category stays a business failure.
    pub fn from_error_type(error_type: &ErrorType) -> Self {
        match error_type {
            ErrorType::Interruption => Self::CancelledInterrupted,
            ErrorType::Timeout => Self::TransportTimeout,
            ErrorType::RateLimited | ErrorType::ServiceUnavailable => Self::Resource,
            _ => Self::BusinessFailure,
        }
    }

    /// Reverse projection used to persist structured error records for a
    /// routed failure. `Resource` reads back as `ServiceUnavailable` (its
    /// dominant upstream-pressure shape); compression has no dedicated
    /// `ErrorType` yet and records as `Internal`.
    pub fn error_type(&self) -> ErrorType {
        match self {
            Self::TransportTimeout => ErrorType::Timeout,
            Self::CancelledInterrupted => ErrorType::Interruption,
            Self::Resource => ErrorType::ServiceUnavailable,
            Self::BusinessFailure | Self::CompressionFailure => ErrorType::Internal,
        }
    }

    /// Kind bucket matching the `error_type()` projection above.
    pub fn error_kind(&self) -> ErrorKind {
        match self {
            Self::TransportTimeout => ErrorKind::Timeout,
            Self::Resource => ErrorKind::Resource,
            Self::BusinessFailure | Self::CancelledInterrupted | Self::CompressionFailure => {
                ErrorKind::Execution
            }
        }
    }
}

impl std::fmt::Display for NodeErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured config carried by an error edge (`EdgeType::Error`) and reused
/// by the workflow-level catch-all: which terminal failure categories the
/// route matches and whether the jump parks at a suspend point.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ErrorRouteConfig {
    /// Matching categories; absent or empty means catch-all. A catch-all
    /// deliberately does NOT match `CancelledInterrupted`: cancellation must
    /// not be swallowed by an unintentionally broad route, so capturing
    /// budget-fuse style cancellations requires listing the category
    /// explicitly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<NodeErrorCategory>>,
    /// Tri-state suspend flag: an explicit true/false wins; when absent the
    /// target node's `error_suspend_point` marker decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
}

impl ErrorRouteConfig {
    pub fn matches(&self, category: NodeErrorCategory) -> bool {
        match &self.categories {
            None => category != NodeErrorCategory::CancelledInterrupted,
            Some(list) if list.is_empty() => category != NodeErrorCategory::CancelledInterrupted,
            Some(list) => list.contains(&category),
        }
    }
}

/// Workflow-level catch-all default (declared on the workflow config, not on
/// any node or edge). Used only when no node-level error edge matches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowErrorDefault {
    pub target_node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
}

/// Resolved jump selected from the routing table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorRouteTarget {
    pub target_node_id: String,
    pub suspend: bool,
}

/// Read-only error summary injected into the error namespace on branch
/// entry. Only summary fields travel to audit and hook payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorBranchSummary {
    pub message: String,
    pub category: NodeErrorCategory,
    pub source_node_id: String,
    pub attempts: u32,
}

impl ErrorBranchSummary {
    pub fn new(
        message: impl Into<String>,
        category: NodeErrorCategory,
        source_node_id: impl Into<String>,
        attempts: u32,
    ) -> Self {
        Self {
            message: truncate_summary(&message.into()),
            category,
            source_node_id: source_node_id.into(),
            attempts: attempts.max(1),
        }
    }

    /// Variable entries injected for the branch (`error` object plus dotted
    /// leaves for `${error.message}` style lookups).
    pub fn variables(&self) -> Vec<(String, serde_json::Value)> {
        let object = serde_json::json!({
            "message": self.message,
            "category": self.category.as_str(),
            "source_node": self.source_node_id,
            "attempts": self.attempts,
        });
        vec![
            (ERROR_NAMESPACE.to_string(), object),
            (
                format!("{ERROR_NAMESPACE}.message"),
                serde_json::Value::String(self.message.clone()),
            ),
            (
                format!("{ERROR_NAMESPACE}.category"),
                serde_json::Value::String(self.category.as_str().to_string()),
            ),
            (
                format!("{ERROR_NAMESPACE}.source_node"),
                serde_json::Value::String(self.source_node_id.clone()),
            ),
            (
                format!("{ERROR_NAMESPACE}.attempts"),
                serde_json::Value::Number(self.attempts.into()),
            ),
        ]
    }
}

/// Minimal suspend context parked as a first-class execution-state and
/// checkpoint-snapshot field so a crashed suspend resumes from storage.
/// It never travels inside the business variable map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorSuspendState {
    pub summary: ErrorBranchSummary,
    pub target_node_id: String,
    pub suspended_at: i64,
}

/// Keep audit payloads slim: cap summaries to a short prefix.
pub fn truncate_summary(message: &str) -> String {
    message.chars().take(ERROR_SUMMARY_CHARS).collect()
}

/// Whether a node config marks its node as an error suspend point.
pub fn is_suspend_point(inner: &serde_json::Value) -> bool {
    inner
        .get(ERROR_SUSPEND_POINT_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Whether a node config marks its node as an explicit error-branch merge
/// point.
pub fn is_merge_point(inner: &serde_json::Value) -> bool {
    inner
        .get(ERROR_MERGE_POINT_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_type_category_projection_is_consistent() {
        assert_eq!(
            NodeErrorCategory::from_error_type(&ErrorType::Interruption),
            NodeErrorCategory::CancelledInterrupted
        );
        assert_eq!(
            NodeErrorCategory::from_error_type(&ErrorType::Timeout),
            NodeErrorCategory::TransportTimeout
        );
        assert_eq!(
            NodeErrorCategory::from_error_type(&ErrorType::RateLimited),
            NodeErrorCategory::Resource
        );
        assert_eq!(
            NodeErrorCategory::from_error_type(&ErrorType::ServiceUnavailable),
            NodeErrorCategory::Resource
        );
        assert_eq!(
            NodeErrorCategory::from_error_type(&ErrorType::LlmError),
            NodeErrorCategory::BusinessFailure
        );
        // Categories that survive a round trip keep their routing semantics;
        // compression intentionally records as Internal until it owns an
        // ErrorType.
        for category in [
            NodeErrorCategory::TransportTimeout,
            NodeErrorCategory::CancelledInterrupted,
            NodeErrorCategory::Resource,
        ] {
            assert_eq!(
                NodeErrorCategory::from_error_type(&category.error_type()),
                category
            );
        }
        assert_eq!(
            NodeErrorCategory::Resource.error_type(),
            ErrorType::ServiceUnavailable
        );
        assert_eq!(
            NodeErrorCategory::Resource.error_kind(),
            ErrorKind::Resource
        );
        assert_eq!(
            NodeErrorCategory::TransportTimeout.error_kind(),
            ErrorKind::Timeout
        );
    }

    #[test]
    fn category_roundtrip_is_strict() {
        for category in [
            NodeErrorCategory::TransportTimeout,
            NodeErrorCategory::BusinessFailure,
            NodeErrorCategory::CancelledInterrupted,
            NodeErrorCategory::CompressionFailure,
            NodeErrorCategory::Resource,
        ] {
            let text = serde_json::to_value(category).expect("serialize");
            let back: NodeErrorCategory = serde_json::from_value(text).expect("deserialize");
            assert_eq!(back, category);
        }
        // Unknown category strings are a hard deserialization error.
        let bad: Result<NodeErrorCategory, _> = serde_json::from_str("\"bogus\"");
        assert!(bad.is_err());
    }

    #[test]
    fn catch_all_excludes_cancellation() {
        let catch_all = ErrorRouteConfig::default();
        assert!(catch_all.matches(NodeErrorCategory::BusinessFailure));
        assert!(catch_all.matches(NodeErrorCategory::TransportTimeout));
        assert!(catch_all.matches(NodeErrorCategory::CompressionFailure));
        assert!(catch_all.matches(NodeErrorCategory::Resource));
        assert!(!catch_all.matches(NodeErrorCategory::CancelledInterrupted));

        let empty_list = ErrorRouteConfig {
            categories: Some(vec![]),
            suspend: None,
        };
        assert!(!empty_list.matches(NodeErrorCategory::CancelledInterrupted));

        let explicit = ErrorRouteConfig {
            categories: Some(vec![NodeErrorCategory::CancelledInterrupted]),
            suspend: None,
        };
        assert!(explicit.matches(NodeErrorCategory::CancelledInterrupted));
        assert!(!explicit.matches(NodeErrorCategory::BusinessFailure));
    }

    #[test]
    fn summary_truncates_and_exposes_namespace() {
        let long = "x".repeat(ERROR_SUMMARY_CHARS + 40);
        let summary = ErrorBranchSummary::new(long, NodeErrorCategory::BusinessFailure, "flaky", 0);
        assert_eq!(summary.message.chars().count(), ERROR_SUMMARY_CHARS);
        assert_eq!(summary.attempts, 1);
        let vars = summary.variables();
        assert!(vars.iter().any(|(k, _)| k == "error"));
        assert!(vars.iter().any(|(k, _)| k == "error.message"));
    }
}
