use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::error::{RuntimeError, RuntimeResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl LogLevel {
    pub fn parse(raw: &str) -> RuntimeResult<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" | "warning" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            "off" => Ok(Self::Off),
            other => Err(RuntimeError::Logger(format!("invalid log level: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Full,
    Compact,
    Json,
    Pretty,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogConfig {
    pub level: Option<LogLevel>,
    pub format: Option<LogFormat>,
}

impl LogConfig {
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = Some(format);
        self
    }
}

/// The tracing subscriber is process-global. Keeping the guard alive documents
/// that ownership; dropping it cannot uninstall the subscriber.
pub struct Guard {
    _inner: (),
}

pub fn init_tracing(config: &LogConfig) -> RuntimeResult<Guard> {
    let level = config.level.unwrap_or(LogLevel::Warn).as_str();
    let filter = EnvFilter::new(level);

    let fmt = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_ansi(false)
        .with_writer(std::io::stderr);

    let init_result = match config.format.unwrap_or(LogFormat::Full) {
        LogFormat::Full => tracing_subscriber::registry()
            .with(filter)
            .with(fmt)
            .try_init(),
        LogFormat::Compact => tracing_subscriber::registry()
            .with(filter)
            .with(fmt.compact())
            .try_init(),
        LogFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(fmt.json())
            .try_init(),
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(filter)
            .with(fmt.pretty())
            .try_init(),
    };

    // A second bootstrap keeps the first subscriber. There is no per-instance
    // filter to restore, so the result carries no configuration state.
    let _ = init_result;
    Ok(Guard { _inner: () })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default_is_unset() {
        let config = LogConfig::default();
        assert_eq!(config.level, None);
        assert_eq!(config.format, None);
    }

    #[test]
    fn test_log_config_builder() {
        let config = LogConfig::default()
            .with_level(LogLevel::Debug)
            .with_format(LogFormat::Json);

        assert_eq!(config.level, Some(LogLevel::Debug));
        assert_eq!(config.format, Some(LogFormat::Json));
    }

    #[test]
    fn test_parse_level_normalizes_spellings() {
        assert_eq!(LogLevel::parse(" DEBUG ").unwrap(), LogLevel::Debug);
        assert_eq!(LogLevel::parse("warning").unwrap(), LogLevel::Warn);
        assert_eq!(LogLevel::parse("OFF").unwrap(), LogLevel::Off);
    }

    #[test]
    fn test_parse_level_rejects_unknown() {
        assert!(LogLevel::parse("verbose").is_err());
        assert!(LogLevel::parse("").is_err());
    }
}
