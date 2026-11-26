use super::*;

fn eval_judge_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginEvalJudge>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginEvalJudge>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers an eval judge under `name`, replacing any prior entry.
pub fn register_eval_judge(name: &str, judge: Arc<dyn PluginEvalJudge>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = eval_judge_registry().lock() {
        guard.insert(trimmed.to_string(), judge);
    }
}

/// Removes an eval judge.
pub fn unregister_eval_judge(name: &str) {
    if let Ok(mut guard) = eval_judge_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds an eval judge by name, including built-in judges.
///
/// Built-in names (`builtin:`) resolve without any plugin and take precedence
/// only when no plugin registered the exact name explicitly.
pub fn find_eval_judge(name: &str) -> Option<Arc<dyn PluginEvalJudge>> {
    let trimmed = name.trim();
    if let Some(judge) = eval_judge_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(trimmed).cloned())
    {
        return Some(judge);
    }
    match trimmed {
        crate::builtin_judge::BUILTIN_HEURISTIC_JUDGE => {
            Some(Arc::new(crate::builtin_judge::HeuristicJudge))
        }
        _ => None,
    }
}

/// Clears eval judges; tests use it to isolate cases.
pub fn clear_eval_judges() {
    if let Ok(mut guard) = eval_judge_registry().lock() {
        guard.clear();
    }
}

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
    name: &str,
    query: &str,
    answer: &str,
    contexts: &[String],
) -> crate::error::Result<JudgeScore> {
    let Some(judge) = find_eval_judge(name) else {
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
