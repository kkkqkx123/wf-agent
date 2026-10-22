use super::*;
use crate::strategy::shell::vfs_paths::parse_command_chain;

/// How a rule hit is handled once a severity is known.
// DecisionMode lives in facade.

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

        let sub_norm = super::normalize::strip_fd_redirect(&normalize_command(norm_sub, shell_type));
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