use wf_types::agent::config::{AgentActionPolicy, AgentConfig, AgentDecisionPolicy};

pub fn resolve_action_policy(config: Option<&AgentConfig>) -> AgentActionPolicy {
    config
        .and_then(|c| c.action_policy)
        .unwrap_or(AgentActionPolicy::Chat)
}

pub fn decision_policy_for(
    config: Option<&AgentConfig>,
) -> Result<&AgentDecisionPolicy, crate::error::AgentError> {
    let policy = config.and_then(|c| c.decision_policy.as_ref());
    match (resolve_action_policy(config), policy) {
        (AgentActionPolicy::Chat, _) => Err(crate::error::AgentError::Validation(
            "decision policy requested while action_policy is chat".to_string(),
        )),
        (AgentActionPolicy::Decision, Some(policy)) => Ok(policy),
        (AgentActionPolicy::Decision, None) => Err(crate::error::AgentError::Validation(
            "action_policy 'decision' requires decision_policy".to_string(),
        )),
    }
}

pub fn build_tool_choice_request(
    observation: &str,
    tool_names: &[String],
    tool_descriptions: &std::collections::HashMap<String, String>,
    done_description: &str,
    model: Option<String>,
    safety_identifier: Option<String>,
) -> wf_llm::DecisionRequest {
    let mut choices = Vec::with_capacity(tool_names.len() + 1);
    for name in tool_names {
        choices.push(wf_llm::llm_decision::types::ChoiceOption {
            value: wf_llm::ChoiceValue::Text(name.clone()),
            description: tool_descriptions
                .get(name)
                .map(|d| serde_json::Value::String(d.clone())),
        });
    }
    choices.push(wf_llm::llm_decision::types::ChoiceOption {
        value: wf_llm::ChoiceValue::Text("done".to_string()),
        description: Some(serde_json::Value::String(done_description.to_string())),
    });
    wf_llm::DecisionRequest {
        model,
        state: wf_llm::DecisionState::Text(observation.to_string()),
        questions: vec![wf_llm::NamedQuestion {
            name: Some("next_action".to_string()),
            question: wf_llm::DecisionQuestion::Choice {
                name: Some("next_action".to_string()),
                instructions: Some(serde_json::Value::String(
                    "Select the tool that best advances the task, or done if the task is complete."
                        .to_string(),
                )),
                choices,
            },
        }],
        safety_identifier,
        assets: None,
    }
}

pub fn select_tool_choice(
    response: &wf_llm::DecisionResponse,
    tool_names: &[String],
) -> Result<Option<String>, crate::error::AgentError> {
    let invalid = || {
        crate::error::AgentError::Validation(
            "decision response did not carry a usable next_action choice".to_string(),
        )
    };
    if response.answers.len() != 1 {
        return Err(invalid());
    }
    let answer = &response.answers[0];
    if answer.name.as_deref() != Some("next_action") {
        return Err(invalid());
    }
    match &answer.answer {
        wf_llm::DecisionAnswer::Refusal { .. } => Ok(None),
        wf_llm::DecisionAnswer::Choice { choice, .. } => {
            let selected = match choice {
                wf_llm::ChoiceValue::Text(text) => text.clone(),
                wf_llm::ChoiceValue::Bool(value) => value.to_string(),
            };
            if selected == "done" {
                return Ok(None);
            }
            if tool_names.iter().any(|name| name == &selected) {
                Ok(Some(selected))
            } else {
                Err(invalid())
            }
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice_response(choice: &str) -> wf_llm::DecisionResponse {
        wf_llm::DecisionResponse {
            model: "decision-test".to_string(),
            answers: vec![wf_llm::NamedAnswer {
                name: Some("next_action".to_string()),
                answer: wf_llm::DecisionAnswer::Choice {
                    name: Some("next_action".to_string()),
                    choice: wf_llm::ChoiceValue::Text(choice.to_string()),
                    confidence: 0.9,
                    probabilities: Vec::new(),
                },
            }],
            usage: wf_llm::DecisionUsage::default(),
        }
    }

    #[test]
    fn default_policy_is_chat() {
        assert_eq!(resolve_action_policy(None), AgentActionPolicy::Chat);
    }

    #[test]
    fn tool_choice_request_lists_done_last() {
        let request = build_tool_choice_request(
            "observe",
            &["search".to_string()],
            &std::collections::HashMap::from([(
                "search".to_string(),
                "Search the index".to_string(),
            )]),
            "Nothing left to do.",
            None,
            None,
        );
        assert_eq!(request.questions.len(), 1);
        let wf_llm::DecisionQuestion::Choice { choices, .. } = &request.questions[0].question
        else {
            panic!("expected a choice question");
        };
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn select_tool_choice_maps_done_to_none() {
        let tools = vec!["search".to_string()];
        assert_eq!(
            select_tool_choice(&choice_response("done"), &tools).unwrap(),
            None
        );
        assert_eq!(
            select_tool_choice(&choice_response("search"), &tools).unwrap(),
            Some("search".to_string())
        );
        assert!(select_tool_choice(&choice_response("unknown"), &tools).is_err());
    }
}
