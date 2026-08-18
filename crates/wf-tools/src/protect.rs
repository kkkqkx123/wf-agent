const DEFAULT_PROTECTED_PATTERNS: &[&str] = &[
    ".agentignore",
    ".agentrules*",
    ".agentconfig",
    ".vscode/**",
    ".idea/**",
    "*.code-workspace",
    ".git/**",
    ".gitignore",
    ".gitattributes",
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "tsconfig.json",
    "turbo.json",
    ".github/**",
    ".gitlab-ci.yml",
    "Jenkinsfile",
    "README.md",
    "LICENSE",
    "CHANGELOG.md",
];

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = if pattern.contains('/') {
        pattern.to_string()
    } else {
        format!("**/{}", pattern)
    };

    let glob_pattern = if pattern.ends_with("/**") || pattern.ends_with("/*") {
        pattern.to_string()
    } else if !pattern.contains('*') && !pattern.contains('?') {
        format!("{}/**", pattern.trim_end_matches('/'))
    } else {
        pattern.to_string()
    };

    let opts = globset::GlobBuilder::new(&glob_pattern)
        .case_insensitive(true)
        .literal_separator(true)
        .build();

    let Ok(glob) = opts else {
        return false;
    };
    let matcher = glob.compile_matcher();
    matcher.is_match(path)
}

#[derive(Debug, Clone)]
pub struct ProtectController {
    patterns: Vec<String>,
}

impl ProtectController {
    pub fn new(additional_patterns: Option<&[String]>) -> Self {
        let mut patterns: Vec<String> = DEFAULT_PROTECTED_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();

        if let Some(extra) = additional_patterns {
            patterns.extend_from_slice(extra);
        }

        Self { patterns }
    }

    pub fn is_write_protected(&self, file_path: &str) -> bool {
        let normalized = file_path.replace('\\', "/");
        self.patterns.iter().any(|p| glob_match(p, &normalized))
    }

    pub fn get_protected_files<'a>(&self, paths: &'a [String]) -> Vec<&'a str> {
        paths
            .iter()
            .filter(|p| self.is_write_protected(p))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn annotate_paths(&self, paths: &[String]) -> Vec<(String, bool)> {
        paths
            .iter()
            .map(|p| (p.clone(), self.is_write_protected(p)))
            .collect()
    }

    pub fn get_protection_message(&self) -> &str {
        "This file is write-protected and requires approval for modifications"
    }

    pub fn get_instructions(&self) -> String {
        let patterns = self.patterns.join(", ");
        format!(
            "# Protected Files\n\nThe following file patterns are write-protected and always require approval for modifications, regardless of auto-approval settings.\n\nProtected patterns: {}",
            patterns
        )
    }

    pub fn get_protected_patterns(&self) -> &[String] {
        &self.patterns
    }
}

impl Default for ProtectController {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_config_files() {
        let pc = ProtectController::default();
        assert!(pc.is_write_protected(".env"));
        assert!(pc.is_write_protected("src/.env"));
        assert!(pc.is_write_protected(".git/config"));
        assert!(pc.is_write_protected("package.json"));
        assert!(pc.is_write_protected("tsconfig.json"));
    }

    #[test]
    fn test_non_protected_files() {
        let pc = ProtectController::default();
        assert!(!pc.is_write_protected("src/main.rs"));
        assert!(!pc.is_write_protected("lib/foo.ts"));
        assert!(!pc.is_write_protected("test/test.js"));
    }

    #[test]
    fn test_annotate_paths() {
        let pc = ProtectController::default();
        let paths = vec![
            ".env".to_string(),
            "src/main.rs".to_string(),
            "package.json".to_string(),
        ];
        let annotated = pc.annotate_paths(&paths);
        assert!(annotated[0].1);
        assert!(!annotated[1].1);
        assert!(annotated[2].1);
    }

    #[test]
    fn test_custom_patterns() {
        let custom = vec!["my_secrets/**".to_string()];
        let pc = ProtectController::new(Some(&custom));
        assert!(pc.is_write_protected("my_secrets/key.txt"));
    }
}
