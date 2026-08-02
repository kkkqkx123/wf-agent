#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionMode {
    Interactive,
    Headless,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Text,
    Json,
    Silent,
}

#[derive(Debug, Clone)]
pub struct ModeInfo {
    pub mode: ExecutionMode,
    pub output_format: OutputFormat,
    pub color_enabled: bool,
}

impl ModeInfo {
    pub fn is_headless(&self) -> bool {
        self.mode == ExecutionMode::Headless
    }

    pub fn is_interactive(&self) -> bool {
        self.mode == ExecutionMode::Interactive
    }

    pub fn is_test(&self) -> bool {
        self.mode == ExecutionMode::Test
    }

    pub fn is_json_mode(&self) -> bool {
        self.output_format == OutputFormat::Json
    }

    pub fn is_silent_mode(&self) -> bool {
        self.output_format == OutputFormat::Silent
    }
}

const ENV_CLI_MODE: &str = "CLI_MODE";
const ENV_HEADLESS: &str = "HEADLESS";
const ENV_TEST_MODE: &str = "TEST_MODE";
const ENV_OUTPUT_FORMAT: &str = "CLI_OUTPUT_FORMAT";
const ENV_NO_COLOR: &str = "NO_COLOR";

/// Serializes access to the mode-related environment variables. Detection
/// reads process-global env state; concurrent test setups mutate the same
/// vars from other threads, so a reader could observe a torn state. Every
/// public detection function takes the lock, and tests hold it across their
/// set/assert/clear sequences while calling the `*_inner` variants.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    // A panicked test thread poisons the mutex; recovering keeps the
    // remaining tests runnable instead of deadlocking on the next acquire.
    ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn detect_mode(config_fallback: Option<ExecutionMode>) -> ExecutionMode {
    let _guard = env_lock();
    detect_mode_inner(config_fallback)
}

fn detect_mode_inner(config_fallback: Option<ExecutionMode>) -> ExecutionMode {
    let cli_mode = std::env::var(ENV_CLI_MODE).ok();

    if let Some(ref mode) = cli_mode {
        match mode.as_str() {
            "test" => return ExecutionMode::Test,
            "headless" | "programmatic" => return ExecutionMode::Headless,
            _ => {}
        }
    }

    if std::env::var(ENV_TEST_MODE).as_deref() == Ok("true") {
        return ExecutionMode::Test;
    }

    if std::env::var(ENV_HEADLESS).as_deref() == Ok("true") {
        return ExecutionMode::Headless;
    }

    match config_fallback {
        Some(ExecutionMode::Headless) | Some(ExecutionMode::Test) => config_fallback.unwrap(),
        _ => ExecutionMode::Interactive,
    }
}

pub fn detect_output_format(mode: ExecutionMode) -> OutputFormat {
    let _guard = env_lock();
    detect_output_format_inner(mode)
}

fn detect_output_format_inner(mode: ExecutionMode) -> OutputFormat {
    if let Ok(format) = std::env::var(ENV_OUTPUT_FORMAT) {
        match format.as_str() {
            "json" => return OutputFormat::Json,
            "silent" => return OutputFormat::Silent,
            "text" => return OutputFormat::Text,
            _ => {}
        }
    }

    match mode {
        ExecutionMode::Headless => OutputFormat::Json,
        ExecutionMode::Test => OutputFormat::Text,
        ExecutionMode::Interactive => OutputFormat::Text,
    }
}

pub fn detect_color_enabled() -> bool {
    let _guard = env_lock();
    detect_color_enabled_inner()
}

fn detect_color_enabled_inner() -> bool {
    if std::env::var_os(ENV_NO_COLOR).is_some() {
        return false;
    }
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

pub fn detect_all(config_fallback: Option<ExecutionMode>) -> ModeInfo {
    let _guard = env_lock();
    detect_all_inner(config_fallback)
}

fn detect_all_inner(config_fallback: Option<ExecutionMode>) -> ModeInfo {
    let mode = detect_mode_inner(config_fallback);
    let output_format = detect_output_format_inner(mode);
    let color_enabled = detect_color_enabled_inner();

    ModeInfo {
        mode,
        output_format,
        color_enabled,
    }
}

pub fn is_headless() -> bool {
    detect_mode(None) == ExecutionMode::Headless
}

pub fn is_interactive() -> bool {
    detect_mode(None) == ExecutionMode::Interactive
}

pub fn is_test() -> bool {
    detect_mode(None) == ExecutionMode::Test
}

pub fn is_json_mode() -> bool {
    detect_output_format(detect_mode(None)) == OutputFormat::Json
}

pub fn is_silent_mode() -> bool {
    detect_output_format(detect_mode(None)) == OutputFormat::Silent
}

pub fn is_color_enabled() -> bool {
    detect_color_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the env mutex and clears every mode-related variable. Tests run
    /// in parallel threads within one process, so they must not mutate the
    /// process-global env without holding the lock (see `ENV_LOCK`).
    fn with_clean_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = env_lock();
        clear_env_vars();
        let result = f();
        clear_env_vars();
        result
    }

    fn clear_env_vars() {
        std::env::remove_var(ENV_CLI_MODE);
        std::env::remove_var(ENV_HEADLESS);
        std::env::remove_var(ENV_TEST_MODE);
        std::env::remove_var(ENV_OUTPUT_FORMAT);
        std::env::remove_var(ENV_NO_COLOR);
    }

    #[test]
    fn test_detect_mode_cli_mode_test() {
        with_clean_env(|| {
            std::env::set_var(ENV_CLI_MODE, "test");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Test);
        });
    }

    #[test]
    fn test_detect_mode_cli_mode_headless() {
        with_clean_env(|| {
            std::env::set_var(ENV_CLI_MODE, "headless");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Headless);
        });
    }

    #[test]
    fn test_detect_mode_cli_mode_programmatic() {
        with_clean_env(|| {
            std::env::set_var(ENV_CLI_MODE, "programmatic");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Headless);
        });
    }

    #[test]
    fn test_detect_mode_test_env() {
        with_clean_env(|| {
            std::env::set_var(ENV_TEST_MODE, "true");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Test);
        });
    }

    #[test]
    fn test_detect_mode_headless_env() {
        with_clean_env(|| {
            std::env::set_var(ENV_HEADLESS, "true");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Headless);
        });
    }

    #[test]
    fn test_detect_mode_config_fallback() {
        with_clean_env(|| {
            assert_eq!(
                detect_mode_inner(Some(ExecutionMode::Headless)),
                ExecutionMode::Headless
            );
            assert_eq!(
                detect_mode_inner(Some(ExecutionMode::Test)),
                ExecutionMode::Test
            );
            assert_eq!(
                detect_mode_inner(Some(ExecutionMode::Interactive)),
                ExecutionMode::Interactive
            );
        });
    }

    #[test]
    fn test_detect_mode_default_interactive() {
        with_clean_env(|| {
            assert_eq!(detect_mode_inner(None), ExecutionMode::Interactive);
        });
    }

    #[test]
    fn test_cli_mode_overrides_test_env() {
        with_clean_env(|| {
            std::env::set_var(ENV_TEST_MODE, "true");
            std::env::set_var(ENV_CLI_MODE, "headless");
            assert_eq!(detect_mode_inner(None), ExecutionMode::Headless);
        });
    }

    #[test]
    fn test_detect_output_format_env() {
        with_clean_env(|| {
            std::env::set_var(ENV_OUTPUT_FORMAT, "json");
            assert_eq!(
                detect_output_format_inner(ExecutionMode::Interactive),
                OutputFormat::Json
            );
        });
    }

    #[test]
    fn test_detect_output_format_headless_default() {
        with_clean_env(|| {
            assert_eq!(
                detect_output_format_inner(ExecutionMode::Headless),
                OutputFormat::Json
            );
        });
    }

    #[test]
    fn test_detect_output_format_test_default() {
        with_clean_env(|| {
            assert_eq!(
                detect_output_format_inner(ExecutionMode::Test),
                OutputFormat::Text
            );
        });
    }

    #[test]
    fn test_detect_output_format_interactive_default() {
        with_clean_env(|| {
            assert_eq!(
                detect_output_format_inner(ExecutionMode::Interactive),
                OutputFormat::Text
            );
        });
    }

    #[test]
    fn test_detect_all() {
        with_clean_env(|| {
            std::env::set_var(ENV_CLI_MODE, "headless");
            let info = detect_all_inner(None);
            assert_eq!(info.mode, ExecutionMode::Headless);
            assert_eq!(info.output_format, OutputFormat::Json);
            assert!(info.is_headless());
            assert!(!info.is_interactive());
            assert!(info.is_json_mode());
        });
    }

    #[test]
    fn test_no_color_disables_color() {
        with_clean_env(|| {
            std::env::set_var(ENV_NO_COLOR, "");
            assert!(!detect_color_enabled_inner());
        });
    }

    #[test]
    fn test_mode_info_helpers() {
        let info = ModeInfo {
            mode: ExecutionMode::Test,
            output_format: OutputFormat::Text,
            color_enabled: false,
        };
        assert!(info.is_test());
        assert!(!info.is_headless());
        assert!(!info.is_interactive());
        assert!(!info.is_json_mode());
        assert!(!info.is_silent_mode());
    }
}
