use async_trait::async_trait;
use wf_types::script::sandbox::{LuaPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};

pub struct LuaStaticAnalyzerStrategy;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident,
    Str,
    Number,
    Punct,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
}

/// Identifier-level tokenizer for Lua source.
///
/// Unlike regex-based matching this understands string literals, comments
/// and bracketed index expressions, so bypasses like `os["execute"]` or
/// concatenated keys like `os["exe".."cute"]` are visible to the analyzer.
fn tokenize(code: &str) -> Vec<Token> {
    let chars: Vec<char> = code.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < n {
        let ch = chars[i];

        // Whitespace
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // Comments: --[[ block ]] and -- line
        if ch == '-' && i + 1 < n && chars[i + 1] == '-' {
            if i + 3 < n && chars[i + 2] == '[' && chars[i + 3] == '[' {
                i += 4;
                while i + 1 < n && !(chars[i] == ']' && chars[i + 1] == ']') {
                    i += 1;
                }
                i = (i + 2).min(n);
            } else {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            continue;
        }

        // String literals (with backslash escapes)
        if ch == '\'' || ch == '"' {
            let quote = ch;
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < n {
                let c = chars[i];
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    break;
                }
                i += 1;
            }
            if i < n {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Str,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Identifiers
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < n && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < n
                && (chars[i].is_ascii_digit()
                    || chars[i].is_ascii_hexdigit()
                    || matches!(chars[i], '.' | 'x' | 'X' | 'p' | 'P'))
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Two-char operators
        let two: String = chars[i..(i + 2).min(n)].iter().collect();
        if matches!(two.as_str(), ".." | "==" | "~=" | "<=" | ">=" | "::") {
            tokens.push(Token {
                kind: TokenKind::Punct,
                text: two,
            });
            i += 2;
            continue;
        }

        tokens.push(Token {
            kind: TokenKind::Punct,
            text: ch.to_string(),
        });
        i += 1;
    }

    tokens
}

struct LuaStaticAnalyzer;

impl LuaStaticAnalyzer {
    /// Find the index of the matching close bracket for an opening bracket
    /// at `open_idx`, or `None` if unbalanced.
    fn find_matching_bracket(
        tokens: &[Token],
        open_idx: usize,
        open: char,
        close: char,
    ) -> Option<usize> {
        let mut depth = 0usize;
        for (idx, tok) in tokens.iter().enumerate().skip(open_idx) {
            if tok.kind != TokenKind::Punct {
                continue;
            }
            let mut chars = tok.text.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Check `os` / `io` field access chains such as `os.execute(`, `os["execute"](`
    /// or dynamic index forms `os["exe".."cute"]` that regexes cannot see.
    /// Returns a violation message if the access is dangerous.
    fn check_module_access(tokens: &[Token], i: usize, policy: &LuaPolicy) -> Option<String> {
        let base = tokens[i].text.as_str();
        let dangerous_fields: &[&str] = if base == "os" {
            &["execute", "remove", "rename", "exit"]
        } else {
            &["popen"]
        };

        let mut field: Option<String> = None;
        let mut dynamic = false;
        let j = i + 1;
        let mut after_access = j;

        match tokens
            .get(j)
            .map(|t| (t.kind == TokenKind::Punct, t.text.as_str()))
        {
            Some((true, ".")) => {
                if let Some(next) = tokens.get(j + 1) {
                    if next.kind == TokenKind::Ident {
                        field = Some(next.text.clone());
                        after_access = j + 2;
                    } else {
                        return Some(format!("Dynamic field access on '{base}' is not allowed"));
                    }
                }
            }
            Some((true, "[")) => {
                let close = Self::find_matching_bracket(tokens, j, '[', ']');
                let Some(close) = close else {
                    return Some(format!("Unbalanced bracket access on '{base}'"));
                };
                let inner: Vec<&Token> = tokens[j + 1..close].iter().collect();
                match inner.as_slice() {
                    [single] if single.kind == TokenKind::Str => {
                        let raw = single.text.trim_matches('\'').trim_matches('"');
                        field = Some(raw.to_string());
                    }
                    [] => return Some(format!("Empty index access on '{base}'")),
                    _ => {
                        // Concatenated / computed keys (`os["exe".."cute"]`) are
                        // not statically resolvable; fail closed.
                        dynamic = true;
                    }
                }
                after_access = close + 1;
            }
            _ => return None,
        }

        // Only enforce when the access is actually a call.
        if tokens
            .get(after_access)
            .map(|t| (t.kind == TokenKind::Punct, t.text.as_str()))
            != Some((true, "("))
        {
            return None;
        }

        if dynamic {
            return Some(format!("Dangerous dynamic index access: {base}[...]()"));
        }

        let name = field.as_deref().unwrap_or("");
        if base == "os" && name == "execute" && policy.allow_os_execute {
            return None;
        }
        if dangerous_fields.contains(&name) {
            return Some(format!("Dangerous function call: {base}.{name}()"));
        }
        None
    }

    /// Extract a string literal argument from a call whose identifier is at
    /// `call_idx` (i.e. `name('...')`).
    fn string_argument(tokens: &[Token], call_idx: usize) -> Option<String> {
        let open_paren = call_idx + 1;
        if tokens
            .get(open_paren)
            .map(|t| t.kind == TokenKind::Punct && t.text == "(")
            != Some(true)
        {
            return None;
        }
        let close = Self::find_matching_bracket(tokens, open_paren, '(', ')')?;
        let inner: Vec<&Token> = tokens[open_paren + 1..close].iter().collect();
        match inner.as_slice() {
            [single] if single.kind == TokenKind::Str => {
                Some(single.text.trim_matches('\'').trim_matches('"').to_string())
            }
            _ => None,
        }
    }

    fn analyze(code: &str, policy: &LuaPolicy) -> Vec<String> {
        let tokens = tokenize(code);
        let n = tokens.len();
        let mut violations = Vec::new();
        let mut i = 0usize;

        while i < n {
            let tok = &tokens[i];
            if tok.kind != TokenKind::Ident {
                i += 1;
                continue;
            }

            let name = tok.text.as_str();
            match name {
                "os" | "io" => {
                    if let Some(msg) = Self::check_module_access(&tokens, i, policy) {
                        violations.push(msg);
                    }
                    if name == "io" {
                        // io.open(path, mode) with write mode
                        let call_idx = tokens
                            .iter()
                            .position(|t| t.kind == TokenKind::Punct && t.text == "(");
                        if let Some(call_idx) = call_idx {
                            if policy.restrict_io_open {
                                let open = Self::find_matching_bracket(&tokens, call_idx, '(', ')');
                                if let Some(open) = open {
                                    let args: Vec<&Token> =
                                        tokens[call_idx + 1..open].iter().collect();
                                    if let Some(comma) = args
                                        .iter()
                                        .position(|t| t.kind == TokenKind::Punct && t.text == ",")
                                    {
                                        if let Some(mode_token) = args.get(comma + 1) {
                                            if mode_token.kind == TokenKind::Str {
                                                let mode = mode_token
                                                    .text
                                                    .trim_matches('\'')
                                                    .trim_matches('"');
                                                if mode
                                                    .chars()
                                                    .any(|c| matches!(c, 'w' | 'a' | 'x' | '+'))
                                                {
                                                    violations.push(format!(
                                                        "Write mode in io.open(): {mode}"
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "loadstring" | "load" | "dofile" | "loadfile" => {
                    let is_call = tokens
                        .get(i + 1)
                        .map(|t| t.kind == TokenKind::Punct && t.text == "(")
                        .unwrap_or(false);
                    if is_call {
                        if (name == "loadstring" || name == "load") && policy.allow_dynamic_load {
                            // allowed by policy
                        } else {
                            violations.push(format!("Dangerous function call: {name}()"));
                        }
                    }
                }
                "require" => {
                    if let Some(module) = Self::string_argument(&tokens, i) {
                        if !module.is_empty() {
                            let module_base =
                                module.split('.').next().unwrap_or(&module).to_string();
                            if !policy.allowed_modules.is_empty()
                                && !policy.allowed_modules.contains(&module_base)
                            {
                                violations.push(format!("Module not allowed: {module}"));
                            }
                            if policy.denied_modules.contains(&module_base) {
                                violations.push(format!("Module denied: {module}"));
                            }
                        }
                    }
                }
                "setfenv" | "getfenv" => {
                    let is_call = tokens
                        .get(i + 1)
                        .map(|t| t.kind == TokenKind::Punct && t.text == "(")
                        .unwrap_or(false);
                    if is_call {
                        violations.push(format!("Dangerous pattern: {name}"));
                    }
                }
                "_G" => {
                    let next = tokens.get(i + 1).map(|t| t.text.as_str());
                    if matches!(next, Some("[") | Some(".")) {
                        violations.push("Dangerous pattern: _G access".to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }

        violations
    }
}

fn violation_result(violations: &[String]) -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: false,
        script_name: "sandbox-lua".to_string(),
        stdout: None,
        stderr: Some(violations.join("; ")),
        exit_code: Some(1),
        execution_time: 0,
        error: Some(format!("Security violation: {}", violations[0])),
        sandbox_mode: None,
        strategy_id: Some("static-analyzer".to_string()),
        violations: Some(violations.to_vec()),
    }
}

fn allowed_result() -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: true,
        script_name: "sandbox-lua".to_string(),
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

#[async_trait]
impl StrategyImplementation for LuaStaticAnalyzerStrategy {
    fn id(&self) -> &str {
        "static-analyzer"
    }
    fn name(&self) -> &str {
        "Lua Static Analyzer"
    }
    fn description(&self) -> &str {
        "Identifier-level static analysis of Lua code for dangerous functions, modules and index accesses (analysis gate, does not execute)"
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
            return Ok(violation_result(&["Empty Lua code".to_string()]));
        }

        let lua_policy = policy.lua.as_ref().cloned().unwrap_or(LuaPolicy {
            allowed_modules: vec![],
            denied_modules: vec![],
            allow_os_execute: false,
            restrict_io_open: true,
            allow_dynamic_load: false,
        });

        let violations = LuaStaticAnalyzer::analyze(&command, &lua_policy);
        if !violations.is_empty() {
            return Ok(violation_result(&violations));
        }

        Ok(allowed_result())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(allow_os_execute: bool, allow_dynamic_load: bool) -> SandboxPolicy {
        SandboxPolicy {
            mode: Some(wf_types::script::sandbox::SandboxMode::Strict),
            lua: Some(LuaPolicy {
                allowed_modules: vec![],
                denied_modules: vec![
                    "os".to_string(),
                    "io".to_string(),
                    "package".to_string(),
                    "debug".to_string(),
                    "ffi".to_string(),
                ],
                allow_os_execute,
                restrict_io_open: true,
                allow_dynamic_load,
            }),
            shell: None,
            python: None,
            javascript: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        }
    }

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

    async fn analyze_code(command: &str, policy: &SandboxPolicy) -> Vec<String> {
        let strategy = LuaStaticAnalyzerStrategy;
        let result = strategy
            .execute(make_options(command), policy)
            .await
            .unwrap();
        result.violations.unwrap_or_default()
    }

    #[tokio::test]
    async fn test_denies_os_execute() {
        let violations = analyze_code("os.execute('rm -rf /')", &make_policy(false, false)).await;
        assert!(
            violations.iter().any(|v| v.contains("os.execute")),
            "os.execute must be denied: {violations:?}"
        );
    }

    #[tokio::test]
    async fn test_denies_os_bracket_execute() {
        let violations =
            analyze_code("os['execute']('rm -rf /')", &make_policy(false, false)).await;
        assert!(
            violations.iter().any(|v| v.contains("os.execute")),
            "os['execute'] bypass must be denied: {violations:?}"
        );
    }

    #[tokio::test]
    async fn test_denies_os_concatenated_key() {
        let violations = analyze_code(
            "os['exe' .. 'cute']('rm -rf /')",
            &make_policy(false, false),
        )
        .await;
        assert!(
            violations
                .iter()
                .any(|v| v.contains("os") && v.contains("...")),
            "concatenated key must be denied: {violations:?}"
        );
    }

    #[tokio::test]
    async fn test_denies_os_remove_rename_exit() {
        for cmd in ["os.remove('/tmp/f')", "os.rename('/a', '/b')", "os.exit(1)"] {
            let violations = analyze_code(cmd, &make_policy(true, false)).await;
            assert!(!violations.is_empty(), "must deny: {cmd}");
        }
    }

    #[tokio::test]
    async fn test_allows_os_execute_when_permitted() {
        let violations = analyze_code("os.execute('echo hi')", &make_policy(true, false)).await;
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[tokio::test]
    async fn test_denies_io_popen() {
        let violations = analyze_code("io.popen('rm -rf /')", &make_policy(false, false)).await;
        assert!(
            violations.iter().any(|v| v.contains("io.popen")),
            "{violations:?}"
        );
    }

    #[tokio::test]
    async fn test_denies_io_open_write_mode() {
        let violations = analyze_code("io.open('/tmp/f', 'w')", &make_policy(false, false)).await;
        assert!(
            violations.iter().any(|v| v.contains("Write mode")),
            "{violations:?}"
        );
    }

    #[tokio::test]
    async fn test_allows_io_open_read_mode() {
        let violations =
            analyze_code("io.open('/etc/hosts', 'r')", &make_policy(false, false)).await;
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[tokio::test]
    async fn test_denies_load_variants() {
        for cmd in [
            "loadstring('x')",
            "load('x')",
            "dofile('/tmp/f')",
            "loadfile('/tmp/f')",
        ] {
            let violations = analyze_code(cmd, &make_policy(false, false)).await;
            assert!(!violations.is_empty(), "must deny: {cmd}");
        }
    }

    #[tokio::test]
    async fn test_allows_load_when_dynamic_load_permitted() {
        let violations = analyze_code("load('x')", &make_policy(false, true)).await;
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[tokio::test]
    async fn test_denies_require_denied_module() {
        let violations = analyze_code("require('os')", &make_policy(false, false)).await;
        assert!(
            violations.iter().any(|v| v.contains("Module denied: os")),
            "{violations:?}"
        );
    }

    #[tokio::test]
    async fn test_require_whitelist_enforced() {
        let mut policy = make_policy(false, false);
        let lua = policy.lua.as_mut().unwrap();
        lua.allowed_modules = vec!["math".to_string()];
        let violations = analyze_code("require('string')", &policy).await;
        assert!(
            violations
                .iter()
                .any(|v| v.contains("Module not allowed: string")),
            "{violations:?}"
        );
        let violations = analyze_code("require('math.max')", &policy).await;
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[tokio::test]
    async fn test_denies_global_access() {
        for cmd in ["_G['x'] = 1", "_G.x = 1", "setfenv(1, {})", "getfenv(1)"] {
            let violations = analyze_code(cmd, &make_policy(false, false)).await;
            assert!(!violations.is_empty(), "must deny: {cmd}");
        }
    }

    #[tokio::test]
    async fn test_comments_are_ignored() {
        let cmd = "-- os.execute('rm -rf /')\nprint('safe')";
        let violations = analyze_code(cmd, &make_policy(false, false)).await;
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[tokio::test]
    async fn test_allows_benign_code() {
        for cmd in [
            "print('hello')",
            "local x = 1 + 2",
            "os.clock()",
            "table.insert(t, 1)",
        ] {
            let violations = analyze_code(cmd, &make_policy(false, false)).await;
            assert!(violations.is_empty(), "must allow: {cmd} => {violations:?}");
        }
    }

    #[tokio::test]
    async fn test_empty_command() {
        let strategy = LuaStaticAnalyzerStrategy;
        let result = strategy
            .execute(make_options(""), &make_policy(false, false))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Empty Lua code"));
    }
}
