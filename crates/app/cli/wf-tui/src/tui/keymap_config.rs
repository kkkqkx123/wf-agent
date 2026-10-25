//! User keymap overrides loaded from `wf-config` keymap settings.
//!
//! Settings live in the global XDG config dir (`$XDG_CONFIG_HOME/wf/` or
//! `$HOME/.config/wf/`) and are overridden by the project file
//! (`.wf/keymap-settings.toml`). Bindings are strings (`context` →
//! `command id` → `chord`); the command id is the registry id, so a
//! settings file can only rebind commands that actually exist. Unknown
//! contexts or ids are skipped with a warning.

use std::path::PathBuf;

use tui_core::keymap::{CKey, Key, Keymap, KeymapContext};
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

/// Build the runtime keymap from user overrides; builtin bindings resolve
/// from the command registry on demand.
pub fn load_user_keymap() -> Keymap {
    let mut keymap = Keymap::new();
    let Some(settings_dir) = config_settings_dir() else {
        return keymap;
    };
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let Ok(settings) = config_keymap::load_and_merge_keymap_settings(&settings_dir, &project_root)
    else {
        return keymap;
    };
    for (ctx_name, commands) in &settings.bindings {
        let Some(ctx) = parse_context(ctx_name) else {
            tracing::warn!(context = %ctx_name, "unknown keymap context in keymap settings");
            continue;
        };
        for (command_id, description) in commands {
            let Ok(chord) = config_keymap::parse_chord(description) else {
                tracing::warn!(
                    command = %command_id,
                    chord = %description,
                    "ignoring invalid keymap binding"
                );
                continue;
            };
            let key = chord_to_key(&chord);
            if !keymap.bind_id(ctx, key, command_id) {
                tracing::warn!(
                    command = %command_id,
                    "unknown command id in keymap settings"
                );
            }
        }
    }
    keymap
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_core::keymap::CKey;

    #[test]
    fn parses_known_contexts() {
        assert_eq!(parse_context("global"), Some(KeymapContext::Global));
        assert_eq!(parse_context("approval"), Some(KeymapContext::Approval));
        assert_eq!(parse_context("nope"), None);
    }

    #[test]
    fn binds_registry_command_ids_only() {
        let mut keymap = Keymap::new();
        assert!(keymap.bind_id(
            KeymapContext::List,
            Key::plain(CKey::Char('k')),
            "ui.move_prev"
        ));
        assert!(!keymap.bind_id(
            KeymapContext::List,
            Key::plain(CKey::Char('k')),
            "legacy.move_prev"
        ));
        assert_eq!(
            keymap
                .resolve(KeymapContext::List, Key::plain(CKey::Char('k')))
                .map(|command| command.id),
            Some("ui.move_prev")
        );
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
    fn missing_settings_dir_yields_empty_overrides() {
        // No env manipulation: the function must tolerate a missing XDG
        // setup and return an override-free keymap that still resolves
        // builtin chords from the registry.
        let keymap = load_user_keymap();
        assert_eq!(
            keymap
                .resolve(KeymapContext::List, Key::plain(CKey::Char('k')))
                .map(|command| command.id),
            Some("ui.move_prev")
        );
    }
}
