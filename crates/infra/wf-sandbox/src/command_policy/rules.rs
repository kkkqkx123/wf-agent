use super::*;

/// A named, severity-tagged dangerous rule — the building block of the Pack
/// system. `pattern` is matched against the masked command string.
#[derive(Debug, Clone, Copy)]
pub struct CommandRule {
    /// Addressable rule id, e.g. `core.bash:rm-rf-root`.
    pub id: &'static str,
    /// Pack identifier for group enable/disable.
    pub pack: &'static str,
    pub pattern: &'static str,
    pub severity: Severity,
}

/// Detailed verdict: decision plus the evidence that produced it.
#[derive(Debug, Clone)]
pub struct CommandVerdict {
    pub decision: CommandDecision,
    pub severity: Option<Severity>,
    pub reason: Option<String>,
    /// 0.0–1.0; exact rule hits are 1.0, heuristic/normalized outcomes lower.
    pub confidence: f64,
}

impl CommandVerdict {
    pub(crate) fn deny(severity: Severity, reason: impl Into<String>) -> Self {
        Self {
            decision: CommandDecision::AutoDeny,
            severity: Some(severity),
            reason: Some(reason.into()),
            confidence: 1.0,
        }
    }

    pub(crate) fn approve() -> Self {
        Self {
            decision: CommandDecision::AutoApprove,
            severity: None,
            reason: None,
            confidence: 1.0,
        }
    }

    pub(crate) fn ask() -> Self {
        Self {
            decision: CommandDecision::AskUser,
            severity: None,
            reason: None,
            confidence: 0.5,
        }
    }
}

/// Unified allow/deny rules for command-identity decisions.
#[derive(Debug, Clone, Default)]
pub struct CommandRules {
    pub allowed_commands: Vec<String>,
    pub denied_commands: Vec<String>,
}

/// Dialect default blacklists (fallback when a sandbox `ShellPolicy` does not
/// specify `denied_commands`). Kept in the analyzers, exported here.
pub fn default_denied_commands(shell_type: ShellType) -> &'static [&'static str] {
    match shell_type {
        ShellType::Bash => crate::strategy::shell::bash::DENIED_COMMANDS,
        ShellType::Cmd => crate::strategy::shell::cmd::DENIED_COMMANDS,
        ShellType::PowerShell => crate::strategy::shell::powershell::DENIED_COMMANDS,
    }
}

/// Look up a built-in dangerous rule by its addressable id.
pub fn find_rule(shell_type: ShellType, id: &str) -> Option<&'static CommandRule> {
    match shell_type {
        ShellType::Bash => crate::strategy::shell::bash::DANGEROUS_PATTERNS
            .iter()
            .find(|r| r.id == id),
        ShellType::Cmd => crate::strategy::shell::cmd::DANGEROUS_PATTERNS
            .iter()
            .find(|r| r.id == id),
        ShellType::PowerShell => crate::strategy::shell::powershell::DANGEROUS_PATTERNS
            .iter()
            .find(|r| r.id == id),
    }
}

/// All built-in dangerous rules belonging to a pack (e.g. `core.filesystem`).
pub fn rules_by_pack(shell_type: ShellType, pack: &str) -> Vec<&'static CommandRule> {
    match shell_type {
        ShellType::Bash => crate::strategy::shell::bash::DANGEROUS_PATTERNS
            .iter()
            .filter(|r| r.pack == pack)
            .collect(),
        ShellType::Cmd => crate::strategy::shell::cmd::DANGEROUS_PATTERNS
            .iter()
            .filter(|r| r.pack == pack)
            .collect(),
        ShellType::PowerShell => crate::strategy::shell::powershell::DANGEROUS_PATTERNS
            .iter()
            .filter(|r| r.pack == pack)
            .collect(),
    }
}
