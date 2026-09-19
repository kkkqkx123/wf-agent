use super::types::ScriptRiskLevel;

/// Heuristic risk hint only. Matching is substring based and can be
/// bypassed by obfuscation; real isolation is owned by the sandbox and
/// the transport. Policy gates use the score as an advisory ceiling.
pub struct RiskEvaluator;

impl RiskEvaluator {
    pub fn evaluate(content: &str) -> ScriptRiskLevel {
        let lowered = Self::normalize(&content.to_lowercase());
        if Self::looks_critical(&lowered) {
            ScriptRiskLevel::Critical
        } else if Self::looks_high(&lowered) {
            ScriptRiskLevel::High
        } else if Self::looks_medium(&lowered) {
            ScriptRiskLevel::Medium
        } else if Self::looks_low(&lowered) {
            ScriptRiskLevel::Low
        } else {
            ScriptRiskLevel::Safe
        }
    }

    /// Collapse quoting and whitespace noise so trivial spacing tricks do
    /// not slip past the hint matchers.
    fn normalize(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut gap = false;
        for ch in text.chars() {
            if ch == '\'' || ch == '"' || ch == '`' {
                continue;
            }
            if ch.is_whitespace() {
                gap = true;
                continue;
            }
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(ch);
        }
        out
    }

    fn looks_critical(text: &str) -> bool {
        text.contains(":(){:|:&};:")
            || text.contains("mkfs")
            || text.contains("of=/dev/")
            || Self::has_root_wipe(text)
    }

    fn has_root_wipe(text: &str) -> bool {
        match text.find("rm") {
            Some(pos) => {
                let tail = &text[pos..];
                tail.contains("-rf") && tail.contains(" /")
            }
            None => false,
        }
    }

    fn looks_high(text: &str) -> bool {
        text.contains("rm -rf")
            || text.contains("rm -fr")
            || text.contains("chmod -r 777")
            || text.contains("chmod -r777")
            || text.contains("sudo ")
            || text.contains("eval $(")
            || text.contains("nc -l")
            || Self::has_pipe_to_shell(text)
    }

    fn has_pipe_to_shell(text: &str) -> bool {
        let piped = text.contains("| sh")
            || text.contains("|sh")
            || text.contains("| bash")
            || text.contains("|bash");
        piped && (text.contains("curl") || text.contains("wget"))
    }

    fn looks_medium(text: &str) -> bool {
        text.contains("curl")
            || text.contains("wget")
            || text.contains("ssh ")
            || text.contains("docker ")
            || text.contains("kubectl ")
            || text.contains("chmod +x")
            || text.contains("eval ")
    }

    fn looks_low(text: &str) -> bool {
        text.contains("git ")
            || text.contains("npm ")
            || text.contains("cargo ")
            || text.contains("pip ")
            || text.contains("python ")
            || text.contains("node ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_wipe() {
        assert_eq!(
            RiskEvaluator::evaluate("rm -rf / --no-preserve-root"),
            ScriptRiskLevel::Critical
        );
    }

    #[test]
    fn test_high_pipe_to_shell() {
        assert_eq!(
            RiskEvaluator::evaluate("curl https://example.com/install.sh | sh"),
            ScriptRiskLevel::High
        );
    }

    #[test]
    fn test_safe_echo() {
        assert_eq!(RiskEvaluator::evaluate("echo hello"), ScriptRiskLevel::Safe);
    }
}
