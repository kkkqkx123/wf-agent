//! Shell detection and resolution.
//!
//! Detects the shells available on the current platform and provides the
//! shell-specific executable + command flag for command execution, mirroring
//! the TS `terminal/shell-detector`.
//!
//! Path resolution priority (highest to lowest):
//!   1. custom override;
//!   2. default hardcoded path;
//!   3. `which` / `where` lookup in `PATH`;
//!   4. executable name only (OS `PATH` fallback).

use std::collections::HashMap;
use std::sync::Mutex;

/// Supported shell types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    Sh,
    Cmd,
    Powershell,
    Pwsh,
    GitBash,
    Wsl,
}

impl ShellType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShellType::Bash => "bash",
            ShellType::Zsh => "zsh",
            ShellType::Fish => "fish",
            ShellType::Sh => "sh",
            ShellType::Cmd => "cmd",
            ShellType::Powershell => "powershell",
            ShellType::Pwsh => "pwsh",
            ShellType::GitBash => "git-bash",
            ShellType::Wsl => "wsl",
        }
    }

    /// Parse a shell type from its string name (e.g. `"bash"`).
    pub fn from_name(name: &str) -> Option<ShellType> {
        match name {
            "bash" => Some(ShellType::Bash),
            "zsh" => Some(ShellType::Zsh),
            "fish" => Some(ShellType::Fish),
            "sh" => Some(ShellType::Sh),
            "cmd" => Some(ShellType::Cmd),
            "powershell" => Some(ShellType::Powershell),
            "pwsh" => Some(ShellType::Pwsh),
            "git-bash" | "git_bash" => Some(ShellType::GitBash),
            "wsl" => Some(ShellType::Wsl),
            _ => None,
        }
    }
}

/// Static per-shell metadata.
#[derive(Debug, Clone, Copy)]
struct ShellConfig {
    shell_type: ShellType,
    default_path: &'static str,
    command_flag: &'static str,
}

const SHELL_CONFIGS: &[ShellConfig] = &[
    ShellConfig {
        shell_type: ShellType::Bash,
        default_path: "/bin/bash",
        command_flag: "-c",
    },
    ShellConfig {
        shell_type: ShellType::Zsh,
        default_path: "/bin/zsh",
        command_flag: "-c",
    },
    ShellConfig {
        shell_type: ShellType::Fish,
        default_path: "/bin/fish",
        command_flag: "-c",
    },
    ShellConfig {
        shell_type: ShellType::Sh,
        default_path: "/bin/sh",
        command_flag: "-c",
    },
    ShellConfig {
        shell_type: ShellType::Cmd,
        default_path: "cmd.exe",
        command_flag: "/c",
    },
    ShellConfig {
        shell_type: ShellType::Powershell,
        default_path: "powershell.exe",
        command_flag: "-Command",
    },
    ShellConfig {
        shell_type: ShellType::Pwsh,
        default_path: "pwsh.exe",
        command_flag: "-Command",
    },
    ShellConfig {
        shell_type: ShellType::GitBash,
        default_path: "C:\\Program Files\\Git\\bin\\bash.exe",
        command_flag: "-c",
    },
    ShellConfig {
        shell_type: ShellType::Wsl,
        default_path: "wsl.exe",
        command_flag: "--",
    },
];

fn config_for(shell_type: ShellType) -> &'static ShellConfig {
    SHELL_CONFIGS
        .iter()
        .find(|c| c.shell_type == shell_type)
        .expect("all shell types have a config")
}

/// Shells that are Windows-only (unavailable on Unix without WSL).
const WINDOWS_ONLY: &[ShellType] = &[
    ShellType::Cmd,
    ShellType::Powershell,
    ShellType::Pwsh,
    ShellType::GitBash,
    ShellType::Wsl,
];

/// Resolved shell information.
#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub shell_type: ShellType,
    pub path: String,
    pub available: bool,
    pub command_flag: &'static str,
}

/// Detects and resolves shells on the current system.
pub struct ShellDetector {
    overrides: HashMap<ShellType, String>,
    cache: Mutex<HashMap<ShellType, Option<String>>>,
    is_windows: bool,
}

impl Default for ShellDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellDetector {
    pub fn new() -> Self {
        Self {
            overrides: HashMap::new(),
            cache: Mutex::new(HashMap::new()),
            is_windows: std::env::consts::OS == "windows",
        }
    }

    /// Register a custom executable path for a shell type (highest priority).
    pub fn add_path_override(&mut self, shell_type: ShellType, path: impl Into<String>) {
        self.overrides.insert(shell_type, path.into());
        self.cache.lock().unwrap().remove(&shell_type);
    }

    /// Default shell for the platform: `$SHELL` on Unix, powershell on
    /// Windows, `bash` as the fallback.
    pub fn get_default_shell(&self) -> ShellType {
        if self.is_windows {
            return ShellType::Powershell;
        }
        if let Ok(shell_env) = std::env::var("SHELL") {
            let lower = shell_env.to_lowercase();
            if lower.contains("zsh") {
                return ShellType::Zsh;
            }
            if lower.contains("fish") {
                return ShellType::Fish;
            }
            if lower.contains("bash") {
                return ShellType::Bash;
            }
        }
        ShellType::Bash
    }

    /// Resolve the executable path for a shell type.
    ///
    /// Priority: override > platform heuristics > default path > `which` /
    /// `where` lookup > executable name fallback.
    pub fn resolve_shell_path(&self, shell_type: ShellType) -> Option<String> {
        if let Some(cached) = self.cache.lock().unwrap().get(&shell_type) {
            return cached.clone();
        }

        let resolved = self.resolve_shell_path_uncached(shell_type);
        self.cache
            .lock()
            .unwrap()
            .insert(shell_type, resolved.clone());
        resolved
    }

    fn resolve_shell_path_uncached(&self, shell_type: ShellType) -> Option<String> {
        // 1. Custom override.
        if let Some(path) = self.overrides.get(&shell_type) {
            return Some(path.clone());
        }

        // 2. Platform compatibility gate.
        if !self.is_platform_compatible(shell_type) {
            return None;
        }

        let config = config_for(shell_type);

        // 3. Windows built-in shells are always "available" by name.
        let builtin =
            matches!(shell_type, ShellType::Cmd | ShellType::Powershell) && self.is_windows;
        if builtin {
            return Some(config.default_path.to_string());
        }

        // 4. Default hardcoded path (Unix absolute paths / heuristics).
        if !self.is_windows
            && config.default_path.starts_with('/')
            && std::path::Path::new(config.default_path).exists()
        {
            return Some(config.default_path.to_string());
        }

        // 5. which / where lookup.
        if let Some(found) = self.lookup_in_path(shell_type) {
            return Some(found);
        }

        // 6. Executable name only (OS PATH fallback).
        let exec_name = config
            .default_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(config.default_path);
        Some(exec_name.to_string())
    }

    /// Check whether a shell type is usable on the current platform.
    pub fn is_platform_compatible(&self, shell_type: ShellType) -> bool {
        if self.is_windows {
            return true;
        }
        if WINDOWS_ONLY.contains(&shell_type) {
            // Only WSL is meaningful on non-Windows.
            return shell_type == ShellType::Wsl;
        }
        true
    }

    /// Whether a shell is available (usable on this platform and resolvable).
    pub fn is_shell_available(&self, shell_type: ShellType) -> bool {
        if !self.is_platform_compatible(shell_type) {
            return false;
        }
        self.resolve_shell_path(shell_type).is_some()
    }

    /// Resolve the shell path (same as [`resolve_shell_path`]).
    pub fn get_shell_path(&self, shell_type: ShellType) -> Option<String> {
        self.resolve_shell_path(shell_type)
    }

    /// Command flag for the shell (e.g. `-c` for bash, `/c` for cmd).
    pub fn get_command_flag(&self, shell_type: ShellType) -> &'static str {
        config_for(shell_type).command_flag
    }

    /// Build the argument vector `[flag, command]`.
    pub fn get_shell_args(&self, shell_type: ShellType, command: &str) -> Vec<String> {
        vec![
            self.get_command_flag(shell_type).to_string(),
            command.to_string(),
        ]
    }

    /// Resolve to a shell type with fallback to the platform default when the
    /// requested one is unavailable.
    pub fn resolve_shell_type(&self, shell_type: Option<ShellType>) -> ShellType {
        match shell_type {
            Some(requested) if self.is_shell_available(requested) => requested,
            _ => {
                let default = self.get_default_shell();
                if self.is_shell_available(default) {
                    default
                } else {
                    ShellType::Sh
                }
            }
        }
    }

    /// All shells available on this system.
    pub fn get_available_shells(&self) -> Vec<ShellType> {
        SHELL_CONFIGS
            .iter()
            .map(|c| c.shell_type)
            .filter(|t| self.is_shell_available(*t))
            .collect()
    }

    /// Full shell info for a type.
    pub fn get_shell_info(&self, shell_type: ShellType) -> ShellInfo {
        ShellInfo {
            shell_type,
            path: self
                .resolve_shell_path(shell_type)
                .unwrap_or_else(|| config_for(shell_type).default_path.to_string()),
            available: self.is_shell_available(shell_type),
            command_flag: self.get_command_flag(shell_type),
        }
    }

    /// Clear the cached detection results.
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }

    fn lookup_in_path(&self, shell_type: ShellType) -> Option<String> {
        let config = config_for(shell_type);
        let exec_name = config
            .default_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(config.default_path);

        let lookup_cmd = if self.is_windows {
            format!("where {}", exec_name)
        } else {
            format!("which {}", exec_name)
        };
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&lookup_cmd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
    }
}

/// Lazy global default detector.
pub fn default_shell_detector() -> &'static ShellDetector {
    use once_cell::sync::Lazy;
    static DETECTOR: Lazy<ShellDetector> = Lazy::new(ShellDetector::new);
    &DETECTOR
}

/// Resolve the command string for execution: returns `(program, args)` for
/// the given shell, or a shell command line when resolution fails (falls back
/// to `/bin/sh -c` on Unix / `cmd.exe /c` on Windows).
pub fn resolve_shell_command(
    detector: &ShellDetector,
    shell_type: Option<ShellType>,
    command: &str,
) -> (String, Vec<String>) {
    let resolved = detector.resolve_shell_type(shell_type);
    if let Some(path) = detector.resolve_shell_path(resolved) {
        let flag = detector.get_command_flag(resolved);
        (path, vec![flag.to_string(), command.to_string()])
    } else if detector.is_windows {
        ("cmd.exe".into(), vec!["/c".into(), command.to_string()])
    } else {
        ("/bin/sh".into(), vec!["-c".into(), command.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shell_reads_shell_env() {
        let detector = ShellDetector::new();
        let shell = detector.get_default_shell();
        // On Unix, matches $SHELL; otherwise the platform default.
        assert!(matches!(
            shell,
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Powershell
        ));
    }

    #[test]
    fn test_bash_available_on_unix() {
        if std::env::consts::OS == "windows" {
            return;
        }
        let detector = ShellDetector::new();
        assert!(detector.is_shell_available(ShellType::Sh));
        assert!(
            detector.is_shell_available(ShellType::Bash)
                || std::env::var("SHELL").is_err() && std::path::Path::new("/bin/bash").exists()
        );
        assert!(
            !detector.is_shell_available(ShellType::Cmd),
            "cmd should be unavailable on Unix"
        );
        assert!(!detector.is_shell_available(ShellType::Powershell));
    }

    #[test]
    fn test_shell_args_shape() {
        let detector = ShellDetector::new();
        let args = detector.get_shell_args(ShellType::Bash, "ls -la");
        assert_eq!(args, vec!["-c", "ls -la"]);
        assert_eq!(detector.get_command_flag(ShellType::Cmd), "/c");
        assert_eq!(detector.get_command_flag(ShellType::Powershell), "-Command");
    }

    #[test]
    fn test_override_wins() {
        let mut detector = ShellDetector::new();
        detector.add_path_override(ShellType::Bash, "/custom/bash");
        assert_eq!(
            detector.resolve_shell_path(ShellType::Bash).as_deref(),
            Some("/custom/bash")
        );
    }

    #[test]
    fn test_resolve_shell_command_fallback() {
        let detector = ShellDetector::new();
        let (program, args) = resolve_shell_command(&detector, None, "echo hi");
        assert!(!program.is_empty());
        assert_eq!(args.len(), 2);
        assert_eq!(args[1], "echo hi");
    }

    #[test]
    fn test_shell_type_roundtrip() {
        for shell in [
            ShellType::Bash,
            ShellType::Zsh,
            ShellType::Fish,
            ShellType::Sh,
            ShellType::Cmd,
            ShellType::Powershell,
            ShellType::Pwsh,
            ShellType::GitBash,
            ShellType::Wsl,
        ] {
            assert_eq!(ShellType::from_name(shell.as_str()), Some(shell));
        }
        assert_eq!(ShellType::from_name("unknown"), None);
    }

    #[test]
    fn test_resolve_unknown_shell_falls_back() {
        let detector = ShellDetector::new();
        let resolved = detector.resolve_shell_type(None);
        assert!(matches!(
            resolved,
            ShellType::Bash
                | ShellType::Zsh
                | ShellType::Fish
                | ShellType::Sh
                | ShellType::Powershell
        ));
    }
}
