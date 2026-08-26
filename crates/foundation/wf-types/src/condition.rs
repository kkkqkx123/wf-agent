use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpressionCondition {
    pub r#type: String,
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PredicateType {
    IsEmpty,
    IsNotEmpty,
    IsNull,
    IsNotNull,
    IsTrue,
    IsFalse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredicateCondition {
    pub r#type: String,
    pub predicate_type: PredicateType,
    pub variable: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptCondition {
    pub r#type: String,
    pub script: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaCondition {
    pub r#type: String,
    pub variable: String,
    pub schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Condition {
    Expression(ExpressionCondition),
    Predicate(PredicateCondition),
    Script(ScriptCondition),
    Schema(SchemaCondition),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationContext {
    pub variables: serde_json::Value,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
}
