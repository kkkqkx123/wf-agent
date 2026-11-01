use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_not_empty;

use wf_types::decision_profile::DecisionProfile;

pub fn validate_decision_profile(profile: &DecisionProfile) -> ConfigResult<()> {
    validate_not_empty(&profile.id, "id")?;
    validate_not_empty(&profile.name, "name")?;
    validate_not_empty(&profile.protocol, "protocol")?;
    let normalized = profile.protocol.to_lowercase();
    if !matches!(
        normalized.as_str(),
        "openai" | "jev" | "typesafe" | "systemone"
    ) {
        return Err(crate::error::ConfigError::Validation(format!(
            "decision profile '{}' has unknown protocol '{}'; expected openai or jev",
            profile.id, profile.protocol
        )));
    }
    validate_not_empty(&profile.base_url, "base_url")?;
    validate_not_empty(&profile.model, "model")?;
    if let Some(timeout) = profile.timeout_secs {
        if timeout == 0 {
            return Err(crate::error::ConfigError::Validation(format!(
                "decision profile '{}' has timeout_secs 0; it must be >= 1",
                profile.id
            )));
        }
    }
    Ok(())
}

pub fn transform_decision_profile(
    profile: &DecisionProfile,
    parameters: &HashMap<String, String>,
) -> ConfigResult<DecisionProfile> {
    let mut cloned = profile.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_decision_profile(profile: DecisionProfile) -> DecisionProfile {
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile() -> DecisionProfile {
        DecisionProfile {
            id: "decide-default".to_string(),
            name: "Default decision".to_string(),
            description: None,
            protocol: "openai".to_string(),
            base_url: "https://api.openai.com/v1/decisions".to_string(),
            model: "gpt-6-luna".to_string(),
            api_key: None,
            timeout_secs: None,
            headers: HashMap::new(),
            query_params: HashMap::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn valid_profile_passes() {
        assert!(validate_decision_profile(&make_profile()).is_ok());
    }

    #[test]
    fn unknown_protocol_fails() {
        let mut profile = make_profile();
        profile.protocol = "chat".to_string();
        assert!(validate_decision_profile(&profile).is_err());
    }

    #[test]
    fn missing_connection_fails() {
        let mut profile = make_profile();
        profile.base_url = String::new();
        assert!(validate_decision_profile(&profile).is_err());
    }
}
