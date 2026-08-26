use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if_exists: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchRegisterOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if_exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_errors: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnregisterOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_references: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchUnregisterOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_references: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_errors: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_if_not_exists: Option<bool>,
}
