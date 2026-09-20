use std::path::{Path, PathBuf};

use wf_config::parser;

use crate::custom::types::{
    CustomPromptDefinition, CustomResources, CustomResourcesPresetConfig, CustomToolDefinition,
    CustomTriggerDefinition,
};

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    parser::parse_config_file(path).map_err(|e| format!("parse error in {}: {}", path.display(), e))
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
    #[derive(serde::Deserialize)]
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
    #[derive(serde::Deserialize)]
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
    #[derive(serde::Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::Builder;

    fn write_tmp(content: &str, ext: &str) -> tempfile::NamedTempFile {
        let mut file = Builder::new().suffix(ext).tempfile().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn loads_tools_from_json() {
        let file = write_tmp(
            r#"{"tools": [{"id": "t1", "type": "STATELESS", "description": "d",
                "schema": {"parameters": []}, "handler": {"type": "inline", "code": "x"}}]}"#,
            ".json",
        );
        let tools = load_custom_tools(file.path(), Path::new(".")).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "t1");
    }

    #[test]
    fn loads_tools_from_toml() {
        let file = write_tmp(
            r#"
[[tools]]
id = "t1"
type = "STATELESS"
description = "d"
[tools.schema]
parameters = []
[tools.handler]
type = "inline"
code = "x"
"#,
            ".toml",
        );
        let tools = load_custom_tools(file.path(), Path::new(".")).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "t1");
    }

    #[test]
    fn invalid_file_reports_error() {
        let file = write_tmp("not json at all", ".json");
        let err = load_custom_tools(file.path(), Path::new(".")).unwrap_err();
        assert!(err[0].contains("parse error"));
    }

    #[test]
    fn unsupported_format_rejected() {
        let file = write_tmp("{}", ".yaml");
        let err = load_custom_tools(file.path(), Path::new(".")).unwrap_err();
        assert!(err[0].contains("unsupported config format"));
    }
}
