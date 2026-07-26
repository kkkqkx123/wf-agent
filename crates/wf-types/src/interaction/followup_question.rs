use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestion {
    pub question: String,
    pub options: Option<Vec<String>>,
    pub response: Option<String>,
}
