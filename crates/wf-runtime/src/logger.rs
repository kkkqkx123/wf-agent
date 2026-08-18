use std::path::PathBuf;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::error::{RuntimeError, RuntimeResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Full,
    Compact,
    Json,
    Pretty,
}

#[derive(Debug, Clone)]
pub enum LogOutput {
    Stdout,
    Stderr,
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub level: String,
    pub format: LogFormat,
    pub output: LogOutput,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "warn".to_string(),
            format: LogFormat::Full,
            output: LogOutput::Stdout,
        }
    }
}

impl LogConfig {
    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }

    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_output(mut self, output: LogOutput) -> Self {
        self.output = output;
        self
    }
}

pub struct Guard {
    _inner: (),
}

pub fn init_tracing(config: &LogConfig) -> RuntimeResult<Guard> {
    let env_filter = EnvFilter::try_new(&config.level)
        .or_else(|_| EnvFilter::try_new(format!("warn,{}", config.level)))
        .map_err(|e| RuntimeError::Logger(format!("Invalid log level filter: {}", e)))?;

    let fmt = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false);

    let init_result = match config.format {
        LogFormat::Full => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt)
            .try_init(),
        LogFormat::Compact => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt.compact())
            .try_init(),
        LogFormat::Json => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt.json())
            .try_init(),
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt.pretty())
            .try_init(),
    };

    match init_result {
        Ok(()) => Ok(Guard { _inner: () }),
        Err(e) if e.to_string().contains("already been set") => Ok(Guard { _inner: () }),
        Err(e) => Err(RuntimeError::Logger(format!(
            "Failed to install tracing subscriber: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.level, "warn");
        assert_eq!(config.format, LogFormat::Full);
        assert!(matches!(config.output, LogOutput::Stdout));
    }

    #[test]
    fn test_log_config_builder() {
        let config = LogConfig::default()
            .with_level("debug")
            .with_format(LogFormat::Json)
            .with_output(LogOutput::Stderr);

        assert_eq!(config.level, "debug");
        assert_eq!(config.format, LogFormat::Json);
        assert!(matches!(config.output, LogOutput::Stderr));
    }
}
