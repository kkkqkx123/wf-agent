use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestion {
    pub index: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<FollowupQuestionOption>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionOption {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionRequestData {
    pub questions: Vec<FollowupQuestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_info_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionResponseData {
    pub answers: Vec<FollowupAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupAnswer {
    pub question_index: u32,
    pub selected_value: Option<String>,
    pub custom_input: Option<String>,
}
