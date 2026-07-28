use std::fmt;
use wf_types::script::sandbox::ShellPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Cmd,
    PowerShell,
}

impl fmt::Display for ShellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellType::Bash => write!(f, "bash"),
            ShellType::Cmd => write!(f, "cmd"),
            ShellType::PowerShell => write!(f, "powershell"),
        }
    }
}

impl ShellType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bash" | "sh" | "zsh" | "fish" => Some(ShellType::Bash),
            "cmd" | "cmd.exe" | "command" => Some(ShellType::Cmd),
            "powershell" | "pwsh" | "ps" => Some(ShellType::PowerShell),
            _ => None,
        }
    }

    pub fn default_for_platform() -> Self {
        if cfg!(target_os = "windows") {
            ShellType::PowerShell
        } else {
            ShellType::Bash
        }
    }
}

pub struct ShellAnalysisResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub command: String,
    pub shell_type: ShellType,
}

pub struct ShellAnalysisContext<'a> {
    pub command: &'a str,
    pub policy: &'a ShellPolicy,
}

pub trait ShellAnalyzer: Send + Sync {
    fn shell_type(&self) -> ShellType;

    fn analyze(&self, ctx: &ShellAnalysisContext) -> ShellAnalysisResult;
}
