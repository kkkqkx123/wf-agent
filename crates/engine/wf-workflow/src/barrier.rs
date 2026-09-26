#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BranchFailure {
    pub category: wf_types::workflow::error_branch::NodeErrorCategory,
    pub detail: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BranchResult {
    pub branch_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    /// Single structured failure for a settled branch. `None` on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<BranchFailure>,
    /// Snapshot of the branch's public variables (non-internal) taken when
    /// the branch settled. `None` for branches that expose no variables or
    /// failed before the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl BranchResult {
    pub fn success(branch_id: impl Into<String>, output: serde_json::Value) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: true,
            output,
            failure: None,
            variables: None,
        }
    }

    pub fn success_with_variables(
        branch_id: impl Into<String>,
        output: serde_json::Value,
        variables: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: true,
            output,
            failure: None,
            variables: Some(variables),
        }
    }

    pub fn failure(branch_id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: false,
            output: serde_json::Value::Null,
            failure: Some(BranchFailure {
                category:
                    wf_types::workflow::error_branch::NodeErrorCategory::BusinessFailure,
                detail: detail.into(),
            }),
            variables: None,
        }
    }

    pub fn with_category(
        mut self,
        category: wf_types::workflow::error_branch::NodeErrorCategory,
    ) -> Self {
        if let Some(ref mut failure) = self.failure {
            failure.category = category;
        }
        self
    }

    pub fn error_message(&self) -> Option<&str> {
        self.failure.as_ref().map(|f| f.detail.as_str())
    }

    pub fn error_category(
        &self,
    ) -> Option<wf_types::workflow::error_branch::NodeErrorCategory> {
        self.failure.as_ref().map(|f| f.category)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureStrategy {
    FailFast,
    ContinueOnError,
    FailOnThreshold { threshold: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkOutcome {
    Succeeded,
    Failed,
    Partial,
}

impl FailureStrategy {
    pub fn evaluate(&self, results: &[BranchResult]) -> ForkOutcome {
        let total = results.len();
        let failures = results.iter().filter(|r| !r.success).count();

        match self {
            FailureStrategy::FailFast => {
                if failures > 0 {
                    ForkOutcome::Failed
                } else {
                    ForkOutcome::Succeeded
                }
            }
            FailureStrategy::ContinueOnError => {
                if failures > 0 {
                    ForkOutcome::Partial
                } else {
                    ForkOutcome::Succeeded
                }
            }
            FailureStrategy::FailOnThreshold { threshold } => {
                if total == 0 {
                    return ForkOutcome::Succeeded;
                }
                let failure_rate = failures as f64 / total as f64;
                if failure_rate > *threshold {
                    ForkOutcome::Failed
                } else if failures > 0 {
                    ForkOutcome::Partial
                } else {
                    ForkOutcome::Succeeded
                }
            }
        }
    }
}
