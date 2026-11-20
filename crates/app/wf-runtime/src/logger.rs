use std::sync::OnceLock;

use tracing_subscriber::reload::{self, Handle};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

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

impl LogFormat {
    pub fn parse(raw: &str) -> RuntimeResult<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "compact" => Ok(Self::Compact),
            "json" => Ok(Self::Json),
            "pretty" => Ok(Self::Pretty),
            other => Err(RuntimeError::Logger(format!("invalid log format: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Json => "json",
            Self::Pretty => "pretty",
        }
    }
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

struct ActiveLogging {
    filter: Handle<EnvFilter, Registry>,
    format: LogFormat,
}

static ACTIVE_LOGGING: OnceLock<Result<ActiveLogging, String>> = OnceLock::new();

/// The tracing subscriber is process-global. Keeping the guard alive documents
/// that ownership; dropping it cannot uninstall the subscriber. The installed
/// filter is reloaded by a later `init_tracing` call.
pub struct Guard {
    _inner: (),
}

fn install_logging(format: LogFormat, filter: EnvFilter) -> Result<ActiveLogging, String> {
    let (filter_layer, handle) = reload::Layer::<EnvFilter, Registry>::new(filter);
    let fmt = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_ansi(false)
        .with_writer(std::io::stderr);

    let init_result = match format {
        LogFormat::Full => tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt)
            .try_init(),
        LogFormat::Compact => tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt.compact())
            .try_init(),
        LogFormat::Json => tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt.json())
            .try_init(),
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt.pretty())
            .try_init(),
    };
    init_result.map_err(|err| format!("tracing subscriber already installed: {err}"))?;

    Ok(ActiveLogging {
        filter: handle,
        format,
    })
}

pub fn init_tracing(config: &LogConfig) -> RuntimeResult<Guard> {
    let level = config.level.unwrap_or(LogLevel::Warn);
    let format = config.format.unwrap_or(LogFormat::Full);

    let active =
        ACTIVE_LOGGING.get_or_init(|| install_logging(format, EnvFilter::new(level.as_str())));
    let active = active
        .as_ref()
        .map_err(|err| RuntimeError::Logger(err.clone()))?;
    if active.format != format {
        return Err(RuntimeError::Logger(format!(
            "log format {} conflicts with the installed format {}",
            format.as_str(),
            active.format.as_str()
        )));
    }
    active
        .filter
        .reload(EnvFilter::new(level.as_str()))
        .map_err(|err| RuntimeError::Logger(format!("failed to reload log filter: {err}")))?;

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

    #[test]
    fn test_parse_format_normalizes_spellings() {
        assert_eq!(LogFormat::parse(" JSON ").unwrap(), LogFormat::Json);
        assert_eq!(LogFormat::parse("Compact").unwrap(), LogFormat::Compact);
        assert_eq!(LogFormat::parse("pretty").unwrap(), LogFormat::Pretty);
    }

    #[test]
    fn test_parse_format_rejects_unknown() {
        assert!(LogFormat::parse("verbose").is_err());
        assert!(LogFormat::parse("").is_err());
    }

    #[test]
    fn test_repeated_init_reloads_level() {
        let quiet = LogConfig::default().with_level(LogLevel::Off);
        let verbose = LogConfig::default().with_level(LogLevel::Debug);
        assert!(init_tracing(&quiet).is_ok());
        assert!(init_tracing(&verbose).is_ok());
        assert!(init_tracing(&quiet).is_ok());
    }
}
