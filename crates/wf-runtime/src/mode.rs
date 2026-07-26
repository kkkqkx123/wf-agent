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

pub fn detect_mode(config_fallback: Option<ExecutionMode>) -> ExecutionMode {
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
    if std::env::var_os(ENV_NO_COLOR).is_some() {
        return false;
    }
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

pub fn detect_all(config_fallback: Option<ExecutionMode>) -> ModeInfo {
    let mode = detect_mode(config_fallback);
    let output_format = detect_output_format(mode);
    let color_enabled = detect_color_enabled();

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

    fn clear_env_vars() {
        std::env::remove_var(ENV_CLI_MODE);
        std::env::remove_var(ENV_HEADLESS);
        std::env::remove_var(ENV_TEST_MODE);
        std::env::remove_var(ENV_OUTPUT_FORMAT);
        std::env::remove_var(ENV_NO_COLOR);
    }

    #[test]
    fn test_detect_mode_cli_mode_test() {
        clear_env_vars();
        std::env::set_var(ENV_CLI_MODE, "test");
        assert_eq!(detect_mode(None), ExecutionMode::Test);
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_cli_mode_headless() {
        clear_env_vars();
        std::env::set_var(ENV_CLI_MODE, "headless");
        assert_eq!(detect_mode(None), ExecutionMode::Headless);
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_cli_mode_programmatic() {
        clear_env_vars();
        std::env::set_var(ENV_CLI_MODE, "programmatic");
        assert_eq!(detect_mode(None), ExecutionMode::Headless);
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_test_env() {
        clear_env_vars();
        std::env::set_var(ENV_TEST_MODE, "true");
        assert_eq!(detect_mode(None), ExecutionMode::Test);
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_headless_env() {
        clear_env_vars();
        std::env::set_var(ENV_HEADLESS, "true");
        assert_eq!(detect_mode(None), ExecutionMode::Headless);
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_config_fallback() {
        clear_env_vars();
        assert_eq!(
            detect_mode(Some(ExecutionMode::Headless)),
            ExecutionMode::Headless
        );
        assert_eq!(detect_mode(Some(ExecutionMode::Test)), ExecutionMode::Test);
        assert_eq!(
            detect_mode(Some(ExecutionMode::Interactive)),
            ExecutionMode::Interactive
        );
        clear_env_vars();
    }

    #[test]
    fn test_detect_mode_default_interactive() {
        clear_env_vars();
        assert_eq!(detect_mode(None), ExecutionMode::Interactive);
        clear_env_vars();
    }

    #[test]
    fn test_cli_mode_overrides_test_env() {
        clear_env_vars();
        std::env::set_var(ENV_TEST_MODE, "true");
        std::env::set_var(ENV_CLI_MODE, "headless");
        assert_eq!(detect_mode(None), ExecutionMode::Headless);
        clear_env_vars();
    }

    #[test]
    fn test_detect_output_format_env() {
        clear_env_vars();
        std::env::set_var(ENV_OUTPUT_FORMAT, "json");
        assert_eq!(
            detect_output_format(ExecutionMode::Interactive),
            OutputFormat::Json
        );
        clear_env_vars();
    }

    #[test]
    fn test_detect_output_format_headless_default() {
        clear_env_vars();
        assert_eq!(
            detect_output_format(ExecutionMode::Headless),
            OutputFormat::Json
        );
        clear_env_vars();
    }

    #[test]
    fn test_detect_output_format_test_default() {
        clear_env_vars();
        assert_eq!(
            detect_output_format(ExecutionMode::Test),
            OutputFormat::Text
        );
        clear_env_vars();
    }

    #[test]
    fn test_detect_output_format_interactive_default() {
        clear_env_vars();
        assert_eq!(
            detect_output_format(ExecutionMode::Interactive),
            OutputFormat::Text
        );
        clear_env_vars();
    }

    #[test]
    fn test_detect_all() {
        clear_env_vars();
        std::env::set_var(ENV_CLI_MODE, "headless");
        let info = detect_all(None);
        assert_eq!(info.mode, ExecutionMode::Headless);
        assert_eq!(info.output_format, OutputFormat::Json);
        assert!(info.is_headless());
        assert!(!info.is_interactive());
        assert!(info.is_json_mode());
        clear_env_vars();
    }

    #[test]
    fn test_no_color_disables_color() {
        clear_env_vars();
        std::env::set_var(ENV_NO_COLOR, "");
        assert!(!detect_color_enabled());
        clear_env_vars();
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
