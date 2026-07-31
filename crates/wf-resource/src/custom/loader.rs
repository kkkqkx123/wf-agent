use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::custom::types::{
    CustomPromptDefinition, CustomResources, CustomResourcesPresetConfig, CustomToolDefinition,
    CustomTriggerDefinition,
};

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("parse error in {}: {}", path.display(), e))
}

fn resolve_path(path_str: &str, base_dir: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

pub fn load_custom_tools(
    path: &Path,
    _base_dir: &Path,
) -> Result<Vec<CustomToolDefinition>, Vec<String>> {
    #[derive(Deserialize)]
    struct ToolsFile {
        tools: Vec<CustomToolDefinition>,
    }

    match load_json::<ToolsFile>(path) {
        Ok(file) => Ok(file.tools),
        Err(e) => Err(vec![e]),
    }
}

pub fn load_custom_triggers(
    path: &Path,
    _base_dir: &Path,
) -> Result<Vec<CustomTriggerDefinition>, Vec<String>> {
    #[derive(Deserialize)]
    struct TriggersFile {
        triggers: Vec<CustomTriggerDefinition>,
    }

    match load_json::<TriggersFile>(path) {
        Ok(file) => Ok(file.triggers),
        Err(e) => Err(vec![e]),
    }
}

pub fn load_custom_prompts(
    path: &Path,
    _base_dir: &Path,
) -> Result<Vec<CustomPromptDefinition>, Vec<String>> {
    #[derive(Deserialize)]
    struct PromptsFile {
        prompts: Vec<CustomPromptDefinition>,
    }

    match load_json::<PromptsFile>(path) {
        Ok(file) => Ok(file.prompts),
        Err(e) => Err(vec![e]),
    }
}

pub fn load_custom_resources(
    config: &CustomResourcesPresetConfig,
    base_dir: &Path,
) -> CustomResources {
    let mut resources = CustomResources::default();
    let enabled = config.enabled.unwrap_or(true);
    if !enabled {
        return resources;
    }

    if let Some(ref tools_path) = config.tools_path {
        let path = resolve_path(tools_path, base_dir);
        if path.exists() {
            match load_custom_tools(&path, base_dir) {
                Ok(tools) => resources.tools = tools,
                Err(errors) => resources.errors.extend(errors),
            }
        }
    }

    if let Some(ref triggers_path) = config.triggers_path {
        let path = resolve_path(triggers_path, base_dir);
        if path.exists() {
            match load_custom_triggers(&path, base_dir) {
                Ok(triggers) => resources.triggers = triggers,
                Err(errors) => resources.errors.extend(errors),
            }
        }
    }

    if let Some(ref prompts_path) = config.prompts_path {
        let path = resolve_path(prompts_path, base_dir);
        if path.exists() {
            match load_custom_prompts(&path, base_dir) {
                Ok(prompts) => resources.prompts = prompts,
                Err(errors) => resources.errors.extend(errors),
            }
        }
    }

    resources
}
