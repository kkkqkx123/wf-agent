use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::tool::approval::ToolApprovalOptions;
use crate::tool::path_grant::{expand_path_grants, PathGrantPreset, SCRATCH_PRESET};

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
    /// Named path authorization presets. Selected presets are expanded once
    /// when options resolve, and the resulting grants also feed the
    /// untracked write prefixes of the execution file scope.
    #[serde(default)]
    pub presets: BTreeMap<String, PathGrantPreset>,
    #[serde(default)]
    pub active_presets: Vec<String>,
}

impl ToolApprovalConfig {
    /// Effective policy for executions launched under this config: the
    /// user-supplied options merged field-wise over the engine baseline,
    /// so a partial override (e.g. only `security_preset`) cannot silently
    /// drop baseline protections such as the sensitive-file ruleset.
    pub fn resolved_options(&self) -> ToolApprovalOptions {
        let base = ToolApprovalOptions::balanced_defaults();
        let mut user = self
            .options
            .clone()
            .unwrap_or_else(ToolApprovalOptions::empty);
        user.auto_approval_enabled = user.auto_approval_enabled.or(base.auto_approval_enabled);
        user.security_preset = user.security_preset.or(base.security_preset);
        user.auto_approve_patterns = user.auto_approve_patterns.or(base.auto_approve_patterns);
        user.categories = user.categories.or(base.categories);
        user.file_permissions = user.file_permissions.or(base.file_permissions);
        user.command = user.command.or(base.command);
        user.mcp = user.mcp.or(base.mcp);
        user.network = user.network.or(base.network);
        user.allow_write_protected = user.allow_write_protected.or(base.allow_write_protected);
        user.path_grants = Some(merge_path_grants(
            user.path_grants.as_deref(),
            &expand_path_grants(&self.presets, &self.active_presets),
        ));
        user
    }

    /// Paths from active presets that must be admitted as untracked write
    /// prefixes so the execution scope can reach them without claiming they
    /// are restorable through checkpoints.
    pub fn untracked_write_prefixes(&self) -> Vec<String> {
        resolve_preset_grants(self)
            .into_iter()
            .filter(|grant| !grant.checkpointed && grant.operation.allows_write())
            .map(|grant| grant.pattern)
            .collect()
    }
}

fn resolve_preset_grants(config: &ToolApprovalConfig) -> Vec<crate::tool::path_grant::PathGrant> {
    expand_path_grants(&config.presets, &config.active_presets)
}

fn merge_path_grants(
    options: Option<&[crate::tool::path_grant::PathGrant]>,
    preset_grants: &[crate::tool::path_grant::PathGrant],
) -> Vec<crate::tool::path_grant::PathGrant> {
    let mut merged = preset_grants.to_vec();
    for grant in options.unwrap_or_default() {
        if !merged
            .iter()
            .any(|existing| existing.pattern == grant.pattern)
        {
            merged.push(grant.clone());
        }
    }
    merged
}

/// Validate preset selections: every active name must resolve to a declared
/// preset or to the built-in scratch preset.
pub fn validate_tool_approval_config(config: &ToolApprovalConfig) -> Result<(), String> {
    for name in &config.active_presets {
        if !config.presets.contains_key(name) && name != SCRATCH_PRESET {
            return Err(format!("unknown path authorization preset '{name}'"));
        }
    }
    for (name, preset) in &config.presets {
        if preset.paths.is_empty() {
            return Err(format!("path authorization preset '{name}' has no paths"));
        }
        for path in &preset.paths {
            if path.trim().is_empty() || path.contains('\0') {
                return Err(format!(
                    "path authorization preset '{name}' has an invalid path"
                ));
            }
        }
    }
    Ok(())
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
            ..ToolApprovalConfig::default()
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
                path_grants: None,
                command: None,
                mcp: None,
                network: None,
                allow_write_protected: Some(true),
            }),
            ..ToolApprovalConfig::default()
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

    #[test]
    fn active_presets_resolve_into_path_grants() {
        let config: ToolApprovalConfig =
            toml::from_str("enabled = true\nactive_presets = [\"scratch\"]\n")
                .expect("parse presets");
        assert!(validate_tool_approval_config(&config).is_ok());
        let options = config.resolved_options();
        let grants = options.path_grants.expect("grants");
        assert!(grants.iter().any(|grant| !grant.checkpointed));
        assert_eq!(
            config.untracked_write_prefixes(),
            grants
                .iter()
                .filter(|grant| !grant.checkpointed)
                .map(|grant| grant.pattern.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unknown_active_preset_fails_validation() {
        let config: ToolApprovalConfig =
            toml::from_str("enabled = true\nactive_presets = [\"missing\"]\n")
                .expect("parse presets");
        assert!(validate_tool_approval_config(&config).is_err());
    }
}
