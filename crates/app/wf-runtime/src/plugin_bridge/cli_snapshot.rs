use wf_plugin::ContributionManager;

pub(crate) fn build_cli_snapshot(manager: &ContributionManager) -> wf_types::cli::CliSnapshot {
    use wf_types::cli::*;

    let mut commands: Vec<CliCommandView> = manager
        .cli_command_snapshot()
        .into_iter()
        .map(|command| CliCommandView {
            id: command.id,
            title: command.title,
            group: command.group,
            description: command.description,
            slash_name: command.slash_name,
            slash_aliases: command.slash_aliases,
            slash_argument: command.slash_argument,
            palette: command.palette,
            key_hint: command.key_hint,
            when: CliCommandCondition {
                screens: command.when.screens,
                roles: command.when.roles,
                require_connected: command.when.require_connected,
                hide_when_overlay: command.when.hide_when_overlay,
            },
        })
        .collect();
    commands.sort_by(|a, b| a.id.cmp(&b.id));
    let mut completions: Vec<CliCompletionSource> = manager
        .completion_provider_snapshot()
        .into_iter()
        .map(|source| CliCompletionSource {
            id: source.id,
            trigger_chars: source.trigger_chars,
            items: source
                .items
                .into_iter()
                .map(|item| CliCompletionItem {
                    value: item.value,
                    label: item.label,
                    description: item.description,
                })
                .collect(),
        })
        .collect();
    completions.sort_by(|a, b| a.id.cmp(&b.id));
    let mut settings: Vec<CliSettingField> = manager
        .setting_field_snapshot()
        .into_iter()
        .map(|field| CliSettingField {
            key: field.key,
            label: field.label,
            values: field.values,
            fallback: field.fallback,
        })
        .collect();
    settings.sort_by(|a, b| a.key.cmp(&b.key));
    let mut tool_summaries: Vec<CliToolSummary> = manager
        .tool_summary_snapshot()
        .into_iter()
        .map(|summary| CliToolSummary {
            tool_name: summary.tool_name,
            call_template: summary.call_template,
            result_template: summary.result_template,
        })
        .collect();
    tool_summaries.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
    let mut markdown_rules: Vec<CliMarkdownRule> = manager
        .markdown_transformer_snapshot()
        .into_iter()
        .map(|rule| CliMarkdownRule {
            id: rule.id,
            pattern: rule.pattern,
            replacement: rule.replacement,
            surfaces: rule.surfaces,
        })
        .collect();
    markdown_rules.sort_by(|a, b| a.id.cmp(&b.id));
    let mut hasher = blake3::Hasher::new();
    for command in &commands {
        hasher.update(b"cli-command\0");
        hasher.update(command.id.as_bytes());
        hasher.update(b"\0");
    }
    for source in &completions {
        hasher.update(b"completion-provider\0");
        hasher.update(source.id.as_bytes());
        hasher.update(b"\0");
    }
    for field in &settings {
        hasher.update(b"setting-field\0");
        hasher.update(field.key.as_bytes());
        hasher.update(b"\0");
    }
    for summary in &tool_summaries {
        hasher.update(b"tool-summary\0");
        hasher.update(summary.tool_name.as_bytes());
        hasher.update(b"\0");
    }
    for rule in &markdown_rules {
        hasher.update(b"markdown-transformer\0");
        hasher.update(rule.id.as_bytes());
        hasher.update(b"\0");
    }
    let digest = hasher.finalize();
    let mut revision_bytes = [0u8; 8];
    revision_bytes.copy_from_slice(&digest.as_bytes()[..8]);
    CliSnapshot {
        revision: u64::from_le_bytes(revision_bytes),
        commands,
        completions,
        settings,
        tool_summaries,
        markdown_rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_api::infra::handler_chain::PluginHandlerSource;
    use wf_plugin::ContributionRegistrar;

    use crate::plugin_bridge::handler_source::WfPluginHandlerSource;

    fn snapshot_command(id: &str, slash: &str) -> wf_plugin::PluginCliCommand {
        wf_plugin::PluginCliCommand {
            id: id.into(),
            title: "Review".into(),
            group: "Review".into(),
            description: "Review the diff".into(),
            slash_name: Some(slash.into()),
            slash_aliases: Vec::new(),
            slash_argument: String::new(),
            palette: true,
            key_hint: None,
            when: wf_plugin::PluginCliCondition::default(),
        }
    }

    #[test]
    fn cli_snapshot_projects_all_five_contribution_kinds_sorted() {
        let manager = Arc::new(ContributionManager::new());
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_cli_command(snapshot_command("zeta.run", "zeta"))
            .unwrap();
        manager
            .as_registrar()
            .register_cli_command(snapshot_command("alpha.run", "alpha"))
            .unwrap();
        manager
            .as_registrar()
            .register_completion_provider(wf_plugin::PluginCompletionSource {
                id: "alpha.paths".to_string(),
                trigger_chars: vec!["@".to_string()],
                items: vec![wf_plugin::PluginCompletionItem {
                    value: "src/".to_string(),
                    label: None,
                    description: None,
                }],
            })
            .unwrap();
        manager
            .as_registrar()
            .register_setting_field(wf_plugin::PluginSettingField {
                key: "alpha.level".to_string(),
                label: "Level".to_string(),
                values: vec!["low".to_string()],
                fallback: "low".to_string(),
            })
            .unwrap();
        manager
            .as_registrar()
            .register_tool_summary(wf_plugin::PluginToolSummary {
                tool_name: "grep".to_string(),
                call_template: "search {args}".to_string(),
                result_template: String::new(),
            })
            .unwrap();
        manager
            .as_registrar()
            .register_markdown_transformer(wf_plugin::PluginMarkdownRule {
                id: "alpha.tickets".to_string(),
                pattern: "[T]".to_string(),
                replacement: "ticket".to_string(),
                surfaces: Vec::new(),
            })
            .unwrap();
        let source = WfPluginHandlerSource::new(manager);
        let snapshot = source.cli_snapshot();
        assert_eq!(
            snapshot
                .commands
                .iter()
                .map(|command| command.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha.run", "zeta.run"],
            "snapshot commands sort by id for deterministic consumers"
        );
        assert_eq!(snapshot.completions.len(), 1);
        assert_eq!(snapshot.settings.len(), 1);
        assert_eq!(snapshot.tool_summaries.len(), 1);
        assert_eq!(snapshot.markdown_rules.len(), 1);
        assert_ne!(snapshot.revision, 0, "non-empty snapshot fingerprints");
        assert!(!snapshot.is_empty());
    }

    #[test]
    fn cli_snapshot_revision_changes_with_content() {
        let manager = Arc::new(ContributionManager::new());
        manager.start_registration("p1");
        let source = WfPluginHandlerSource::new(manager.clone());
        let empty = source.cli_snapshot();
        assert!(empty.is_empty());
        manager
            .as_registrar()
            .register_cli_command(snapshot_command("alpha.run", "alpha"))
            .unwrap();
        let full = source.cli_snapshot();
        assert_ne!(
            empty.revision, full.revision,
            "revision distinguishes empty from populated snapshots"
        );
    }
}
