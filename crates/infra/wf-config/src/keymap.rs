//! User keybinding configuration loading and merging.
//!
//! Keymap overrides live in a dedicated settings file and are expressed as
//! plain strings (`context` -> `action` -> `chord`, e.g. `"ctrl+shift+s"`).
//! This module intentionally knows nothing about `tui-core`'s `Keymap`
//! types: upper layers convert the parsed [`Chord`] values into their own
//! key representations. Precedence follows the MCP settings pattern:
//! project file overrides the global file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, ConfigResult};
use crate::layout;
use crate::loader;

pub const DEFAULT_KEYMAP_SETTINGS_FILE: &str = "keymap-settings.toml";

/// The base key names accepted inside a chord description. Everything else
/// (single letters, digits) is accepted verbatim as a character key.
pub const NAMED_KEYS: &[&str] = &[
    "enter", "esc", "tab", "up", "down", "left", "right", "home", "end", "pageup", "pagedown",
    "backspace", "delete", "space", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10",
    "f11", "f12",
];

/// A parsed chord description: a base key plus optional modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    /// Lowercase base key: a named key from [`NAMED_KEYS`] or a single
    /// ASCII character (letters are lowercased, digits pass through).
    pub code: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Chord {
    /// Canonical display form, e.g. `ctrl+shift+delete`.
    pub fn to_description(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(&self.code);
        parts.join("+")
    }
}

/// Parse a chord description such as `"ctrl+shift+s"`, `"esc"` or `"?"`.
/// Modifiers may appear in any order and are case-insensitive; the base
/// key must be a named key or a single character.
pub fn parse_chord(description: &str) -> ConfigResult<Chord> {
    let mut chord = Chord {
        code: String::new(),
        ctrl: false,
        alt: false,
        shift: false,
    };
    for token in description.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        let lowered = token.to_ascii_lowercase();
        match lowered.as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" | "option" => chord.alt = true,
            "shift" => chord.shift = true,
            _ => {
                if !chord.code.is_empty() {
                    return Err(ConfigError::Validation(format!(
                        "invalid key chord '{description}': multiple base keys"
                    )));
                }
                if NAMED_KEYS.contains(&lowered.as_str()) {
                    chord.code = lowered;
                } else {
                    let mut chars = token.chars();
                    let first = chars.next().ok_or_else(|| {
                        ConfigError::Validation(format!("invalid key chord '{description}': empty key"))
                    })?;
                    if chars.next().is_some() {
                        return Err(ConfigError::Validation(format!(
                            "invalid key chord '{description}': unknown key '{token}'"
                        )));
                    }
                    chord.code = first.to_ascii_lowercase().to_string();
                }
            }
        }
    }
    if chord.code.is_empty() {
        return Err(ConfigError::Validation(format!(
            "invalid key chord '{description}': missing base key"
        )));
    }
    Ok(chord)
}

/// Raw keymap settings as stored on disk: `context -> action -> chord`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeymapSettings {
    #[serde(default, rename = "keymap")]
    pub bindings: HashMap<String, HashMap<String, String>>,
}

impl KeymapSettings {
    /// Merge `other` on top of `self`; bindings from `other` win.
    pub fn merge(&mut self, other: KeymapSettings) {
        for (context, actions) in other.bindings {
            self.bindings.entry(context).or_default().extend(actions);
        }
    }

    /// Look up the chord bound to `action` inside `context`.
    pub fn resolve(&self, context: &str, action: &str) -> Option<Chord> {
        self.bindings
            .get(context)
            .and_then(|actions| actions.get(action))
            .and_then(|description| parse_chord(description).ok())
    }

    /// Parse every binding and report validation problems.
    ///
    /// Returns one error message per invalid entry (unknown key name,
    /// malformed chord) and one per conflict (the same chord bound to two
    /// actions inside the same context).
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (context, actions) in &self.bindings {
            let mut by_chord: HashMap<String, Vec<&String>> = HashMap::new();
            for (action, description) in actions {
                match parse_chord(description) {
                    Ok(chord) => {
                        by_chord
                            .entry(chord.to_description())
                            .or_default()
                            .push(action);
                    }
                    Err(_) => problems.push(format!(
                        "keymap.{context}.{action}: invalid chord '{description}'"
                    )),
                }
            }
            let mut conflicts: Vec<(String, Vec<&String>)> =
                by_chord
                    .into_iter()
                    .filter(|(_, actions)| actions.len() > 1)
                .map(|(chord, mut actions)| {
                    actions.sort();
                    (chord, actions)
                })
                .collect();
            conflicts.sort_by(|a, b| a.0.cmp(&b.0));
            for (chord, actions) in conflicts {
                let names = actions
                    .iter()
                    .map(|action| action.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                problems.push(format!(
                    "keymap.{context}: chord '{chord}' bound to multiple actions: {names}"
                ));
            }
        }
        problems
    }
}

/// Global settings file: `{settings_dir}/keymap-settings.toml`.
pub fn get_global_keymap_settings_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(DEFAULT_KEYMAP_SETTINGS_FILE)
}

/// Project-specific file: `{project_root}/.wf/keymap-settings.toml`
/// (highest precedence).
pub fn get_project_keymap_path(project_root: &Path) -> PathBuf {
    project_root
        .join(layout::PROJECT_WF_DIR)
        .join(DEFAULT_KEYMAP_SETTINGS_FILE)
}

/// Load a single keymap settings file. A missing file is not an error: it
/// simply contributes no bindings.
pub fn load_keymap_settings(file_path: &Path) -> ConfigResult<KeymapSettings> {
    if !file_path.exists() {
        return Ok(KeymapSettings::default());
    }
    loader::load_config_file_sync::<KeymapSettings>(file_path)
}

/// Load a single keymap settings file, logging a warning when the file
/// exists but cannot be parsed.
fn load_keymap_settings_warn(file_path: &Path) -> Option<KeymapSettings> {
    match load_keymap_settings(file_path) {
        Ok(settings) => Some(settings),
        Err(e) => {
            if file_path.exists() {
                tracing::warn!(error = %e, "ignoring unparseable keymap settings file");
            }
            None
        }
    }
}

/// Load and merge keymap settings from the global directory and the project
/// file. Precedence chain (highest first): `.wf/keymap-settings.toml` >
/// global `keymap-settings.toml`. Missing files are skipped; files that
/// exist but fail to parse produce a warning and are skipped.
pub fn load_and_merge_keymap_settings(
    settings_dir: &Path,
    project_root: &Path,
) -> ConfigResult<KeymapSettings> {
    let mut merged = load_keymap_settings_warn(&get_global_keymap_settings_path(settings_dir))
        .unwrap_or_default();
    if let Some(project) = load_keymap_settings_warn(&get_project_keymap_path(project_root)) {
        merged.merge(project);
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_modified_chords() {
        let chord = parse_chord("ctrl+shift+s").unwrap();
        assert_eq!(chord.code, "s");
        assert!(chord.ctrl);
        assert!(chord.shift);
        assert!(!chord.alt);
        assert_eq!(chord.to_description(), "ctrl+shift+s");
    }

    #[test]
    fn modifier_order_and_case_do_not_matter() {
        assert_eq!(parse_chord("Shift+Ctrl+S").unwrap(), parse_chord("ctrl+shift+s").unwrap());
    }

    #[test]
    fn parses_named_and_char_keys() {
        assert_eq!(parse_chord("esc").unwrap().code, "esc");
        assert_eq!(parse_chord("PageUp").unwrap().code, "pageup");
        assert_eq!(parse_chord("?").unwrap().code, "?");
        assert_eq!(parse_chord("A").unwrap().code, "a");
        assert_eq!(parse_chord("5").unwrap().code, "5");
    }

    #[test]
    fn rejects_malformed_chords() {
        assert!(parse_chord("").is_err());
        assert!(parse_chord("ctrl+").is_err());
        assert!(parse_chord("ctrl+foo+bar").is_err());
        assert!(parse_chord("ctrl+s+enter").is_err());
        assert!(parse_chord("s+q").is_err());
    }

    #[test]
    fn empty_settings_resolve_to_nothing_and_validate_clean() {
        let settings = KeymapSettings::default();
        assert!(settings.resolve("global", "quit").is_none());
        assert!(settings.validate().is_empty());
    }

    #[test]
    fn merge_gives_project_precedence() {
        let mut global = KeymapSettings::default();
        global.bindings.insert(
            "global".into(),
            HashMap::from([("quit".to_string(), "ctrl+q".to_string())]),
        );
        let mut project = KeymapSettings::default();
        project.bindings.insert(
            "global".into(),
            HashMap::from([("quit".to_string(), "ctrl+x".to_string())]),
        );
        global.merge(project);
        assert_eq!(
            global.resolve("global", "quit").unwrap().to_description(),
            "ctrl+x"
        );
    }

    #[test]
    fn validate_reports_invalid_chords_and_conflicts() {
        let mut settings = KeymapSettings::default();
        settings.bindings.insert(
            "composer".into(),
            HashMap::from([
                ("submit".to_string(), "ctrl+bad-key".to_string()),
                ("clear".to_string(), "ctrl+k".to_string()),
                ("history".to_string(), "Ctrl+K".to_string()),
            ]),
        );
        let problems = settings.validate();
        assert_eq!(problems.len(), 2);
        assert!(problems.iter().any(|p| p.contains("invalid chord")));
        assert!(problems.iter().any(|p| p.contains("multiple actions")));
    }

    #[test]
    fn loads_missing_file_as_empty_settings() {
        let settings = load_keymap_settings(Path::new("/nonexistent/keymap-settings.toml")).unwrap();
        assert!(settings.bindings.is_empty());
    }

    #[test]
    fn loads_toml_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_wf_keymap_settings.toml");
        std::fs::write(
            &path,
            "[keymap.global]\nquit = \"ctrl+q\"\nhelp = \"?\"\n",
        )
        .unwrap();
        let settings = load_keymap_settings(&path).unwrap();
        assert_eq!(
            settings.resolve("global", "quit").unwrap().to_description(),
            "ctrl+q"
        );
        assert_eq!(settings.resolve("global", "help").unwrap().code, "?");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn merged_loading_prefers_project_file() {
        let dir = std::env::temp_dir().join("test_wf_keymap_merge");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".wf")).unwrap();
        std::fs::write(
            get_global_keymap_settings_path(&dir),
            "[keymap.global]\nhelp = \"?\"\n",
        )
        .unwrap();
        std::fs::write(
            get_project_keymap_path(&dir),
            "[keymap.global]\nhelp = \"f1\"\n",
        )
        .unwrap();
        let merged = load_and_merge_keymap_settings(&dir, &dir).unwrap();
        assert_eq!(merged.resolve("global", "help").unwrap().code, "f1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
