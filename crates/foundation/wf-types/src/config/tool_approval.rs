use serde::{Deserialize, Serialize};

use crate::tool::approval::ToolApprovalOptions;

/// Host-side default tool-level approval configuration.
///
/// The library contract stays opt-in: with no approval handler attached,
/// tool calls auto-approve. When the host enables this section, execution
/// construction paths uniformly attach the interaction-backed handler
/// together with the effective [`ToolApprovalOptions`] policy, so the
/// engine decides per tool call which ones auto-run, which are denied and
/// which open a persisted approval request for a human to answer.
///
/// There is deliberately no timeout here: an unanswered approval waits
/// while the process lives (the wall-clock budget is paused during the
/// wait). Whether a tool asks at all is a policy question answered by
/// [`Self::options`], not a question of how long to wait.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalConfig {
    /// Enable host-default tool-level approval. Library/default value is
    /// `false`; hosts turn it on in their infrastructure config as a
    /// product decision.
    #[serde(default)]
    pub enabled: bool,
    /// Optional policy override on top of the engine baseline. Every field
    /// left `None` inherits the balanced defaults (read-only tools run
    /// automatically, writes/executions ask, sensitive files always ask,
    /// protected files denied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ToolApprovalOptions>,
}

impl ToolApprovalConfig {
    /// Effective policy for executions launched under this config: the
    /// user-supplied options merged field-wise over the engine baseline,
    /// so a partial override (e.g. only `security_preset`) cannot silently
    /// drop baseline protections such as the sensitive-file ruleset.
    pub fn resolved_options(&self) -> ToolApprovalOptions {
        let base = ToolApprovalOptions::balanced_defaults();
        let Some(mut user) = self.options.clone() else {
            return base;
        };
        user.auto_approval_enabled = user.auto_approval_enabled.or(base.auto_approval_enabled);
        user.security_preset = user.security_preset.or(base.security_preset);
        user.risk_threshold = user.risk_threshold.or(base.risk_threshold);
        user.auto_approve_patterns = user.auto_approve_patterns.or(base.auto_approve_patterns);
        user.categories = user.categories.or(base.categories);
        user.workspace_boundary = user.workspace_boundary.or(base.workspace_boundary);
        user.file_permissions = user.file_permissions.or(base.file_permissions);
        user.command = user.command.or(base.command);
        user.mcp = user.mcp.or(base.mcp);
        user.network = user.network.or(base.network);
        user.interaction = user.interaction.or(base.interaction);
        user.allow_write_protected = user.allow_write_protected.or(base.allow_write_protected);
        user
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::approval::{ApprovalCategories, SecurityPreset};
    use crate::tool::file_permission::{
        FilePermissionLevel, FilePermissionRule, FilePermissionSettings,
    };

    #[test]
    fn default_stays_opt_in_without_options() {
        let config = ToolApprovalConfig::default();
        assert!(!config.enabled);
        assert!(config.options.is_none());
        // Resolved policy still equals the engine baseline.
        let resolved = config.resolved_options();
        assert_eq!(resolved.auto_approval_enabled, Some(true));
        assert_eq!(resolved.security_preset, Some(SecurityPreset::Balanced));
        assert!(resolved.file_permissions.is_some());
    }

    #[test]
    fn partial_override_keeps_baseline_fields() {
        let config = ToolApprovalConfig {
            enabled: true,
            options: Some(ToolApprovalOptions {
                security_preset: Some(SecurityPreset::Safe),
                ..ToolApprovalOptions::empty()
            }),
        };
        let resolved = config.resolved_options();
        assert_eq!(resolved.security_preset, Some(SecurityPreset::Safe));
        // Inherited from the baseline despite the sparse override.
        assert_eq!(resolved.auto_approval_enabled, Some(true));
        assert!(
            resolved.file_permissions.is_some(),
            "baseline sensitive-file rules must survive a sparse override"
        );
    }

    #[test]
    fn full_override_replaces_every_field() {
        let rules = FilePermissionSettings {
            rules: vec![FilePermissionRule {
                pattern: "**/*".to_string(),
                permission: FilePermissionLevel::Write,
                description: None,
            }],
            default_permission: Some(FilePermissionLevel::Write),
        };
        let config = ToolApprovalConfig {
            enabled: true,
            options: Some(ToolApprovalOptions {
                auto_approval_enabled: Some(true),
                security_preset: Some(SecurityPreset::Permissive),
                risk_threshold: Some("write".to_string()),
                auto_approve_patterns: Some(vec!["web_*".to_string()]),
                categories: Some(ApprovalCategories {
                    always_allow_read_only: Some(true),
                    always_allow_write: Some(true),
                    always_allow_execute: Some(false),
                    always_allow_mcp: None,
                    always_allow_network: None,
                    always_allow_interaction: None,
                }),
                file_permissions: Some(rules.clone()),
                command: None,
                mcp: None,
                network: None,
                interaction: None,
                workspace_boundary: None,
                allow_write_protected: Some(true),
            }),
        };
        let resolved = config.resolved_options();
        assert_eq!(resolved.security_preset, Some(SecurityPreset::Permissive));
        assert_eq!(
            resolved.file_permissions.as_ref(),
            Some(&rules),
            "an explicit ruleset replaces the baseline"
        );
        assert_eq!(resolved.allow_write_protected, Some(true));
    }

    #[test]
    fn empty_options_table_resolves_to_baseline() {
        // `[options]` present but empty must not degrade into ask-everything;
        // it resolves onto the balanced baseline like an absent section.
        let config: ToolApprovalConfig =
            toml::from_str("enabled = true\n\n[options]\n").expect("parse empty options table");
        assert!(config.enabled);
        let resolved = config.resolved_options();
        assert_eq!(resolved.auto_approval_enabled, Some(true));
        assert_eq!(resolved.security_preset, Some(SecurityPreset::Balanced));
    }
}
