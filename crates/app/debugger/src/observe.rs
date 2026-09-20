use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::MessageView;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableDiff {
    pub key: String,
    pub kind: ChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
    #[serde(default)]
    pub text_diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageDiff {
    pub context_id: String,
    pub message_id: String,
    pub kind: ChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<MessageView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<MessageView>,
}

pub fn variable_diffs(
    before: &HashMap<String, serde_json::Value>,
    after: &HashMap<String, serde_json::Value>,
) -> Vec<VariableDiff> {
    let mut keys: HashSet<&String> = HashSet::new();
    keys.extend(before.keys());
    keys.extend(after.keys());
    let mut out = Vec::new();
    let mut sorted: Vec<&&String> = keys.iter().collect();
    sorted.sort();
    for key in sorted {
        let b = before.get(*key);
        let a = after.get(*key);
        match (b, a) {
            (None, Some(after_value)) => out.push(VariableDiff {
                key: (*key).clone(),
                kind: ChangeKind::Added,
                before: None,
                after: Some(after_value.clone()),
                text_diff: format!("+ {} = {}", key, render_value(after_value)),
            }),
            (Some(before_value), None) => out.push(VariableDiff {
                key: (*key).clone(),
                kind: ChangeKind::Removed,
                before: Some(before_value.clone()),
                after: None,
                text_diff: format!("- {} = {}", key, render_value(before_value)),
            }),
            (Some(before_value), Some(after_value)) => {
                if before_value != after_value {
                    out.push(VariableDiff {
                        key: (*key).clone(),
                        kind: ChangeKind::Modified,
                        before: Some(before_value.clone()),
                        after: Some(after_value.clone()),
                        text_diff: text_value_diff(
                            &render_value(before_value),
                            &render_value(after_value),
                        ),
                    });
                }
            }
            (None, None) => {}
        }
    }
    out
}

pub fn message_diffs(
    context_id: &str,
    before: &[MessageView],
    after: &[MessageView],
) -> Vec<MessageDiff> {
    let before_by_id: HashMap<&str, &MessageView> =
        before.iter().map(|m| (m.id.as_str(), m)).collect();
    let after_by_id: HashMap<&str, &MessageView> =
        after.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut ids: HashSet<&str> = HashSet::new();
    ids.extend(before_by_id.keys().copied());
    ids.extend(after_by_id.keys().copied());
    let mut sorted: Vec<&&str> = ids.iter().collect();
    sorted.sort();
    let mut out = Vec::new();
    for id in sorted {
        match (before_by_id.get(id), after_by_id.get(id)) {
            (None, Some(after_msg)) => out.push(MessageDiff {
                context_id: context_id.to_string(),
                message_id: (*id).to_string(),
                kind: ChangeKind::Added,
                before: None,
                after: Some((*after_msg).clone()),
            }),
            (Some(before_msg), None) => out.push(MessageDiff {
                context_id: context_id.to_string(),
                message_id: (*id).to_string(),
                kind: ChangeKind::Removed,
                before: Some((*before_msg).clone()),
                after: None,
            }),
            (Some(before_msg), Some(after_msg)) => {
                if before_msg.role != after_msg.role || before_msg.content != after_msg.content {
                    out.push(MessageDiff {
                        context_id: context_id.to_string(),
                        message_id: (*id).to_string(),
                        kind: ChangeKind::Modified,
                        before: Some((*before_msg).clone()),
                        after: Some((*after_msg).clone()),
                    });
                }
            }
            (None, None) => {}
        }
    }
    out
}

pub fn all_message_diffs(
    before: &HashMap<String, Vec<MessageView>>,
    after: &HashMap<String, Vec<MessageView>>,
) -> Vec<MessageDiff> {
    let mut ids: HashSet<&String> = HashSet::new();
    ids.extend(before.keys());
    ids.extend(after.keys());
    let mut sorted: Vec<&&String> = ids.iter().collect();
    sorted.sort();
    let mut out = Vec::new();
    for ctx in sorted {
        let b: &[MessageView] = before.get(*ctx).map(Vec::as_slice).unwrap_or_default();
        let a: &[MessageView] = after.get(*ctx).map(Vec::as_slice).unwrap_or_default();
        out.extend(message_diffs(ctx, b, a));
    }
    out
}

fn render_value(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::from("null"))
}

fn text_value_diff(before: &str, after: &str) -> String {
    let diff = similar::TextDiff::from_lines(before, after);
    let mut buf = String::new();
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Equal => " ",
            };
            buf.push_str(sign);
            buf.push_str(change.value());
            if !change.value().ends_with('\n') {
                buf.push('\n');
            }
        }
    }
    if buf.is_empty() {
        format!("- {before}\n+ {after}\n")
    } else {
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_added_removed_modified() {
        let mut before = HashMap::new();
        before.insert("keep".to_string(), json!(1));
        before.insert("old".to_string(), json!(1));
        before.insert("chg".to_string(), json!(1));
        let mut after = HashMap::new();
        after.insert("keep".to_string(), json!(1));
        after.insert("new".to_string(), json!(2));
        after.insert("chg".to_string(), json!(2));
        let diffs = variable_diffs(&before, &after);
        assert_eq!(diffs.len(), 3);
    }

    #[test]
    fn aligns_messages_by_id() {
        let before = vec![MessageView {
            id: "m1".to_string(),
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let after = vec![
            MessageView {
                id: "m1".to_string(),
                role: "user".to_string(),
                content: "hi!".to_string(),
            },
            MessageView {
                id: "m2".to_string(),
                role: "assistant".to_string(),
                content: "hello".to_string(),
            },
        ];
        let diffs = message_diffs("ctx", &before, &after);
        assert_eq!(diffs.len(), 2);
    }
}
