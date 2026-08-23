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
use crate::strategy::shell::vfs_paths::parse_command_chain;

use regex::Regex;
use std::sync::OnceLock;

/// Upper bound on recursive wrapper stripping (prevents DoS).
pub const MAX_NORMALIZE_DEPTH: usize = 32;

fn fd_redirect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d*>&\d*").unwrap())
}

/// Strip fd redirect tokens (`2>&1`, `3>&2`) — they carry no command identity
/// and previously tripped up prefix matching.
fn strip_fd_redirect(s: &str) -> String {
    fd_redirect_re().replace(s, "").to_string()
}

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

/// Default severity → mode mapping. `warn` (downgrade) must be
/// explicitly requested — the default keeps everything Deny (fail-closed).
pub fn decision_mode_for(severity: Severity, allow_warn: bool) -> DecisionMode {
    match severity {
        Severity::Critical | Severity::High => DecisionMode::Deny,
        Severity::Medium if allow_warn => DecisionMode::Warn,
        Severity::Medium => DecisionMode::Deny,
        Severity::Low if allow_warn => DecisionMode::Log,
        Severity::Low => DecisionMode::Deny,
    }
}

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
    fn deny(severity: Severity, reason: impl Into<String>) -> Self {
        Self {
            decision: CommandDecision::AutoDeny,
            severity: Some(severity),
            reason: Some(reason.into()),
            confidence: 1.0,
        }
    }

    fn approve() -> Self {
        Self {
            decision: CommandDecision::AutoApprove,
            severity: None,
            reason: None,
            confidence: 1.0,
        }
    }

    fn ask() -> Self {
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

// ---------------------------------------------------------------------------
// Command normalization
// ---------------------------------------------------------------------------

/// Strip wrapper prefixes (`sudo`, `env`, `command`, `time`, `nice`, `nohup`,
/// backslash, `start`, `@`, `&`, `$x =`) and absolute path prefixes
/// (`/usr/bin/git` → `git`, `C:\...\notepad.exe` → `notepad`) so pattern and
/// allow/deny matching sees the real command. Recursive with a fixed depth cap
/// to avoid DoS. Non-destructive: returns a string used for *matching* only;
/// the original command is never replaced for execution.
pub fn normalize_command(command: &str, shell_type: ShellType) -> String {
    let mut cur = command.trim().to_string();
    for _ in 0..MAX_NORMALIZE_DEPTH {
        let next = normalize_once(&cur, shell_type);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur
}

fn join_tokens(tokens: &[String]) -> String {
    tokens.join(" ")
}

/// Lightweight whitespace split that preserves backslashes (Windows paths).
fn split_raw(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

fn is_var_assignment(t: &str) -> bool {
    if t.is_empty() || t.starts_with('-') || t.starts_with('=') {
        return false;
    }
    t.find('=').map(|eq| eq > 0).unwrap_or(false)
}

fn normalize_once(cmd: &str, shell_type: ShellType) -> String {
    // Windows dialects keep backslashes in paths, which `shlex` would eat as
    // escapes — split them on raw whitespace instead.
    let tokens = match shell_type {
        ShellType::Cmd | ShellType::PowerShell => split_raw(cmd),
        _ => shlex::split(cmd).unwrap_or_else(|| split_raw(cmd)),
    };
    if tokens.is_empty() {
        return String::new();
    }

    let first = tokens[0].as_str();
    let mut idx = 0usize;

    match shell_type {
        ShellType::Bash => {
            // Absolute path prefix: /usr/bin/git -> git
            if first.starts_with('/') && first.len() > 1 {
                let base = first.rsplit('/').next().unwrap_or(first);
                if !base.is_empty() {
                    let mut out = vec![base.to_string()];
                    out.extend(tokens[1..].iter().cloned());
                    return join_tokens(&out);
                }
            }
            match first {
                "sudo" => idx = skip_sudo_args(&tokens, 1),
                "env" => idx = skip_env_args(&tokens, 1),
                "command" => {
                    // `command -v` / `command -V` are queries — never strip.
                    if tokens
                        .get(1)
                        .map(|t| t == "-v" || t == "-V")
                        .unwrap_or(false)
                    {
                        return cmd.trim().to_string();
                    }
                    idx = 1;
                    if tokens
                        .get(idx)
                        .map(|t| t == "-p" || t == "--")
                        .unwrap_or(false)
                    {
                        idx += 1;
                    }
                }
                "time" => idx = 1,
                "nice" => {
                    idx = 1;
                    if tokens.get(idx).map(|t| t == "-n").unwrap_or(false) {
                        idx += 2;
                    }
                }
                "nohup" => idx = 1,
                "\\" => idx = 1,
                _ => {}
            }
        }
        ShellType::Cmd => {
            if first.eq_ignore_ascii_case("start") {
                idx = 1;
                while idx < tokens.len() && tokens[idx].starts_with('/') {
                    idx += 1;
                }
            } else if first.starts_with('@') && first.len() > 1 {
                let mut out = vec![first[1..].to_string()];
                out.extend(tokens[1..].iter().cloned());
                return join_tokens(&out);
            }
        }
        ShellType::PowerShell => {
            if first == "&" {
                idx = 1;
            } else if first.starts_with('$') && tokens.get(1).map(|t| t == "=").unwrap_or(false) {
                idx = 2;
            }
        }
    }

    if idx >= tokens.len() {
        return String::new();
    }
    join_tokens(&tokens[idx..])
}

fn skip_sudo_args(tokens: &[String], mut idx: usize) -> usize {
    // Flags that consume a value; everything else starting with `-` is
    // skipped as a bare flag.
    const VALUE_FLAGS: &[&str] = &[
        "-u", "-g", "-C", "-D", "-R", "-r", "-t", "-T", "-p", "-P", "-U", "-A",
    ];
    while idx < tokens.len() {
        let t = tokens[idx].as_str();
        if t == "--" {
            idx += 1;
            break;
        }
        if VALUE_FLAGS.contains(&t) {
            idx += 2;
        } else if t.starts_with('-') && t.len() > 1 {
            idx += 1;
        } else {
            break;
        }
    }
    idx
}

fn skip_env_args(tokens: &[String], mut idx: usize) -> usize {
    while idx < tokens.len() {
        let t = tokens[idx].as_str();
        if t == "-i" || t == "--" {
            idx += 1;
        } else if t == "-u" && idx + 1 < tokens.len() {
            idx += 2;
        } else if (t.starts_with('-') && t.len() > 1) || is_var_assignment(t) {
            idx += 1;
        } else {
            break;
        }
    }
    idx
}

// ---------------------------------------------------------------------------
// Lexical primary-command extraction (merged from bash.rs / cmd.rs /
// powershell.rs)
// ---------------------------------------------------------------------------

/// Extract the lexical primary command from a token list, dialect-aware.
pub fn primary_command(tokens: &[String], shell_type: ShellType) -> Option<String> {
    match shell_type {
        ShellType::Bash => primary_bash(tokens),
        ShellType::Cmd => primary_cmd(tokens),
        ShellType::PowerShell => primary_powershell(tokens),
    }
}

fn primary_bash(tokens: &[String]) -> Option<String> {
    const PREFIX: &[&str] = &["time", "env", "nice", "nohup", "command", "\\"];
    let mut idx = 0;
    while idx < tokens.len() && PREFIX.contains(&tokens[idx].as_str()) {
        idx += 1;
    }
    let t = tokens.get(idx)?;
    Some(
        t.chars()
            .filter(|c| {
                c.is_alphanumeric()
                    || *c == '_'
                    || *c == '-'
                    || *c == '.'
                    || *c == '/'
                    || *c == '\\'
            })
            .collect(),
    )
}

fn primary_cmd(tokens: &[String]) -> Option<String> {
    let without_start = {
        let first = tokens.first()?;
        if first.eq_ignore_ascii_case("start") {
            tokens
                .iter()
                .find(|w| !w.starts_with('/'))
                .cloned()
                .unwrap_or_default()
        } else {
            first.trim_start_matches('@').to_string()
        }
    };
    if without_start.is_empty() {
        return None;
    }
    let basename = without_start
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&without_start);
    let name = if let Some(dot) = basename.rfind('.') {
        let ext = &basename[dot + 1..];
        if matches!(ext, "exe" | "com" | "bat" | "cmd") {
            basename[..dot].to_string()
        } else {
            basename.to_string()
        }
    } else {
        basename.to_string()
    };
    Some(name.to_lowercase())
}

fn primary_powershell(tokens: &[String]) -> Option<String> {
    let mut idx = 0;
    while idx + 1 < tokens.len() && tokens[idx].starts_with('$') && tokens[idx + 1] == "=" {
        idx += 2;
    }
    let first = tokens.get(idx)?;
    if first.is_empty() {
        return None;
    }
    let stripped = first.strip_prefix('&').unwrap_or(first).trim_start();
    let lower = stripped.to_lowercase();
    if let Some(&resolved) =
        crate::strategy::shell::powershell::build_alias_map().get(lower.as_str())
    {
        return Some(resolved.to_string());
    }
    Some(stripped.replace(['"', '\''], ""))
}

// ---------------------------------------------------------------------------
// Context classification — mask data spans
// ---------------------------------------------------------------------------

/// Mask single-quoted data and shell comments so dangerous-pattern matching
/// never fires on data. Conservative: double quotes and ambiguous spans are
/// kept as Executed (false positives are acceptable, false negatives are not).
pub fn mask_data_spans(command: &str, shell_type: ShellType) -> String {
    let mut out = String::with_capacity(command.len());
    let mut in_single = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if escaped {
            out.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        if ch == '\\' && !in_single {
            out.push(ch);
            escaped = true;
            i += 1;
            continue;
        }
        if ch == '\'' && matches!(shell_type, ShellType::Bash | ShellType::PowerShell) {
            out.push(' ');
            in_single = !in_single;
            i += 1;
            continue;
        }
        if in_single {
            out.push(' ');
            i += 1;
            continue;
        }
        // Comment: `#` at line start or after whitespace (bash/powershell).
        if ch == '#'
            && matches!(shell_type, ShellType::Bash | ShellType::PowerShell)
            && (i == 0 || chars[i - 1].is_whitespace())
        {
            while i < chars.len() {
                out.push(' ');
                i += 1;
            }
            break;
        }
        out.push(ch);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Quick reject — keyword pre-filter for the hot path
// ---------------------------------------------------------------------------

/// Extract alphanumeric-ish tokens (len >= 2) from a rule pattern or command
/// word, lowercased. Used to build the quick-reject keyword set.
fn extract_literal_tokens(s: &str) -> Vec<String> {
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    // No `.` in the character class: `curl.*` must yield `curl`, not `curl.`.
    let re = TOKEN_RE.get_or_init(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_\-]*").unwrap());
    re.find_iter(s)
        .map(|m| m.as_str().to_lowercase())
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Collect every literal keyword that can trigger a deny decision from the
/// effective dangerous patterns and deny rules. Missing a keyword here only
/// means the command falls back to the full analysis — never a bypass.
pub fn quick_reject_keywords(patterns: &[String], commands: &[String]) -> Vec<String> {
    let mut set = std::collections::HashSet::new();
    for p in patterns {
        for t in extract_literal_tokens(p) {
            set.insert(t);
        }
    }
    for c in commands {
        for t in extract_literal_tokens(c) {
            set.insert(t);
        }
    }
    set.into_iter().collect()
}

/// Returns `true` when `command` contains none of the keywords and may be
/// fast-rejected (skipped). Callers must first guarantee the conservative
/// guard (no quotes / backslashes / command substitution) so a masked or
/// escaped danger is never shortcut.
pub fn quick_reject(command: &str, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return true;
    }
    let lower = command.to_lowercase();
    !keywords.iter().any(|k| lower.contains(k))
}

// ---------------------------------------------------------------------------
// Inline script / heredoc detection (second pass)
// ---------------------------------------------------------------------------

/// Interpreters whose `-c` / `-e` / `-r` / `--command` argument carries an
/// inline script that must be scanned for dangerous content.
const INLINE_INTERPRETERS: &[&str] = &[
    "python", "python2", "python3", "bash", "sh", "zsh", "node", "nodejs", "ruby", "perl", "php",
    "lua", "luajit",
];
const INLINE_CODE_FLAGS: &[&str] = &[
    "-c",
    "-e",
    "-r",
    "--command",
    "-Command",
    "-command",
    "-EncodedCommand",
];

/// Extract inline script bodies (`python3 -c '...'`, `bash -c '...'`,
/// `node -e '...'`, heredoc `<<EOF ... EOF`) for a second dangerous-pattern
/// pass. The extracted bodies are scanned verbatim (single quotes are NOT
/// masked) so `os.system('rm -rf /')` is caught.
pub fn extract_inline_scripts(command: &str, shell_type: ShellType) -> Vec<String> {
    let mut out = Vec::new();

    // 1. `<interpreter> <code-flag> <code>` (quotes already resolved by shlex).
    let tokens = shlex::split(command).unwrap_or_default();
    for i in 0..tokens.len() {
        if !INLINE_INTERPRETERS.contains(&tokens[i].to_lowercase().as_str()) {
            continue;
        }
        if let Some(flag) = tokens.get(i + 1) {
            if INLINE_CODE_FLAGS.contains(&flag.as_str()) {
                if let Some(code) = tokens.get(i + 2) {
                    // `-EncodedCommand` payloads are base64 — flag them
                    // without attempting to scan the blob.
                    if flag.eq_ignore_ascii_case("-encodedcommand") {
                        out.push("<base64-encoded-command>".to_string());
                    } else {
                        out.push(code.clone());
                    }
                }
            }
        }
    }

    // 2. Heredoc bodies: `<<[-]WORD` ... a line equal to `WORD`.
    if matches!(shell_type, ShellType::Bash | ShellType::PowerShell) {
        let mut cursor = 0usize;
        while let Some(rel) = command[cursor..].find("<<") {
            let start = cursor + rel;
            let after = command[start + 2..]
                .strip_prefix('-')
                .unwrap_or(&command[start + 2..]);
            let trimmed = after.trim_start();
            // Delimiter word is alphanumeric/underscore.
            let word_len = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            if word_len == 0 {
                break;
            }
            let word: String = trimmed.chars().take(word_len).collect();
            let body_start = match command[start + 2..].find('\n') {
                Some(p) => start + 2 + p + 1,
                None => break,
            };
            let end_marker = format!("\n{word}");
            match command[body_start..].find(&end_marker) {
                Some(rel_end) => {
                    let body = &command[body_start..body_start + rel_end];
                    if !body.is_empty() {
                        out.push(body.to_string());
                    }
                    cursor = body_start + rel_end;
                }
                None => break,
            }
        }
    }

    out
}

/// Mask shell comments only (`# ...`), keeping single-quoted content visible.
/// Used for inline-script bodies where quote-hiding would be a bypass.
pub fn mask_comments_only(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            while i < chars.len() {
                out.push(' ');
                i += 1;
            }
            break;
        }
        out.push(ch);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Unified evaluation
// ---------------------------------------------------------------------------

/// Unified command-identity decision. `shell_policy` carries the sandbox
/// shell rules; `rules` carries the engine-level rules. Deny is a union
/// (sandbox denies default to the dialect blacklist), allow is the stricter
/// side (a sandbox whitelist replaces the engine allowlist — tighten only).
pub fn evaluate_command(
    command: &str,
    shell_type: ShellType,
    rules: &CommandRules,
    shell_policy: Option<&ShellPolicy>,
) -> CommandDecision {
    evaluate_command_verdict(command, shell_type, rules, shell_policy).decision
}

/// Like [`evaluate_command`] but also returns a human-readable denial reason
/// (useful for the sandbox static-analysis gate error messages).
pub fn evaluate_command_reason(
    command: &str,
    shell_type: ShellType,
    rules: &CommandRules,
    shell_policy: Option<&ShellPolicy>,
) -> (CommandDecision, Option<String>) {
    let verdict = evaluate_command_verdict(command, shell_type, rules, shell_policy);
    (verdict.decision, verdict.reason)
}

/// Full verdict (decision + severity + confidence) for a command.
pub fn evaluate_command_verdict(
    command: &str,
    shell_type: ShellType,
    rules: &CommandRules,
    shell_policy: Option<&ShellPolicy>,
) -> CommandVerdict {
    let command = command.trim();
    if command.is_empty() {
        return CommandVerdict::approve();
    }

    let mut denied: Vec<String> = rules
        .denied_commands
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let mut allowed: Vec<String> = rules
        .allowed_commands
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    if let Some(sp) = shell_policy {
        match &sp.denied_commands {
            Some(d) => denied.extend(d.iter().map(|s| s.to_lowercase())),
            None => denied.extend(
                default_denied_commands(shell_type)
                    .iter()
                    .map(|s| s.to_lowercase()),
            ),
        }
        if let Some(a) = &sp.allowed_commands {
            if !a.is_empty() && !a.iter().any(|x| x == "*") {
                allowed = a.iter().map(|s| s.to_lowercase()).collect();
            }
        }
    }

    let normalized = normalize_command(command, shell_type);
    if normalized.trim().is_empty() {
        return CommandVerdict::deny(Severity::High, "Empty command after normalization");
    }

    // Chain both the raw and the normalized command so the raw primary (e.g.
    // `sudo`) stays blacklistable while sub-command rules run on the
    // normalized string. Normalization never changes the chain boundaries.
    let raw_subs = parse_command_chain(command);
    let norm_subs = parse_command_chain(&normalized);
    let mut has_ask = false;
    for (raw_sub, norm_sub) in raw_subs.iter().zip(norm_subs.iter()) {
        let raw_tokens = shlex::split(raw_sub).unwrap_or_default();
        let raw_primary = primary_command(&raw_tokens, shell_type).unwrap_or_default();

        let sub_norm = strip_fd_redirect(&normalize_command(norm_sub, shell_type));
        if sub_norm.trim().is_empty() {
            continue;
        }
        let norm_tokens = shlex::split(&sub_norm).unwrap_or_default();
        let norm_primary = primary_command(&norm_tokens, shell_type).unwrap_or_default();

        let verdict = evaluate_single(&sub_norm, &raw_primary, &norm_primary, &allowed, &denied);
        match verdict.decision {
            CommandDecision::AutoDeny => return verdict,
            CommandDecision::AskUser => has_ask = true,
            CommandDecision::AutoApprove => {}
        }
    }
    if has_ask {
        CommandVerdict::ask()
    } else {
        CommandVerdict::approve()
    }
}

fn evaluate_single(
    normalized_sub: &str,
    raw_primary: &str,
    norm_primary: &str,
    allowed: &[String],
    denied: &[String],
) -> CommandVerdict {
    let raw_p = raw_primary.to_lowercase();
    let norm_p = norm_primary.to_lowercase();

    // 1. Command-level blacklist: exact match on either primary form.
    for cand in [&raw_p, &norm_p] {
        if !cand.is_empty()
            && denied
                .iter()
                .any(|d| !d.contains(' ') && !d.contains('\t') && d == cand)
        {
            return CommandVerdict::deny(
                Severity::High,
                format!("Command denied by blacklist: {cand}"),
            );
        }
    }

    // 2. Sub-command blacklist: prefix match on the normalized string.
    if let Some(rule) = prefix_rule_hit(normalized_sub, denied) {
        return CommandVerdict::deny(
            Severity::High,
            format!("Command denied by blacklist: {rule}"),
        );
    }

    // 3. Whitelist off -> allow.
    if allowed.is_empty() || allowed.iter().any(|a| a == "*") {
        return CommandVerdict::approve();
    }

    // 4. Command-level whitelist: exact match on the normalized primary.
    if !norm_p.is_empty() && allowed.iter().any(|a| !a.contains(' ') && a == &norm_p) {
        return CommandVerdict::approve();
    }

    // 5. Sub-command whitelist: prefix match.
    if prefix_rule_hit(normalized_sub, allowed).is_some() {
        return CommandVerdict::approve();
    }

    CommandVerdict::ask()
}

/// Match whitespace-bearing rules (sub-command patterns) against `target` by
/// longest prefix. Returns the matching rule, if any.
fn prefix_rule_hit(target: &str, rules: &[String]) -> Option<String> {
    let lower = target.to_lowercase();
    let mut best: Option<String> = None;
    for r in rules {
        if !r.contains(' ') && !r.contains('\t') {
            continue;
        }
        if lower.starts_with(r.as_str()) {
            match &best {
                Some(b) if r.len() > b.len() => best = Some(r.clone()),
                None => best = Some(r.clone()),
                _ => {}
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Rule-table accessors (addressable, pack-filterable rules)
// ---------------------------------------------------------------------------

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
