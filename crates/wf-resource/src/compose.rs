use std::collections::HashMap;

use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_types::SystemPromptFragment;

#[derive(Debug, Clone)]
pub struct Config {
    pub fragment_ids: Vec<String>,
    pub separator: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub variables: Option<HashMap<String, String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("fragment not found: {0}")]
    FragmentNotFound(String),
}

pub fn compose(
    cfg: &Config,
    fragments: &ConcurrentRegistry<SystemPromptFragment>,
) -> Result<String, Error> {
    let sep = cfg.separator.as_deref().unwrap_or("\n\n");
    let mut parts: Vec<String> = Vec::with_capacity(cfg.fragment_ids.len());

    for id in &cfg.fragment_ids {
        let fragment = fragments
            .get(id)
            .ok_or_else(|| Error::FragmentNotFound(id.clone()))?;
        let content = if let Some(ref vars) = cfg.variables {
            apply_variables(&fragment.content, vars)
        } else {
            fragment.content.clone()
        };
        parts.push(content);
    }

    let body = parts.join(sep);
    let mut result = String::new();

    if let Some(ref prefix) = cfg.prefix {
        result.push_str(prefix);
        result.push('\n');
    }

    result.push_str(&body);

    if let Some(ref suffix) = cfg.suffix {
        result.push('\n');
        result.push_str(suffix);
    }

    Ok(result)
}

fn apply_variables(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::MutableRegistry;

    fn test_registry() -> ConcurrentRegistry<SystemPromptFragment> {
        let reg = ConcurrentRegistry::new();
        reg.register(
            "fragments.role.assistant".into(),
            std::sync::Arc::new(SystemPromptFragment {
                id: "fragments.role.assistant".into(),
                category: "role".into(),
                content: "You are {{name}}.".into(),
                description: None,
                variables: None,
            }),
        )
        .unwrap();
        reg.register(
            "fragments.constraint.general".into(),
            std::sync::Arc::new(SystemPromptFragment {
                id: "fragments.constraint.general".into(),
                category: "constraint".into(),
                content: "Be helpful.".into(),
                description: None,
                variables: None,
            }),
        )
        .unwrap();
        reg
    }

    #[test]
    fn test_compose_two_fragments() {
        let reg = test_registry();
        let cfg = Config {
            fragment_ids: vec![
                "fragments.role.assistant".into(),
                "fragments.constraint.general".into(),
            ],
            separator: Some("\n".into()),
            prefix: None,
            suffix: None,
            variables: Some(HashMap::from([("name".into(), "Bob".into())])),
        };
        let result = compose(&cfg, &reg).unwrap();
        assert_eq!(result, "You are Bob.\nBe helpful.");
    }

    #[test]
    fn test_compose_with_prefix_suffix() {
        let reg = test_registry();
        let cfg = Config {
            fragment_ids: vec!["fragments.constraint.general".into()],
            separator: None,
            prefix: Some("BEGIN".into()),
            suffix: Some("END".into()),
            variables: None,
        };
        let result = compose(&cfg, &reg).unwrap();
        assert_eq!(result, "BEGIN\nBe helpful.\nEND");
    }

    #[test]
    fn test_compose_fragment_not_found() {
        let reg = test_registry();
        let cfg = Config {
            fragment_ids: vec!["nonexistent".into()],
            separator: None,
            prefix: None,
            suffix: None,
            variables: None,
        };
        let err = compose(&cfg, &reg).unwrap_err();
        assert!(matches!(err, Error::FragmentNotFound(_)));
    }

    #[test]
    fn test_apply_variables_no_substitution() {
        let reg = test_registry();
        let cfg = Config {
            fragment_ids: vec!["fragments.constraint.general".into()],
            separator: None,
            prefix: None,
            suffix: None,
            variables: None,
        };
        let result = compose(&cfg, &reg).unwrap();
        assert_eq!(result, "Be helpful.");
    }
}
