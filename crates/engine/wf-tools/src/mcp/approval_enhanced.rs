//! Enhanced MCP approval: parameter-level rules, rate limiting and
//! user/role-based access control layered on top of the basic approval logic
//! in `crate::approval`.

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Operation type checked by access control rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpOperation {
    ToolCall,
    ResourceRead,
}

impl McpOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            McpOperation::ToolCall => "tool_call",
            McpOperation::ResourceRead => "resource_read",
        }
    }
}

/// Per-tool parameter approval rule.
#[derive(Debug, Clone, Default)]
pub struct ParameterApprovalRule {
    pub param_name: String,
    /// Allowed values; each entry is tried as a regex first, then as a
    /// literal string.
    pub allowed_values: Vec<String>,
    /// Denied values; each entry is tried as a regex first, then as a
    /// literal string.
    pub denied_values: Vec<String>,
}

/// Rate limiting rule applied per tool (global or per user).
#[derive(Debug, Clone)]
pub struct RateLimitingRule {
    pub max_calls: u32,
    pub window_ms: u64,
    /// Empty = applies globally.
    pub user_id: Option<String>,
}

/// User/role based access control rule.
#[derive(Debug, Clone, Default)]
pub struct AccessControlRule {
    /// `"*"` matches any user.
    pub user_id: String,
    pub allowed_servers: Vec<String>,
    pub denied_servers: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub allowed_operations: Vec<McpOperation>,
}

/// Risk level of an approval decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRiskLevel {
    Low,
    Medium,
    High,
}

impl ApprovalRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalRiskLevel::Low => "low",
            ApprovalRiskLevel::Medium => "medium",
            ApprovalRiskLevel::High => "high",
        }
    }
}

/// Result of an approval check.
#[derive(Debug, Clone)]
pub struct ApprovalResult {
    pub approved: bool,
    pub reason: String,
    pub risk_level: ApprovalRiskLevel,
    pub metadata: Option<Value>,
}

impl ApprovalResult {
    fn ok(reason: impl Into<String>) -> Self {
        Self {
            approved: true,
            reason: reason.into(),
            risk_level: ApprovalRiskLevel::Low,
            metadata: None,
        }
    }

    fn deny(reason: impl Into<String>, risk_level: ApprovalRiskLevel) -> Self {
        Self {
            approved: false,
            reason: reason.into(),
            risk_level,
            metadata: None,
        }
    }
}

/// Context for a tool call approval check.
#[derive(Debug, Clone, Default)]
pub struct ToolCallApprovalContext {
    pub server_name: String,
    pub tool_name: String,
    pub arguments: Option<Value>,
    pub user_id: Option<String>,
    pub timestamp: Option<i64>,
}

/// Context for a resource access approval check.
#[derive(Debug, Clone, Default)]
pub struct ResourceAccessApprovalContext {
    pub server_name: String,
    pub resource_uri: String,
    pub user_id: Option<String>,
    pub timestamp: Option<i64>,
}

/// Statistics about configured rules and call history.
#[derive(Debug, Clone, Default)]
pub struct ApprovalStatistics {
    pub parameter_rules: usize,
    pub rate_limiting_rules: usize,
    pub access_control_rules: usize,
    pub call_history_size: usize,
}

/// Enhanced MCP approval system.
pub struct EnhancedMcpApprovalSystem {
    parameter_rules: HashMap<String, Vec<ParameterApprovalRule>>,
    rate_limiting_rules: Vec<RateLimitingRule>,
    access_control_rules: Vec<AccessControlRule>,
    call_history: Mutex<HashMap<String, Vec<(i64, u32)>>>,
}

impl Default for EnhancedMcpApprovalSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl EnhancedMcpApprovalSystem {
    pub fn new() -> Self {
        Self {
            parameter_rules: HashMap::new(),
            rate_limiting_rules: Vec::new(),
            access_control_rules: Vec::new(),
            call_history: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_parameter_rule(&mut self, tool_id: &str, rule: ParameterApprovalRule) {
        self.parameter_rules
            .entry(tool_id.to_string())
            .or_default()
            .push(rule);
    }

    pub fn add_rate_limiting_rule(&mut self, rule: RateLimitingRule) {
        self.rate_limiting_rules.push(rule);
    }

    pub fn add_access_control_rule(&mut self, rule: AccessControlRule) {
        self.access_control_rules.push(rule);
    }

    /// Compose the tool id key: `{server}/{tool}`.
    pub fn tool_id(server_name: &str, tool_name: &str) -> String {
        format!("{}/{}", server_name, tool_name)
    }

    /// Check whether a tool call is approved.
    pub fn check_tool_call_approval(&self, ctx: &ToolCallApprovalContext) -> ApprovalResult {
        let tool_id = Self::tool_id(&ctx.server_name, &ctx.tool_name);

        let access = self.check_access_control(
            &ctx.server_name,
            &ctx.tool_name,
            ctx.user_id.as_deref(),
            McpOperation::ToolCall,
        );
        if !access.approved {
            return access;
        }

        let rate = self.check_rate_limit(&tool_id, ctx.user_id.as_deref());
        if !rate.approved {
            return rate;
        }

        if let Some(arguments) = &ctx.arguments {
            let param = self.validate_parameters(&tool_id, arguments);
            if !param.approved {
                return param;
            }
        }

        ApprovalResult::ok("All approval checks passed")
    }

    /// Check whether a resource read is approved.
    pub fn check_resource_access_approval(
        &self,
        ctx: &ResourceAccessApprovalContext,
    ) -> ApprovalResult {
        let access = self.check_access_control(
            &ctx.server_name,
            "resource_read",
            ctx.user_id.as_deref(),
            McpOperation::ResourceRead,
        );
        if !access.approved {
            return access;
        }

        let rate = self.check_rate_limit(
            &Self::tool_id(&ctx.server_name, "resource"),
            ctx.user_id.as_deref(),
        );
        if !rate.approved {
            return rate;
        }

        ApprovalResult::ok("All approval checks passed")
    }

    pub fn get_statistics(&self) -> ApprovalStatistics {
        ApprovalStatistics {
            parameter_rules: self.parameter_rules.len(),
            rate_limiting_rules: self.rate_limiting_rules.len(),
            access_control_rules: self.access_control_rules.len(),
            call_history_size: wf_common::lock::lock_ok(self.call_history.lock()).len(),
        }
    }

    pub fn clear_call_history(&self) {
        wf_common::lock::lock_ok(self.call_history.lock()).clear();
    }

    fn validate_parameters(&self, tool_id: &str, arguments: &Value) -> ApprovalResult {
        let Some(rules) = self.parameter_rules.get(tool_id) else {
            return ApprovalResult::ok("No parameter restrictions");
        };

        for rule in rules {
            let value = arguments.get(&rule.param_name);

            if !rule.allowed_values.is_empty() {
                let Some(value) = value else {
                    return ApprovalResult::deny(
                        format!(
                            "Parameter '{}' is required by approval rule for {}",
                            rule.param_name, tool_id
                        ),
                        ApprovalRiskLevel::High,
                    );
                };
                let value_str = value_to_string(value);
                let is_allowed = rule.allowed_values.iter().any(|pattern| {
                    regex_match(pattern, &value_str).unwrap_or_else(|| pattern == &value_str)
                });
                if !is_allowed {
                    return ApprovalResult::deny(
                        format!(
                            "Parameter '{}' value not allowed: {}",
                            rule.param_name, value_str
                        ),
                        ApprovalRiskLevel::High,
                    );
                }
            }

            if !rule.denied_values.is_empty() {
                if let Some(value) = value {
                    let value_str = value_to_string(value);
                    let is_denied = rule.denied_values.iter().any(|pattern| {
                        regex_match(pattern, &value_str).unwrap_or_else(|| pattern == &value_str)
                    });
                    if is_denied {
                        return ApprovalResult::deny(
                            format!(
                                "Parameter '{}' value denied: {}",
                                rule.param_name, value_str
                            ),
                            ApprovalRiskLevel::High,
                        );
                    }
                }
            }
        }

        ApprovalResult::ok("All parameters passed validation")
    }

    fn check_rate_limit(&self, key: &str, user_id: Option<&str>) -> ApprovalResult {
        let now = wf_common::time::now();
        let history_key = match user_id {
            Some(user) => format!("{}:{}", user, key),
            None => format!("global:{}", key),
        };

        let mut history = wf_common::lock::lock_ok(self.call_history.lock());
        for rule in &self.rate_limiting_rules {
            if let Some(rule_user) = &rule.user_id {
                if Some(rule_user.as_str()) != user_id {
                    continue;
                }
            }

            let entries = history.entry(history_key.clone()).or_default();
            let cutoff = now - rule.window_ms as i64;
            entries.retain(|(ts, _)| *ts > cutoff);

            let recent_count: u32 = entries.iter().map(|(_, c)| *c).sum();
            if recent_count >= rule.max_calls {
                return ApprovalResult {
                    approved: false,
                    reason: format!(
                        "Rate limit exceeded: {} calls in {}ms window (limit {})",
                        recent_count, rule.window_ms, rule.max_calls
                    ),
                    risk_level: ApprovalRiskLevel::Medium,
                    metadata: Some(serde_json::json!({
                        "calls": recent_count,
                        "limit": rule.max_calls,
                        "window_ms": rule.window_ms,
                    })),
                };
            }

            if let Some(last) = entries.last_mut() {
                if last.0 == now {
                    last.1 += 1;
                } else {
                    entries.push((now, 1));
                }
            } else {
                entries.push((now, 1));
            }
        }

        ApprovalResult::ok("Rate limit check passed")
    }

    fn check_access_control(
        &self,
        server_name: &str,
        tool_name: &str,
        user_id: Option<&str>,
        operation: McpOperation,
    ) -> ApprovalResult {
        let applicable: Vec<&AccessControlRule> = self
            .access_control_rules
            .iter()
            .filter(|rule| {
                rule.user_id == "*"
                    || rule.user_id.is_empty()
                    || Some(rule.user_id.as_str()) == user_id
            })
            .collect();

        for rule in applicable {
            if !rule.allowed_operations.is_empty() && !rule.allowed_operations.contains(&operation)
            {
                return ApprovalResult::deny(
                    format!(
                        "Operation '{}' not allowed for user '{}'",
                        operation.as_str(),
                        user_id.unwrap_or("*")
                    ),
                    ApprovalRiskLevel::Medium,
                );
            }

            if !rule.allowed_servers.is_empty()
                && !rule.allowed_servers.contains(&server_name.to_string())
            {
                return ApprovalResult::deny(
                    format!(
                        "Server '{}' not in allowed list for user '{}'",
                        server_name,
                        user_id.unwrap_or("*")
                    ),
                    ApprovalRiskLevel::Medium,
                );
            }

            if rule.denied_servers.contains(&server_name.to_string()) {
                return ApprovalResult::deny(
                    format!(
                        "Server '{}' is denied for user '{}'",
                        server_name,
                        user_id.unwrap_or("*")
                    ),
                    ApprovalRiskLevel::Medium,
                );
            }

            if !rule.allowed_tools.is_empty()
                && !rule.allowed_tools.contains(&tool_name.to_string())
            {
                return ApprovalResult::deny(
                    format!(
                        "Tool '{}' not in allowed list for user '{}'",
                        tool_name,
                        user_id.unwrap_or("*")
                    ),
                    ApprovalRiskLevel::Medium,
                );
            }

            if rule.denied_tools.contains(&tool_name.to_string()) {
                return ApprovalResult::deny(
                    format!(
                        "Tool '{}' is denied for user '{}'",
                        tool_name,
                        user_id.unwrap_or("*")
                    ),
                    ApprovalRiskLevel::Medium,
                );
            }
        }

        ApprovalResult::ok("Access control check passed")
    }
}

fn value_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

/// Match a pattern against a value. Returns `None` when the pattern is not a
/// valid regex (caller falls back to literal equality).
fn regex_match(pattern: &str, value: &str) -> Option<bool> {
    match Regex::new(pattern) {
        Ok(re) => Some(re.is_match(value)),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_rule_allowed_and_denied() {
        let mut approval = EnhancedMcpApprovalSystem::new();
        approval.add_parameter_rule(
            "db/query",
            ParameterApprovalRule {
                param_name: "path".into(),
                allowed_values: vec!["^/data/".to_string()],
                denied_values: vec!["^/data/secret".to_string()],
            },
        );

        let ok = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "db".into(),
            tool_name: "query".into(),
            arguments: Some(serde_json::json!({ "path": "/data/users" })),
            user_id: None,
            timestamp: None,
        });
        assert!(
            ok.approved,
            "path under /data should be allowed: {}",
            ok.reason
        );

        let denied = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "db".into(),
            tool_name: "query".into(),
            arguments: Some(serde_json::json!({ "path": "/data/secret/x" })),
            user_id: None,
            timestamp: None,
        });
        assert!(!denied.approved);

        let outside = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "db".into(),
            tool_name: "query".into(),
            arguments: Some(serde_json::json!({ "path": "/etc/passwd" })),
            user_id: None,
            timestamp: None,
        });
        assert!(!outside.approved);
    }

    #[test]
    fn test_rate_limit_per_user() {
        let mut approval = EnhancedMcpApprovalSystem::new();
        approval.add_rate_limiting_rule(RateLimitingRule {
            max_calls: 2,
            window_ms: 60_000,
            user_id: Some("alice".into()),
        });

        for _ in 0..2 {
            let result = approval.check_tool_call_approval(&ToolCallApprovalContext {
                server_name: "s".into(),
                tool_name: "t".into(),
                arguments: None,
                user_id: Some("alice".into()),
                timestamp: None,
            });
            assert!(result.approved);
        }

        // Alice hits the limit...
        let third = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "s".into(),
            tool_name: "t".into(),
            arguments: None,
            user_id: Some("alice".into()),
            timestamp: None,
        });
        assert!(!third.approved);

        // ...but Bob is unaffected.
        let bob = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "s".into(),
            tool_name: "t".into(),
            arguments: None,
            user_id: Some("bob".into()),
            timestamp: None,
        });
        assert!(bob.approved);
    }

    #[test]
    fn test_access_control() {
        let mut approval = EnhancedMcpApprovalSystem::new();
        approval.add_access_control_rule(AccessControlRule {
            user_id: "alice".into(),
            allowed_servers: vec!["db".into()],
            denied_tools: vec!["drop".into()],
            allowed_operations: vec![McpOperation::ToolCall],
            ..Default::default()
        });

        let ok = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "db".into(),
            tool_name: "select".into(),
            arguments: None,
            user_id: Some("alice".into()),
            timestamp: None,
        });
        assert!(ok.approved);

        let denied_tool = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "db".into(),
            tool_name: "drop".into(),
            arguments: None,
            user_id: Some("alice".into()),
            timestamp: None,
        });
        assert!(!denied_tool.approved);

        let denied_server = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "web".into(),
            tool_name: "fetch".into(),
            arguments: None,
            user_id: Some("alice".into()),
            timestamp: None,
        });
        assert!(!denied_server.approved);

        // Bob has no rules -> allowed.
        let bob = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "web".into(),
            tool_name: "fetch".into(),
            arguments: None,
            user_id: Some("bob".into()),
            timestamp: None,
        });
        assert!(bob.approved);
    }

    #[test]
    fn test_resource_read_operation() {
        let mut approval = EnhancedMcpApprovalSystem::new();
        approval.add_access_control_rule(AccessControlRule {
            user_id: "*".into(),
            allowed_operations: vec![McpOperation::ResourceRead],
            ..Default::default()
        });

        let ok = approval.check_resource_access_approval(&ResourceAccessApprovalContext {
            server_name: "s".into(),
            resource_uri: "file:///etc/hosts".into(),
            user_id: Some("anyone".into()),
            timestamp: None,
        });
        assert!(ok.approved);

        let denied = approval.check_tool_call_approval(&ToolCallApprovalContext {
            server_name: "s".into(),
            tool_name: "exec".into(),
            arguments: None,
            user_id: Some("anyone".into()),
            timestamp: None,
        });
        assert!(
            !denied.approved,
            "tool_call should be denied for read-only users"
        );
    }

    #[test]
    fn test_statistics() {
        let mut approval = EnhancedMcpApprovalSystem::new();
        approval.add_parameter_rule("a/b", ParameterApprovalRule::default());
        approval.add_rate_limiting_rule(RateLimitingRule {
            max_calls: 1,
            window_ms: 1000,
            user_id: None,
        });
        approval.add_access_control_rule(AccessControlRule {
            user_id: "*".into(),
            ..Default::default()
        });
        let stats = approval.get_statistics();
        assert_eq!(stats.parameter_rules, 1);
        assert_eq!(stats.rate_limiting_rules, 1);
        assert_eq!(stats.access_control_rules, 1);
    }
}
