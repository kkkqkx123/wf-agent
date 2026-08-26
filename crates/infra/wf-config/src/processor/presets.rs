use wf_types::config::presets::{
    ContextCompressionPresetConfig, PredefinedPromptsPresetConfig, PredefinedToolsPresetConfig,
    PresetsConfig,
};

use crate::processor::infrastructure::RuntimeEnvironment;

pub fn merge_presets_with_defaults(user: &PresetsConfig) -> PresetsConfig {
    PresetsConfig {
        context_compression: user.context_compression.as_ref().map(|c| {
            ContextCompressionPresetConfig {
                enabled: c.enabled.or(Some(true)),
                threshold: c.threshold.or(Some(0.7)),
                max_tokens: c.max_tokens.or(Some(4096)),
                strategy: c.strategy.clone().or(Some("sliding_window".to_string())),
            }
        }),
        predefined_tools: user
            .predefined_tools
            .as_ref()
            .map(|t| PredefinedToolsPresetConfig {
                enabled: t.enabled.or(Some(true)),
                tools: t.tools.clone(),
            }),
        predefined_prompts: user.predefined_prompts.as_ref().map(|p| {
            PredefinedPromptsPresetConfig {
                enabled: p.enabled.or(Some(true)),
                prompts: p.prompts.clone(),
            }
        }),
    }
}

/// Validate a presets config: a context-compression threshold must be in
/// `(0, 1)` and `max_tokens` must be positive.
pub fn validate_presets_config(user: &PresetsConfig) -> crate::ConfigResult<()> {
    if let Some(cc) = &user.context_compression {
        if let Some(threshold) = cc.threshold {
            if !(0.0..=1.0).contains(&threshold) {
                return Err(crate::ConfigError::Validation(format!(
                    "presets.context_compression.threshold must be in (0, 1], got {threshold}"
                )));
            }
        }
        if let Some(max_tokens) = cc.max_tokens {
            if max_tokens == 0 {
                return Err(crate::ConfigError::Validation(
                    "presets.context_compression.max_tokens must be positive".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Transform a presets config by validating it and merging defaults.
pub fn transform_presets_config(user: PresetsConfig) -> crate::ConfigResult<PresetsConfig> {
    validate_presets_config(&user)?;
    Ok(merge_presets_with_defaults(&user))
}

/// Environment-specific default presets config.
pub fn get_presets_environment_defaults(env: RuntimeEnvironment) -> PresetsConfig {
    match env {
        RuntimeEnvironment::Development => PresetsConfig {
            context_compression: Some(ContextCompressionPresetConfig {
                enabled: Some(false),
                threshold: Some(0.7),
                max_tokens: Some(4096),
                strategy: Some("sliding_window".to_string()),
            }),
            predefined_tools: None,
            predefined_prompts: None,
        },
        RuntimeEnvironment::Production => PresetsConfig {
            context_compression: Some(ContextCompressionPresetConfig {
                enabled: Some(true),
                threshold: Some(0.7),
                max_tokens: Some(8192),
                strategy: Some("sliding_window".to_string()),
            }),
            predefined_tools: Some(PredefinedToolsPresetConfig {
                enabled: Some(true),
                tools: Some(Vec::new()),
            }),
            predefined_prompts: Some(PredefinedPromptsPresetConfig {
                enabled: Some(true),
                prompts: Some(Vec::new()),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_presets_with_defaults() {
        let user = PresetsConfig {
            context_compression: Some(ContextCompressionPresetConfig {
                enabled: Some(true),
                threshold: None,
                max_tokens: None,
                strategy: None,
            }),
            predefined_tools: None,
            predefined_prompts: None,
        };
        let merged = merge_presets_with_defaults(&user);
        let cc = merged.context_compression.unwrap();
        assert_eq!(cc.enabled, Some(true));
        assert_eq!(cc.threshold, Some(0.7));
        assert_eq!(cc.max_tokens, Some(4096));
        assert_eq!(cc.strategy, Some("sliding_window".to_string()));
    }

    #[test]
    fn test_validate_transform_and_environment_defaults() {
        use crate::processor::infrastructure::RuntimeEnvironment;

        let bad = PresetsConfig {
            context_compression: Some(ContextCompressionPresetConfig {
                enabled: Some(true),
                threshold: Some(1.5),
                max_tokens: None,
                strategy: None,
            }),
            predefined_tools: None,
            predefined_prompts: None,
        };
        assert!(validate_presets_config(&bad).is_err());
        assert!(transform_presets_config(bad).is_err());

        let good = PresetsConfig {
            context_compression: Some(ContextCompressionPresetConfig {
                enabled: Some(true),
                threshold: Some(0.5),
                max_tokens: Some(2048),
                strategy: None,
            }),
            predefined_tools: None,
            predefined_prompts: None,
        };
        let transformed = transform_presets_config(good).unwrap();
        let cc = transformed.context_compression.unwrap();
        assert_eq!(cc.threshold, Some(0.5));
        assert_eq!(cc.strategy, Some("sliding_window".to_string()));

        let dev = get_presets_environment_defaults(RuntimeEnvironment::Development);
        assert_eq!(
            dev.context_compression.as_ref().unwrap().enabled,
            Some(false)
        );
        let prod = get_presets_environment_defaults(RuntimeEnvironment::Production);
        assert_eq!(
            prod.context_compression.as_ref().unwrap().enabled,
            Some(true)
        );
        assert!(prod.predefined_tools.is_some());
    }
}
