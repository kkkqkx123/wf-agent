const BUILTIN_IGNORE_DIRS: &[&str] = &[
    ".DS_Store",
    "*.swp",
    "node_modules",
    "__pycache__",
    "env",
    "venv",
    ".venv",
    "vendor",
    "deps",
    "Pods",
    "target/dependency",
    "build/dependencies",
    "dist",
    "out",
    "bundle",
    "target",
    ".next",
    ".nuxt",
    "tmp",
    "temp",
    ".tmp",
    ".temp",
    ".git",
    ".svn",
    ".hg",
    ".idea",
    ".vscode",
    ".zed",
    ".*",
    ".repomix-output.xml",
    ".env",
];

const CRITICAL_IGNORE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "__pycache__",
    "venv",
    "env",
    ".venv",
];

fn is_critical(dir_name: &str) -> bool {
    CRITICAL_IGNORE_DIRS.contains(&dir_name)
}

fn is_hidden_dir(dir_name: &str) -> bool {
    dir_name.starts_with('.') && dir_name != "." && dir_name != ".."
}

#[derive(Debug, Clone, PartialEq)]
pub enum IgnoreMode {
    Builtin,
    Gitignore,
    Custom,
    BuiltinGitignore,
    BuiltinCustom,
    All,
}

impl IgnoreMode {
    pub fn use_builtin(&self) -> bool {
        matches!(
            self,
            IgnoreMode::Builtin
                | IgnoreMode::BuiltinGitignore
                | IgnoreMode::BuiltinCustom
                | IgnoreMode::All
        )
    }

    pub fn use_gitignore(&self) -> bool {
        matches!(
            self,
            IgnoreMode::Gitignore | IgnoreMode::BuiltinGitignore | IgnoreMode::All
        )
    }

    pub fn use_custom(&self) -> bool {
        matches!(
            self,
            IgnoreMode::Custom | IgnoreMode::BuiltinCustom | IgnoreMode::All
        )
    }
}

fn build_glob_matcher(patterns: &[String]) -> globset::GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        if let Ok(glob) = globset::GlobBuilder::new(p)
            .literal_separator(true)
            .case_insensitive(true)
            .build()
        {
            builder.add(glob);
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| globset::GlobSet::empty())
}

fn builtin_patterns() -> Vec<String> {
    let mut patterns = Vec::new();
    for dir in BUILTIN_IGNORE_DIRS {
        if *dir == ".*" {
            patterns.push("**/.*/**".to_string());
        } else if dir.contains('/') {
            patterns.push(format!("**/{}/**", dir));
        } else {
            patterns.push(dir.to_string());
            patterns.push(format!("**/{}/**", dir));
        }
    }
    patterns
}

pub struct IgnoreController {
    cwd: String,
    mode: IgnoreMode,
    builtin_matcher: globset::GlobSet,
    gitignore_matcher: Option<globset::GlobSet>,
    custom_matcher: Option<globset::GlobSet>,
}

impl IgnoreController {
    pub fn new(cwd: &str, mode: IgnoreMode) -> Self {
        let builtin = build_glob_matcher(&builtin_patterns());
        Self {
            cwd: cwd.to_string(),
            mode,
            builtin_matcher: builtin,
            gitignore_matcher: None,
            custom_matcher: None,
        }
    }

    pub fn with_gitignore(mut self, patterns: &[String]) -> Self {
        self.gitignore_matcher = Some(build_glob_matcher(patterns));
        self
    }

    pub fn with_custom(mut self, patterns: &[String]) -> Self {
        self.custom_matcher = Some(build_glob_matcher(patterns));
        self
    }

    fn relative_path(&self, file_path: &str) -> String {
        let cwd = std::path::Path::new(&self.cwd);
        let abs = cwd.join(file_path);
        let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
        match abs.strip_prefix(cwd) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => file_path.replace('\\', "/"),
        }
    }

    pub fn validate_access(&self, file_path: &str) -> bool {
        let rel = self.relative_path(file_path);

        if self.mode.use_builtin() && self.builtin_matcher.is_match(&rel) {
            return false;
        }

        if self.mode.use_gitignore() {
            if let Some(ref matcher) = self.gitignore_matcher {
                if matcher.is_match(&rel) {
                    return false;
                }
            }
        }

        if self.mode.use_custom() {
            if let Some(ref matcher) = self.custom_matcher {
                if matcher.is_match(&rel) {
                    return false;
                }
            }
        }

        true
    }

    pub fn should_include_directory(
        &self,
        dir_name: &str,
        full_path: &str,
        is_target_dir: bool,
        inside_explicit_target: bool,
    ) -> bool {
        if is_target_dir {
            return !is_critical(dir_name);
        }

        if inside_explicit_target {
            if is_critical(dir_name) {
                return false;
            }
            return self.validate_access(full_path);
        }

        if self.mode.use_builtin()
            && (BUILTIN_IGNORE_DIRS.contains(&dir_name) || is_hidden_dir(dir_name))
        {
            return false;
        }

        self.validate_access(full_path)
    }

    pub fn filter_paths(&self, paths: &[String]) -> Vec<String> {
        paths
            .iter()
            .filter(|p| self.validate_access(p))
            .cloned()
            .collect()
    }

    pub fn get_mode(&self) -> &IgnoreMode {
        &self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_ignore_node_modules() {
        let ic = IgnoreController::new("/workspace", IgnoreMode::Builtin);
        assert!(!ic.validate_access("node_modules/foo"));
        assert!(!ic.validate_access("src/node_modules/bar"));
    }

    #[test]
    fn test_does_not_ignore_source() {
        let ic = IgnoreController::new("/workspace", IgnoreMode::Builtin);
        assert!(ic.validate_access("src/main.rs"));
        assert!(ic.validate_access("lib/foo.ts"));
    }

    #[test]
    fn test_should_include_directory() {
        let ic = IgnoreController::new("/workspace", IgnoreMode::Builtin);
        assert!(!ic.should_include_directory(
            "node_modules",
            "/workspace/node_modules",
            false,
            false
        ));
        assert!(ic.should_include_directory("src", "/workspace/src", false, false));
    }

    #[test]
    fn test_critical_dirs_always_blocked() {
        let ic = IgnoreController::new("/workspace", IgnoreMode::Builtin);
        assert!(!ic.should_include_directory(".git", "/workspace/.git", true, false));
    }
}
