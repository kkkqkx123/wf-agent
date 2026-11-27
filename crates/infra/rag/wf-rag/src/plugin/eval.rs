use super::*;

/// One score from an eval judge.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JudgeScore {
    /// Zero-to-one score.
    pub score: f64,
    /// Optional reason supplied by the judge.
    pub reason: String,
}

/// Scores one answer through the named eval judge.
/// Unknown names and out-of-range scores fail so misconfiguration surfaces.
pub fn judge_with_registered(
    extensions: &RetrievalExtensions,
    name: &str,
    query: &str,
    answer: &str,
    contexts: &[String],
) -> crate::error::Result<JudgeScore> {
    let Some(judge) = extensions.find_eval_judge(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown eval judge '{name}'"
        )));
    };
    let input = serde_json::json!({
        "protocol_version": EVAL_JUDGE_PROTOCOL_VERSION,
        "query": query,
        "answer": answer,
        "contexts": contexts,
    });
    let output = judge
        .judge(input)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    let Some(score) = output.get("score").and_then(|value| value.as_f64()) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "eval judge '{name}' must return a numeric score"
        )));
    };
    if !(0.0..=1.0).contains(&score) || !score.is_finite() {
        return Err(crate::error::RagError::InvalidInput(format!(
            "eval judge '{name}' score must be within 0 and 1"
        )));
    }
    let reason = output
        .get("reason")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    Ok(JudgeScore { score, reason })
}
