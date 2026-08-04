use regex::Regex;
use wf_types::script::sandbox::{
    SandboxGlobalConfig, SandboxGlobalConfigError, SandboxMode, SandboxProfile,
    SandboxRuleMatchField,
};

/// Rule-based profile routing for the sandbox global configuration.
///
/// Rules are evaluated in declaration order and the FIRST matching rule wins,
/// selecting its referenced profile. Supported match fields:
/// - `Language` — script language id (`shell`, `python`, `javascript`/`js`, `lua`)
/// - `ScriptName` — script name, matched with glob patterns (`*` and `?`)
///
/// A resolver is built via [`SandboxProfileResolver::compile`], which
/// validates the whole configuration up front (fail-fast): every rule must
/// reference an existing profile and `default_profile` must exist. Unknown
/// match fields are rejected at deserialization time because
/// [`SandboxProfileRule::match_field`] is a closed enum. Glob patterns are
/// precompiled into regexes at compile time, so per-execution resolution is
/// infallible and only performs matching.
#[derive(Debug, Default)]
pub struct SandboxProfileResolver {
    global: SandboxGlobalConfig,
    rules: Vec<CompiledRule>,
}

#[derive(Debug)]
struct CompiledRule {
    match_field: SandboxRuleMatchField,
    profile: String,
    matcher: Regex,
}

/// Configuration errors detected while compiling a [`SandboxProfileResolver`].
#[derive(Debug, thiserror::Error)]
pub enum SandboxProfileError {
    /// Referential integrity errors, validated by
    /// [`SandboxGlobalConfig::validate`] in wf-types (single source of truth).
    #[error(transparent)]
    InvalidGlobal(#[from] SandboxGlobalConfigError),

    #[error("invalid glob pattern '{pattern}': {error}")]
    InvalidGlob { pattern: String, error: String },
}

impl SandboxProfileResolver {
    /// Compile and validate a global sandbox configuration.
    ///
    /// Referential integrity (rules reference existing profiles,
    /// `default_profile` exists) is delegated to
    /// [`SandboxGlobalConfig::validate`] — the single source of truth shared
    /// with the wf-config load path (fail-closed at configuration load
    /// rather than at script execution). Glob patterns are compiled here;
    /// runtime resolution via [`Self::resolve`] cannot fail.
    pub fn compile(global: SandboxGlobalConfig) -> Result<Self, SandboxProfileError> {
        global.validate()?;
        let mut rules = Vec::with_capacity(global.rules.len());
        for rule in &global.rules {
            rules.push(CompiledRule {
                match_field: rule.match_field,
                profile: rule.profile.clone(),
                matcher: compile_glob(&rule.match_pattern)?,
            });
        }
        Ok(Self { global, rules })
    }

    /// Resolve the profile selected by the first matching rule, if any.
    ///
    /// `script_name` may be empty; rules on `ScriptName` then never match.
    pub fn resolve(&self, language: &str, script_name: &str) -> Option<&SandboxProfile> {
        for rule in &self.rules {
            let value = match rule.match_field {
                SandboxRuleMatchField::Language => language,
                SandboxRuleMatchField::ScriptName => script_name,
            };
            if !rule.matcher.is_match(value) {
                continue;
            }
            return self
                .global
                .profiles
                .iter()
                .find(|p| p.name == rule.profile);
        }
        None
    }

    /// Profile referenced by `default_profile`, if configured.
    pub fn default_profile(&self) -> Option<&SandboxProfile> {
        let name = self.global.default_profile.as_deref()?;
        self.global.profiles.iter().find(|p| p.name == name)
    }

    /// Global mode applied when neither config nor profile specifies one.
    pub fn mode(&self) -> Option<&SandboxMode> {
        self.global.mode.as_ref()
    }

    /// Whether audit events should be recorded.
    pub fn audit_logging(&self) -> bool {
        self.global.audit_logging
    }
}

/// Compile a glob pattern (`*` any run of characters, `?` single character)
/// into a regex anchored to the whole value.
fn compile_glob(pattern: &str) -> Result<Regex, SandboxProfileError> {
    let mut re = String::with_capacity(pattern.len() * 2 + 2);
    re.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).map_err(|e| SandboxProfileError::InvalidGlob {
        pattern: pattern.to_string(),
        error: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{
        SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule,
        SandboxRuleMatchField,
    };

    fn profile(name: &str) -> SandboxProfile {
        SandboxProfile {
            name: name.to_string(),
            description: None,
            mode: Some(SandboxMode::Lenient),
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            policy: None,
            vfs: None,
            workdir: None,
            env: None,
        }
    }

    fn rule(field: SandboxRuleMatchField, pattern: &str, profile: &str) -> SandboxProfileRule {
        SandboxProfileRule {
            match_field: field,
            match_pattern: pattern.to_string(),
            profile: profile.to_string(),
        }
    }

    fn compile(rules: Vec<SandboxProfileRule>) -> SandboxProfileResolver {
        let global = SandboxGlobalConfig {
            mode: Some(SandboxMode::Strict),
            profiles: vec![profile("lenient"), profile("strict")],
            rules,
            default_profile: None,
            audit_logging: true,
        };
        SandboxProfileResolver::compile(global).expect("test config must be valid")
    }

    #[test]
    fn test_first_matching_rule_wins() {
        let resolver = compile(vec![
            rule(SandboxRuleMatchField::Language, "shell", "lenient"),
            rule(SandboxRuleMatchField::Language, "python", "strict"),
        ]);
        let p = resolver.resolve("shell", "");
        assert_eq!(p.map(|p| p.name.as_str()), Some("lenient"));
        let p = resolver.resolve("python", "");
        assert_eq!(p.map(|p| p.name.as_str()), Some("strict"));
    }

    #[test]
    fn test_no_match_returns_none() {
        let resolver = compile(vec![rule(SandboxRuleMatchField::Language, "lua", "lenient")]);
        assert!(resolver.resolve("shell", "").is_none());
    }

    #[test]
    fn test_script_name_glob_match() {
        let resolver =
            compile(vec![rule(SandboxRuleMatchField::ScriptName, "data-*.py", "lenient")]);
        assert_eq!(
            resolver.resolve("python", "data-clean.py").map(|p| p.name.as_str()),
            Some("lenient")
        );
        assert!(resolver.resolve("python", "main.py").is_none());
    }

    #[test]
    fn test_unknown_profile_is_compile_error() {
        let global = SandboxGlobalConfig {
            mode: None,
            profiles: vec![profile("lenient")],
            rules: vec![rule(SandboxRuleMatchField::Language, "shell", "nope")],
            default_profile: None,
            audit_logging: true,
        };
        let err = SandboxProfileResolver::compile(global).expect_err("must fail");
        assert!(err.to_string().contains("unknown profile"), "error: {err}");
        assert!(err.to_string().contains("'shell'"), "error: {err}");
    }

    #[test]
    fn test_unknown_default_profile_is_compile_error() {
        let global = SandboxGlobalConfig {
            mode: None,
            profiles: vec![profile("lenient")],
            rules: vec![],
            default_profile: Some("nope".to_string()),
            audit_logging: true,
        };
        let err = SandboxProfileResolver::compile(global).expect_err("must fail");
        assert!(
            err.to_string().contains("default_profile"),
            "error: {err}"
        );
    }

    #[test]
    fn test_unknown_match_field_rejected_at_deserialization() {
        let err = serde_json::from_str::<SandboxProfileRule>(
            r#"{"match_field":"mode","match_pattern":"x","profile":"lenient"}"#,
        )
        .expect_err("unknown match_field must fail deserialization");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_match_field_serde_roundtrip() {
        let rule = rule(SandboxRuleMatchField::ScriptName, "*.py", "lenient");
        let json = serde_json::to_string(&rule).expect("serialize");
        assert!(json.contains("\"script_name\""), "json: {json}");
        let back: SandboxProfileRule =
            serde_json::from_str(&json).expect("deserialize back");
        assert_eq!(back.match_field, SandboxRuleMatchField::ScriptName);
    }

    #[test]
    fn test_default_profile_resolution() {
        let global = SandboxGlobalConfig {
            mode: None,
            profiles: vec![profile("lenient"), profile("strict")],
            rules: vec![],
            default_profile: Some("strict".to_string()),
            audit_logging: true,
        };
        let resolver =
            SandboxProfileResolver::compile(global).expect("valid config must compile");
        assert_eq!(
            resolver.default_profile().map(|p| p.name.as_str()),
            Some("strict")
        );
    }
}
