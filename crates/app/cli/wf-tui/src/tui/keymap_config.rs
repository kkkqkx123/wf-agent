//! User keymap overrides loaded from `wf-config` keymap settings.
//!
//! Settings live in the global XDG config dir (`$XDG_CONFIG_HOME/wf/` or
//! `$HOME/.config/wf/`) and are overridden by the project file
//! (`.wf/keymap-settings.toml`). Bindings are strings (`context` →
//! `action` → `chord`); unknown contexts or actions are skipped with a
//! warning so a stale settings file never breaks startup.

use std::path::PathBuf;

use tui_core::keymap::{builtin_keymap, CKey, Key, KeyAction, Keymap, KeymapContext};
use wf_config::keymap as config_keymap;

/// Resolve the global settings directory following the XDG conventions the
/// CLI app config uses (`$XDG_CONFIG_HOME/wf` or `$HOME/.config/wf`).
fn config_settings_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf"))
}

fn parse_context(name: &str) -> Option<KeymapContext> {
    Some(match name {
        "global" => KeymapContext::Global,
        "list" => KeymapContext::List,
        "detail" => KeymapContext::Detail,
        "chat" => KeymapContext::Chat,
        "input" => KeymapContext::Input,
        "modal" => KeymapContext::Modal,
        "composer" => KeymapContext::Composer,
        "panel" => KeymapContext::Panel,
        "approval" => KeymapContext::Approval,
        "question" => KeymapContext::Question,
        _ => return None,
    })
}

fn parse_action(name: &str) -> Option<KeyAction> {
    Some(match name {
        "quit" => KeyAction::Quit,
        "redraw" => KeyAction::Redraw,
        "help" => KeyAction::Help,
        "palette" => KeyAction::Palette,
        "interrupt" => KeyAction::Interrupt,
        "cancel" => KeyAction::Cancel,
        "back" => KeyAction::Back,
        "select" => KeyAction::Select,
        "submit" => KeyAction::Submit,
        "refresh" => KeyAction::Refresh,
        "delete" => KeyAction::Delete,
        "new" => KeyAction::New,
        "edit" => KeyAction::Edit,
        "move_prev" => KeyAction::MovePrev,
        "move_next" => KeyAction::MoveNext,
        "history_prev" => KeyAction::HistoryPrev,
        "history_next" => KeyAction::HistoryNext,
        "clear" => KeyAction::Clear,
        "home" => KeyAction::Home,
        "end" => KeyAction::End,
        "scan" => KeyAction::Scan,
        "reload" => KeyAction::Reload,
        "cache_clear" => KeyAction::CacheClear,
        "approve" => KeyAction::Approve,
        "approve_all" => KeyAction::ApproveAll,
        "deny_once" => KeyAction::DenyOnce,
        "deny" => KeyAction::Deny,
        _ => return None,
    })
}

fn chord_to_key(chord: &config_keymap::Chord) -> Key {
    let code = match chord.code.as_str() {
        "enter" => CKey::Enter,
        "esc" => CKey::Esc,
        "tab" => CKey::Tab,
        "up" => CKey::Up,
        "down" => CKey::Down,
        "left" => CKey::Left,
        "right" => CKey::Right,
        "home" => CKey::Home,
        "end" => CKey::End,
        "pageup" => CKey::PageUp,
        "pagedown" => CKey::PageDown,
        "backspace" => CKey::Backspace,
        "delete" => CKey::Delete,
        "space" => CKey::Char(' '),
        other => {
            // parse_chord only lets a single character through here.
            CKey::Char(other.chars().next().unwrap_or('?'))
        }
    };
    Key {
        code,
        ctrl: chord.ctrl,
        alt: chord.alt,
        shift: chord.shift,
    }
}

/// Build the runtime keymap: builtin defaults plus user overrides from
/// keymap settings. Returns the keymap and the applied overrides as
/// `(binding, key description)` rows for the help display.
pub fn load_user_keymap() -> (Keymap, Vec<(String, String)>) {
    let mut keymap = builtin_keymap();
    let mut overrides = Vec::new();
    let Some(settings_dir) = config_settings_dir() else {
        return (keymap, overrides);
    };
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let Ok(settings) = config_keymap::load_and_merge_keymap_settings(&settings_dir, &project_root)
    else {
        return (keymap, overrides);
    };
    for (ctx_name, actions) in &settings.bindings {
        let Some(ctx) = parse_context(ctx_name) else {
            tracing::warn!(context = %ctx_name, "unknown keymap context in keymap settings");
            continue;
        };
        for (action_name, description) in actions {
            let (Ok(chord), Some(action)) = (
                config_keymap::parse_chord(description),
                parse_action(action_name),
            ) else {
                tracing::warn!(
                    action = %action_name,
                    chord = %description,
                    "ignoring invalid keymap binding"
                );
                continue;
            };
            let key = chord_to_key(&chord);
            if ctx == KeymapContext::Global {
                keymap.set_global(key, action);
            } else {
                keymap.bind(ctx, key, action);
            }
            overrides.push((format!("{ctx_name}.{action_name}"), key.describe()));
        }
    }
    (keymap, overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_core::keymap::CKey;

    #[test]
    fn parses_known_contexts_and_actions() {
        assert_eq!(parse_context("global"), Some(KeymapContext::Global));
        assert_eq!(parse_context("approval"), Some(KeymapContext::Approval));
        assert_eq!(parse_context("nope"), None);
        assert_eq!(parse_action("quit"), Some(KeyAction::Quit));
        assert_eq!(parse_action("deny_once"), Some(KeyAction::DenyOnce));
        assert_eq!(parse_action("nope"), None);
    }

    #[test]
    fn chord_to_key_maps_named_and_char_keys() {
        let chord = config_keymap::parse_chord("ctrl+k").unwrap();
        let key = chord_to_key(&chord);
        assert_eq!(key, Key::ctrl(CKey::Char('k')));
        let chord = config_keymap::parse_chord("esc").unwrap();
        assert_eq!(chord_to_key(&chord), Key::plain(CKey::Esc));
    }

    #[test]
    fn missing_settings_dir_yields_builtin_keymap() {
        // No env manipulation: the function must tolerate a missing XDG
        // setup and return the builtin keymap unchanged.
        let (keymap, overrides) = load_user_keymap();
        assert!(overrides.is_empty());
        assert_eq!(
            keymap.resolve(KeymapContext::List, Key::plain(CKey::Char('k'))),
            Some(KeyAction::MovePrev)
        );
    }
}
