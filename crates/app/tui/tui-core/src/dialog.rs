//! Host-mediated dialogs for plugins: confirm, select, text input and multiline edit.
//!
//! A plugin describes the question as data; the shell presents it with its
//! own widgets (`wf-tui`), the stdin line reader (`wf-mini`) or the
//! structured stdio protocol (`wf-headless`) and returns the answer.
//! Sandboxed backends never touch the terminal directly, and every prompt
//! fails closed on timeout or EOF so a missing answer never hangs a turn.

use serde::{Deserialize, Serialize};

/// One dialog a plugin asks the host to present. The host owns focus,
/// rendering and dismissal; the plugin only sees the outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogRequest {
    /// Yes/no question. `danger` renders the destructive accent.
    Confirm {
        title: String,
        message: String,
        #[serde(default)]
        danger: bool,
    },
    /// Pick one of `options` by 1-based index; empty options are rejected
    /// by the host before presenting.
    Select {
        title: String,
        message: String,
        options: Vec<String>,
    },
    /// Single-line answer; `secret` disables echoing where the shell can.
    Input {
        title: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(default)]
        secret: bool,
    },
    /// Multiline edit; `initial` prefills the buffer, `language` hints
    /// syntax highlighting where the shell can. Headless and mini shells
    /// degrade to a single-line answer without hanging the turn.
    Edit {
        title: String,
        message: String,
        #[serde(default)]
        initial: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
}

/// Default wait for one dialog answer before failing closed.
pub const DIALOG_DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Wire name of the permission gating dialog requests.
pub fn dialog_permission_id() -> &'static str {
    "cli_dialog"
}

/// Denial message naming the plugin without a dialog permission.
pub fn deny_dialog_without_permission(plugin_id: &str) -> String {
    format!(
        "plugin '{plugin_id}' lacks the {} permission; dialog request denied",
        dialog_permission_id()
    )
}

/// Title of one request for host surfaces and audit logs.
pub fn dialog_title(request: &DialogRequest) -> &str {
    match request {
        DialogRequest::Confirm { title, .. }
        | DialogRequest::Select { title, .. }
        | DialogRequest::Input { title, .. }
        | DialogRequest::Edit { title, .. } => title,
    }
}

/// One-line stderr prompt text for the mini shell: title, message and the
/// expected answer shape. The host owns focus; this text never executes.
pub fn mini_prompt_text(request: &DialogRequest, timeout_secs: u64) -> String {
    match request {
        DialogRequest::Confirm { title, message, .. } => {
            format!("{title}: {message} [y/N] (default N, {timeout_secs}s timeout)")
        }
        DialogRequest::Select { title, message, options } => {
            let mut line = format!("{title}: {message}");
            for (index, option) in options.iter().enumerate() {
                line.push_str(&format!(" [{}] {option}", index + 1));
            }
            line.push_str(" (reply number)");
            line
        }
        DialogRequest::Input { title, message, placeholder, .. } => {
            let hint = placeholder.as_deref().unwrap_or("");
            if hint.is_empty() {
                format!("{title}: {message}")
            } else {
                format!("{title}: {message} ({hint})")
            }
        }
        DialogRequest::Edit { title, message, .. } => {
            format!("{title}: {message} (single line; empty cancels)")
        }
    }
}

/// What the host reports back for one [`DialogRequest`].
#[derive(Debug, Clone, PartialEq)]
pub enum DialogOutcome {
    /// Confirm answer: `true` only on an explicit yes.
    Confirmed(bool),
    /// Selected 0-based option index; `None` means dismissed.
    Selected(Option<usize>),
    /// Entered text; `None` means dismissed without answering.
    Entered(Option<String>),
    /// Edited text; `None` means dismissed without answering.
    Edited(Option<String>),
    /// The request was cancelled before an answer arrived.
    Cancelled,
}

/// Parse one headless answer line for a confirm dialog: only an explicit
/// `y`/`yes` (any case, surrounding whitespace allowed) approves;
/// everything else, including an empty line, denies.
pub fn parse_confirm_answer(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Parse one headless answer line for a select dialog: a 1-based index
/// inside `options`, else `None` for dismiss.
pub fn parse_select_answer(line: &str, options: usize) -> Option<usize> {
    line.trim()
        .parse::<usize>()
        .ok()
        .filter(|index| *index >= 1 && *index <= options)
        .map(|index| index - 1)
}

/// Apply one headless answer line to a pending request. `None` (timeout or
/// EOF) fails closed: denied confirm, dismissed select and input.
pub fn answer_headless(request: &DialogRequest, line: Option<&str>) -> DialogOutcome {
    match request {
        DialogRequest::Confirm { .. } => {
            DialogOutcome::Confirmed(line.is_some_and(parse_confirm_answer))
        }
        DialogRequest::Select { options, .. } => {
            DialogOutcome::Selected(line.and_then(|text| parse_select_answer(text, options.len())))
        }
        DialogRequest::Input { .. } => match line {
            Some(text) => DialogOutcome::Entered(Some(text.to_string())),
            None => DialogOutcome::Cancelled,
        },
        DialogRequest::Edit { .. } => match line {
            Some(text) if text.trim().is_empty() => DialogOutcome::Edited(None),
            Some(text) => DialogOutcome::Edited(Some(text.to_string())),
            None => DialogOutcome::Cancelled,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_needs_an_explicit_yes() {
        assert!(parse_confirm_answer("y"));
        assert!(parse_confirm_answer(" YES "));
        assert!(!parse_confirm_answer(""));
        assert!(!parse_confirm_answer("no"));
        assert!(!parse_confirm_answer("yeah"));
    }

    #[test]
    fn select_accepts_only_in_range_indices() {
        assert_eq!(parse_select_answer("1", 3), Some(0));
        assert_eq!(parse_select_answer("3", 3), Some(2));
        assert_eq!(parse_select_answer("0", 3), None);
        assert_eq!(parse_select_answer("4", 3), None);
        assert_eq!(parse_select_answer("x", 3), None);
    }

    #[test]
    fn headless_answers_fail_closed_without_a_line() {
        let confirm = DialogRequest::Confirm {
            title: "rm?".to_string(),
            message: "delete?".to_string(),
            danger: true,
        };
        assert_eq!(
            answer_headless(&confirm, None),
            DialogOutcome::Confirmed(false)
        );
        let select = DialogRequest::Select {
            title: "pick".to_string(),
            message: "pick one".to_string(),
            options: vec!["a".to_string()],
        };
        assert_eq!(
            answer_headless(&select, None),
            DialogOutcome::Selected(None)
        );
        let input = DialogRequest::Input {
            title: "name".to_string(),
            message: "your name".to_string(),
            placeholder: None,
            secret: false,
        };
        assert_eq!(answer_headless(&input, None), DialogOutcome::Cancelled);
        assert_eq!(
            answer_headless(&input, Some("ada")),
            DialogOutcome::Entered(Some("ada".to_string()))
        );
    }

    #[test]
    fn dialog_requests_round_trip_through_json() {
        let request = DialogRequest::Select {
            title: "pick".to_string(),
            message: "pick one".to_string(),
            options: vec!["a".to_string(), "b".to_string()],
        };
        let value = serde_json::to_value(&request).expect("serializes");
        let back: DialogRequest = serde_json::from_value(value).expect("deserializes");
        assert_eq!(request, back);
        let edit = DialogRequest::Edit {
            title: "notes".to_string(),
            message: "edit notes".to_string(),
            initial: "hi".to_string(),
            language: None,
        };
        let value = serde_json::to_value(&edit).expect("serializes");
        let back: DialogRequest = serde_json::from_value(value).expect("deserializes");
        assert_eq!(edit, back);
    }

    #[test]
    fn edit_answers_fail_closed_and_blank_cancels() {
        let edit = DialogRequest::Edit {
            title: "notes".to_string(),
            message: "edit".to_string(),
            initial: String::new(),
            language: None,
        };
        assert_eq!(answer_headless(&edit, None), DialogOutcome::Cancelled);
        assert_eq!(
            answer_headless(&edit, Some("")),
            DialogOutcome::Edited(None)
        );
        assert_eq!(
            answer_headless(&edit, Some("hello")),
            DialogOutcome::Edited(Some("hello".to_string()))
        );
    }

    #[test]
    fn mini_prompts_name_the_permission_on_denial() {
        assert!(deny_dialog_without_permission("review.bot").contains("review.bot"));
        assert!(deny_dialog_without_permission("review.bot").contains(dialog_permission_id()));
        let confirm = DialogRequest::Confirm {
            title: "rm?".to_string(),
            message: "delete?".to_string(),
            danger: false,
        };
        assert!(mini_prompt_text(&confirm, 5).contains("[y/N]"));
        assert_eq!(dialog_title(&confirm), "rm?");
    }
}
