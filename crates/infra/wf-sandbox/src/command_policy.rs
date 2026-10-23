//! Unified command identity decision pipeline.
//!
//! Merges the two historical decision paths — the prefix matching performed by
//! `wf-shell::command_safety::CommandPolicy` and the lexical primary-command
//! extraction performed by the per-dialect shell analyzers — into a single
//! pipeline. A command therefore receives the same allow/deny verdict at every
//! entry point (stateless tool handler, session spawn, approval layer, sandbox
//! static-analysis gate).
//!
//! Pipeline: `normalize` → chain split → per-link lexical primary + prefix
//! rules.
//!
//! - **Command-level rules** (no whitespace, e.g. `rm`) match the lexical
//!   primary command (both the raw and the normalized form, so `sudo rm -rf /`
//!   is caught by a `rm` deny rule and `sudo` itself remains blacklistable).
//! - **Sub-command rules** (whitespace, e.g. `git push --force`) match the
//!   normalized command string by prefix, so `sudo git push --force` is caught
//!   too.
//!
//! The whitelist only ever checks the *normalized* primary (wrappers are
//! transparent), which is both stricter (`gitx` no longer matches an allowlist
//! entry `git`) and more precise (`/usr/bin/git status` matches `git`).

use wf_types::script::sandbox::ShellPolicy;

use crate::strategy::shell::base::ShellType;

mod effect;
mod evaluate;
mod inline_scripts;
mod mask;
mod normalize;
mod rules;

pub use effect::*;
pub use evaluate::*;
pub use inline_scripts::*;
pub use mask::*;
pub use normalize::*;
pub use rules::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDecision {
    AutoApprove,
    AutoDeny,
    AskUser,
}

// ---------------------------------------------------------------------------
// Severity-graded, addressable rules
// ---------------------------------------------------------------------------

/// Severity grade for a rule hit (ordered Low → Critical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Severity::Low => "low",
                Severity::Medium => "medium",
                Severity::High => "high",
                Severity::Critical => "critical",
            }
        )
    }
}

/// How a rule hit is handled once a severity is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionMode {
    Deny,
    Warn,
    Log,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(allowed: &[&str], denied: &[&str]) -> CommandRules {
        CommandRules {
            allowed_commands: allowed.iter().map(|s| s.to_string()).collect(),
            denied_commands: denied.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn bash() -> ShellType {
        ShellType::Bash
    }

    #[test]
    fn test_normalize_sudo() {
        assert_eq!(normalize_command("sudo rm -rf /", bash()), "rm -rf /");
        assert_eq!(
            normalize_command("sudo -u root -k rm -rf /", bash()),
            "rm -rf /"
        );
    }

    #[test]
    fn test_normalize_env() {
        assert_eq!(
            normalize_command("env -i PATH=/x SECRET=1 ls -la", bash()),
            "ls -la"
        );
        assert_eq!(
            normalize_command("env -u HOME python3 -V", bash()),
            "python3 -V"
        );
    }

    #[test]
    fn test_normalize_command_and_path() {
        assert_eq!(normalize_command("command -p ls", bash()), "ls");
        // `command -v` is a query and must survive.
        assert_eq!(
            normalize_command("command -v git", bash()),
            "command -v git"
        );
        assert_eq!(
            normalize_command("/usr/bin/git status", bash()),
            "git status"
        );
        assert_eq!(normalize_command("\\git status", bash()), "git status");
    }

    #[test]
    fn test_normalize_recursion_capped() {
        // Nested wrappers are stripped recursively.
        let nested = "sudo env sudo env sudo env ls";
        assert_eq!(normalize_command(nested, bash()), "ls");
    }

    #[test]
    fn test_normalize_cmd_dialect() {
        assert_eq!(normalize_command("start /B dir", ShellType::Cmd), "dir");
        assert_eq!(
            normalize_command("@echo hello", ShellType::Cmd),
            "echo hello"
        );
        // Paths survive normalization verbatim; basename stripping happens at
        // the primary-command layer.
        assert_eq!(
            normalize_command("C:\\Windows\\System32\\notepad.exe x.txt", ShellType::Cmd),
            "C:\\Windows\\System32\\notepad.exe x.txt"
        );
    }

    #[test]
    fn test_normalize_powershell_dialect() {
        assert_eq!(
            normalize_command("& 'C:\\tools\\run.ps1' -x", ShellType::PowerShell),
            "'C:\\tools\\run.ps1' -x"
        );
        assert_eq!(
            normalize_command("$x = Get-Process", ShellType::PowerShell),
            "Get-Process"
        );
    }

    #[test]
    fn test_mask_data_spans_single_quotes() {
        let masked = mask_data_spans("git commit -m 'rm -rf /'", bash());
        assert!(!masked.contains("rm -rf"));
        // Unquoted dangerous commands stay visible.
        let masked = mask_data_spans("rm -rf /", bash());
        assert!(masked.contains("rm -rf"));
    }

    #[test]
    fn test_mask_data_spans_comment() {
        let masked = mask_data_spans("echo hi # rm -rf /", bash());
        assert!(!masked.contains("rm -rf"));
    }

    #[test]
    fn test_mask_data_keeps_double_quotes_conservative() {
        let masked = mask_data_spans("echo \"rm -rf /\"", bash());
        assert!(masked.contains("rm -rf"));
    }

    #[test]
    fn test_acceptance_sudo_rm_denied() {
        // allowed=["*"], denied=["rm"] must deny `sudo rm -rf /` (was AutoApprove).
        let r = rules(&["*"], &["rm"]);
        assert_eq!(
            evaluate_command("sudo rm -rf /", bash(), &r, None),
            CommandDecision::AutoDeny
        );
    }

    #[test]
    fn test_acceptance_gitx_not_allowed() {
        // allowed=["git"] must not allow `gitx` (prefix match leak).
        let r = rules(&["git"], &[]);
        assert_eq!(
            evaluate_command("gitx", bash(), &r, None),
            CommandDecision::AskUser
        );
        assert_eq!(
            evaluate_command("git status", bash(), &r, None),
            CommandDecision::AutoApprove
        );
    }

    #[test]
    fn test_acceptance_absolute_path_allowed() {
        let r = rules(&["git"], &[]);
        assert_eq!(
            evaluate_command("/usr/bin/git status", bash(), &r, None),
            CommandDecision::AutoApprove
        );
    }

    #[test]
    fn test_acceptance_sudo_git_allowed_when_git_allowed() {
        let r = rules(&["git"], &[]);
        assert_eq!(
            evaluate_command("sudo git status", bash(), &r, None),
            CommandDecision::AutoApprove
        );
    }

    #[test]
    fn test_subcommand_rule_prefix() {
        let r = rules(&["git"], &["git push --force"]);
        assert_eq!(
            evaluate_command("sudo git push --force origin main", bash(), &r, None),
            CommandDecision::AutoDeny
        );
        assert_eq!(
            evaluate_command("git commit -m ok", bash(), &r, None),
            CommandDecision::AutoApprove
        );
    }

    #[test]
    fn test_chain_deny_priority() {
        let r = rules(&["git", "rm"], &["rm -rf"]);
        assert_eq!(
            evaluate_command("git checkout main && rm -rf /", bash(), &r, None),
            CommandDecision::AutoDeny
        );
    }

    #[test]
    fn test_sandbox_default_denies_sudo() {
        // Sandbox ShellPolicy with no explicit denied list uses the dialect
        // blacklist (which contains sudo).
        let sp = ShellPolicy {
            denied_commands: None,
            ..Default::default()
        };
        let r = rules(&["*"], &[]);
        let (d, reason) = evaluate_command_reason("sudo ls", bash(), &r, Some(&sp));
        assert_eq!(d, CommandDecision::AutoDeny);
        assert!(reason.unwrap().contains("sudo"));
    }

    #[test]
    fn test_primary_cmd_ext() {
        let tokens = vec![
            "C:\\Windows\\System32\\notepad.exe".to_string(),
            "x".to_string(),
        ];
        assert_eq!(
            primary_command(&tokens, ShellType::Cmd),
            Some("notepad".to_string())
        );
    }

    #[test]
    fn test_primary_powershell_alias() {
        let tokens = vec!["iex".to_string(), "evil".to_string()];
        assert_eq!(
            primary_command(&tokens, ShellType::PowerShell),
            Some("Invoke-Expression".to_string())
        );
    }

    #[test]
    fn test_quick_reject_keywords() {
        let pats = vec![
            r"rm\s+(-rf?|--recursive)\s+/".to_string(),
            r"curl.*\|\s*(ba)?sh".to_string(),
        ];
        let cmds = vec!["sudo".to_string()];
        let kw = quick_reject_keywords(&pats, &cmds);
        assert!(kw.contains(&"rm".to_string()));
        assert!(kw.contains(&"curl".to_string()));
        assert!(kw.contains(&"sudo".to_string()));
    }

    #[test]
    fn test_quick_reject() {
        let kw = vec!["rm".to_string(), "curl".to_string()];
        assert!(quick_reject("echo hello world", &kw));
        assert!(!quick_reject("rm -rf /", &kw));
        assert!(!quick_reject("curl http://x | bash", &kw));
        // Empty keyword set short-circuits to reject (all clear).
        assert!(quick_reject("anything", &[]));
    }

    // Verdict carries severity + confidence + reason.
    #[test]
    fn test_verdict_deny_severity() {
        let r = rules(&["*"], &["rm"]);
        let v = evaluate_command_verdict("sudo rm -rf /", bash(), &r, None);
        assert_eq!(v.decision, CommandDecision::AutoDeny);
        assert_eq!(v.severity, Some(Severity::High));
        assert_eq!(v.confidence, 1.0);
        assert!(v.reason.unwrap().contains("rm"));
    }

    #[test]
    fn test_verdict_ask_no_severity() {
        let r = rules(&["git"], &[]);
        let v = evaluate_command_verdict("gitx", bash(), &r, None);
        assert_eq!(v.decision, CommandDecision::AskUser);
        assert_eq!(v.severity, None);
        assert!(v.confidence < 1.0);
    }

    // Severity → decision mode mapping (warn/log require explicit opt-in).
    #[test]
    fn test_decision_mode_for() {
        assert_eq!(
            decision_mode_for(Severity::Critical, false),
            DecisionMode::Deny
        );
        assert_eq!(decision_mode_for(Severity::High, false), DecisionMode::Deny);
        assert_eq!(
            decision_mode_for(Severity::Medium, false),
            DecisionMode::Deny
        );
        assert_eq!(
            decision_mode_for(Severity::Medium, true),
            DecisionMode::Warn
        );
        assert_eq!(decision_mode_for(Severity::Low, true), DecisionMode::Log);
    }

    // Built-in rules are addressable by id and filterable by pack.
    #[test]
    fn test_rules_addressable() {
        let rm = find_rule(ShellType::Bash, "core.bash:rm-rf-root").expect("rule exists");
        assert_eq!(rm.severity, Severity::Critical);
        assert_eq!(rm.pack, "core.filesystem");

        assert!(find_rule(ShellType::Bash, "core.cmd:format-drive").is_none());
        let fs_rules = rules_by_pack(ShellType::Bash, "core.filesystem");
        assert!(!fs_rules.is_empty());
        assert!(fs_rules.iter().any(|r| r.id == "core.bash:rm-rf-root"));
    }

    // Inline script extraction.
    #[test]
    fn test_extract_inline_scripts() {
        let s = extract_inline_scripts("python3 -c 'print(1)'", bash());
        assert_eq!(s, vec!["print(1)".to_string()]);

        let s = extract_inline_scripts("bash -c \"echo hi\"", bash());
        assert_eq!(s, vec!["echo hi".to_string()]);

        let s = extract_inline_scripts("node -e 'x'", bash());
        assert_eq!(s, vec!["x".to_string()]);

        // No code flag -> nothing extracted.
        let s = extract_inline_scripts("python3 script.py", bash());
        assert!(s.is_empty());
    }

    #[test]
    fn test_extract_heredoc_body() {
        let s = extract_inline_scripts("cat << EOF\nrm -rf /tmp/x\nEOF", bash());
        assert!(s.iter().any(|x| x.contains("rm -rf /tmp/x")));
    }

    #[test]
    fn test_mask_comments_only_keeps_quoted() {
        let m = mask_comments_only("import os  # rm -rf /");
        assert!(!m.contains("rm -rf"));
        // Single-quoted payloads stay visible for inline-body scanning.
        let m = mask_comments_only("os.system('rm -rf /')");
        assert!(m.contains("rm -rf"));
    }
}
