use wf_types::config::presets::{ContextCompressionPresetConfig, PredefinedPromptsPresetConfig, PredefinedToolsPresetConfig, PresetsConfig};

pub fn merge_presets_with_defaults(user: &PresetsConfig) -> PresetsConfig {
    PresetsConfig {
        context_compression: user.context_compression.as_ref().map(|c| ContextCompressionPresetConfig {
            enabled: c.enabled.or(Some(true)),
            threshold: c.threshold.or(Some(0.7)),
            max_tokens: c.max_tokens.or(Some(4096)),
            strategy: c.strategy.clone().or(Some("sliding_window".to_string())),
        }),
        predefined_tools: user.predefined_tools.as_ref().map(|t| PredefinedToolsPresetConfig {
            enabled: t.enabled.or(Some(true)),
            tools: t.tools.clone(),
        }),
        predefined_prompts: user.predefined_prompts.as_ref().map(|p| PredefinedPromptsPresetConfig {
            enabled: p.enabled.or(Some(true)),
            prompts: p.prompts.clone(),
        }),
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
}
