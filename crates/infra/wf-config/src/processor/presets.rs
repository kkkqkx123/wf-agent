use wf_types::config::presets::{
    PredefinedPromptsPresetConfig, PredefinedToolsPresetConfig, PresetsConfig,
};

use crate::processor::infrastructure::RuntimeEnvironment;

pub fn merge_presets_with_defaults(user: &PresetsConfig) -> PresetsConfig {
    PresetsConfig {
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

/// Validate a presets config (currently a no-op: no constrained preset
/// fields remain).
pub fn validate_presets_config(_user: &PresetsConfig) -> crate::ConfigResult<()> {
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
            predefined_tools: None,
            predefined_prompts: None,
        },
        RuntimeEnvironment::Production => PresetsConfig {
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
            predefined_tools: None,
            predefined_prompts: None,
        };
        let merged = merge_presets_with_defaults(&user);
        assert!(merged.predefined_tools.is_none());
        assert!(merged.predefined_prompts.is_none());
    }

    #[test]
    fn test_validate_transform_and_environment_defaults() {
        use crate::processor::infrastructure::RuntimeEnvironment;

        let config = PresetsConfig {
            predefined_tools: None,
            predefined_prompts: None,
        };
        assert!(validate_presets_config(&config).is_ok());
        assert!(transform_presets_config(config).is_ok());

        let dev = get_presets_environment_defaults(RuntimeEnvironment::Development);
        assert!(dev.predefined_tools.is_none());
        let prod = get_presets_environment_defaults(RuntimeEnvironment::Production);
        assert!(prod.predefined_tools.is_some());
    }
}
