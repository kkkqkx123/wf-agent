//! CLI snapshot: versioned, data-only projection of plugin CLI
//! contributions.
//!
//! The server publishes this over `GET /api/v1/cli/snapshot`; shells
//! fetch it, re-validate every entry, and merge it with locally declared
//! plugins. Shapes mirror the `wf-plugin-sdk` contribution structs field
//! for field so both sides convert through plain `serde_json` without
//! cross-layer dependencies. The snapshot carries no code and no
//! expressions: templates stay literal text the host renders, so a
//! hostile server can at most change wording, never run code.

use serde::{Deserialize, Serialize};

/// Where a CLI command came from: declared on this machine (`local`) or
/// delivered by the server snapshot (`snapshot`). Builtins outrank both;
/// local entries shadow snapshot entries sharing one id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum CliCommandOrigin {
    /// Delivered by the server snapshot over HTTP.
    #[default]
    Snapshot,
    /// Declared by a plugin on this machine.
    Local,
}

/// Applicability condition of a snapshot command. Carried for fidelity;
/// shells ignore it until host-side condition evaluation lands.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliCommandCondition {
    /// Screen names admitting the command (empty means every screen).
    #[serde(default)]
    pub screens: Vec<String>,
    /// Identity roles admitting the command (empty means every role).
    #[serde(default)]
    pub roles: Vec<String>,
    /// Only offer the command while connected to the server.
    #[serde(default)]
    pub require_connected: bool,
    /// Hide the command while any overlay or modal is open.
    #[serde(default)]
    pub hide_when_overlay: bool,
}

/// One user-facing command in the snapshot: slash entry, palette row and
/// help line share this definition. Mirrors the SDK command shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliCommandView {
    /// Stable command id (`<plugin>.<verb>`); doubles as the registry key.
    pub id: String,
    /// Short display title for palette rows and help.
    pub title: String,
    /// Palette group and help section.
    pub group: String,
    /// One-line description for palette filtering and help.
    pub description: String,
    /// Slash name without the leading slash (`None` hides the slash entry
    /// while keeping the palette row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slash_name: Option<String>,
    /// Extra slash spellings, all without the leading slash.
    #[serde(default)]
    pub slash_aliases: Vec<String>,
    /// Argument syntax shown in help (`""` when the command takes none).
    #[serde(default)]
    pub slash_argument: String,
    /// Whether the command appears in the command palette.
    #[serde(default = "default_palette")]
    pub palette: bool,
    /// Display-only default key hint; the host keymap always wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    /// Applicability condition evaluated by the host (ignored for now).
    #[serde(default)]
    pub when: CliCommandCondition,
}

fn default_palette() -> bool {
    true
}

/// One static completion item in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliCompletionItem {
    /// Text inserted on accept.
    pub value: String,
    /// Display label (defaults to the value in the host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// One-line description shown beside the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One static completion source in the snapshot; `id` is the registry key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliCompletionSource {
    pub id: String,
    /// Trigger characters activating this source (`#`, `@`, `/`).
    pub trigger_chars: Vec<String>,
    /// Items offered while the trigger is active.
    #[serde(default)]
    pub items: Vec<CliCompletionItem>,
}

/// One settings row in the snapshot; `key` is the registry key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliSettingField {
    /// Dotted key (`<plugin>.<name>`); doubles as the registry key.
    pub key: String,
    /// Display label for the settings row.
    pub label: String,
    /// Allowed values; empty means free-form text.
    #[serde(default)]
    pub values: Vec<String>,
    /// Value used when nothing is stored.
    pub fallback: String,
}

/// One tool summary template in the snapshot; `tool_name` is the key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliToolSummary {
    /// Tool name this summary renders; doubles as the registry key.
    pub tool_name: String,
    /// Template for the pending/running call line.
    #[serde(default)]
    pub call_template: String,
    /// Template for the settled result line.
    #[serde(default)]
    pub result_template: String,
}

/// One Markdown rewrite rule in the snapshot; `id` is the registry key.
/// The pattern is always literal text, never an expression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliMarkdownRule {
    /// Rule id; doubles as the registry key.
    pub id: String,
    /// Literal text to replace (never empty, never a pattern).
    pub pattern: String,
    /// Replacement text.
    #[serde(default)]
    pub replacement: String,
    /// Surfaces the rule runs on; empty means every surface.
    #[serde(default)]
    pub surfaces: Vec<String>,
}

/// Versioned CLI snapshot published by the server. `revision` is a
/// content fingerprint, not a counter: shells compare it for equality
/// and replace the whole snapshot on change, never diffing entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CliSnapshot {
    /// Content fingerprint of the five contribution sets.
    pub revision: u64,
    pub commands: Vec<CliCommandView>,
    pub completions: Vec<CliCompletionSource>,
    pub settings: Vec<CliSettingField>,
    pub tool_summaries: Vec<CliToolSummary>,
    pub markdown_rules: Vec<CliMarkdownRule>,
}

impl CliSnapshot {
    /// Empty snapshot: builtin-only behavior, same as an unreachable or
    /// plugin-less server.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when every contribution set is empty (revision ignored).
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
            && self.completions.is_empty()
            && self.settings.is_empty()
            && self.tool_summaries.is_empty()
            && self.markdown_rules.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_has_zero_revision_and_no_entries() {
        let snapshot = CliSnapshot::empty();
        assert_eq!(snapshot.revision, 0);
        assert!(snapshot.is_empty());
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let snapshot = CliSnapshot {
            revision: 7,
            commands: vec![CliCommandView {
                id: "review.run".to_string(),
                title: "Run review".to_string(),
                group: "Review".to_string(),
                description: "Run the review flow".to_string(),
                slash_name: Some("review".to_string()),
                slash_aliases: vec!["rev".to_string()],
                slash_argument: String::new(),
                palette: true,
                key_hint: None,
                when: CliCommandCondition::default(),
            }],
            completions: vec![CliCompletionSource {
                id: "review.paths".to_string(),
                trigger_chars: vec!["@".to_string()],
                items: vec![CliCompletionItem {
                    value: "src/".to_string(),
                    label: None,
                    description: None,
                }],
            }],
            settings: vec![CliSettingField {
                key: "review.level".to_string(),
                label: "Level".to_string(),
                values: vec!["low".to_string(), "high".to_string()],
                fallback: "low".to_string(),
            }],
            tool_summaries: vec![CliToolSummary {
                tool_name: "grep".to_string(),
                call_template: "search {args}".to_string(),
                result_template: String::new(),
            }],
            markdown_rules: vec![CliMarkdownRule {
                id: "review.tickets".to_string(),
                pattern: "[T]".to_string(),
                replacement: "ticket".to_string(),
                surfaces: Vec::new(),
            }],
        };
        let text = serde_json::to_string(&snapshot).expect("serializes");
        let back: CliSnapshot = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(snapshot, back);
        assert!(!back.is_empty());
    }

    #[test]
    fn command_view_defaults_match_sdk_shapes() {
        let value = serde_json::json!({
            "id": "review.run",
            "title": "Run review",
            "group": "Review",
            "description": "Run the review flow",
        });
        let view: CliCommandView = serde_json::from_value(value).expect("deserializes");
        assert_eq!(view.slash_name, None);
        assert!(view.slash_aliases.is_empty());
        assert!(view.palette);
        assert_eq!(view.key_hint, None);
    }

    #[test]
    fn origin_defaults_to_snapshot_for_wire_data() {
        let origin: CliCommandOrigin = serde_json::from_str("\"snapshot\"").expect("deserializes");
        assert_eq!(origin, CliCommandOrigin::Snapshot);
        assert_eq!(CliCommandOrigin::default(), CliCommandOrigin::Snapshot);
    }
}
