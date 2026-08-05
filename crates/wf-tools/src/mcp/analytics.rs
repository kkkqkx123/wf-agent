//! MCP tool usage analytics.
//!
//! [`McpUsageAnalytics`] tracks per `server/tool` call statistics (call
//! counts, success/failure, latency min/avg/max, success rate, first/last
//! execution) with a bounded execution history. It mirrors the TS
//! `McpToolsUsageAnalytics` feature.

use std::collections::HashMap;
use std::sync::Mutex;

/// Default maximum number of execution history records kept in memory.
pub const DEFAULT_MAX_HISTORY: usize = 10_000;

/// Aggregate statistics for a single server/tool pair.
#[derive(Debug, Clone, Default)]
pub struct ToolStats {
    pub server_name: String,
    pub tool_name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub avg_execution_ms: f64,
    pub min_execution_ms: u64,
    pub max_execution_ms: u64,
    pub success_rate: f64,
    pub last_error: Option<String>,
    pub first_executed_at: Option<i64>,
    pub last_executed_at: Option<i64>,
    pub error_counts: HashMap<String, u64>,
    pub user_call_counts: HashMap<String, u64>,
}

impl ToolStats {
    fn new(server_name: &str, tool_name: &str) -> Self {
        Self {
            server_name: server_name.into(),
            tool_name: tool_name.into(),
            min_execution_ms: u64::MAX,
            max_execution_ms: 0,
            ..Default::default()
        }
    }
}

/// Per-server rollup statistics.
#[derive(Debug, Clone, Default)]
pub struct ServerStats {
    pub tool_count: u64,
    pub call_count: u64,
    pub success_count: u64,
}

impl ServerStats {
    pub fn success_rate(&self) -> f64 {
        if self.call_count == 0 {
            0.0
        } else {
            (self.success_count as f64 / self.call_count as f64) * 100.0
        }
    }
}

/// Overall analytics report.
#[derive(Debug, Clone, Default)]
pub struct AnalyticsReport {
    pub total_tools: usize,
    pub active_tools: usize,
    pub total_calls: u64,
    pub overall_success_rate: f64,
    pub hot_tools: Vec<ToolStats>,
    pub cold_tools: Vec<ToolStats>,
    pub problematic_tools: Vec<ToolStats>,
    pub server_stats: HashMap<String, ServerStats>,
    pub generated_at_ms: i64,
}

/// One execution record kept in the bounded history.
#[derive(Debug, Clone)]
struct ExecutionRecord {
    tool_id: String,
    execution_ms: u64,
    success: bool,
    error: Option<String>,
    user_id: Option<String>,
    timestamp: i64,
}

/// Tracks MCP tool usage statistics.
pub struct McpUsageAnalytics {
    stats: Mutex<HashMap<String, ToolStats>>,
    history: Mutex<Vec<ExecutionRecord>>,
    max_history_size: usize,
}

impl Default for McpUsageAnalytics {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HISTORY)
    }
}

impl McpUsageAnalytics {
    pub fn new(max_history_size: usize) -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            history: Mutex::new(Vec::new()),
            max_history_size,
        }
    }

    /// Compose the tool id (`server/tool`) used as the map key.
    pub fn tool_id(server_name: &str, tool_name: &str) -> String {
        format!("{}/{}", server_name, tool_name)
    }

    /// Record a tool execution.
    pub fn record_execution(
        &self,
        server_name: &str,
        tool_name: &str,
        execution_ms: u64,
        success: bool,
        error: Option<String>,
        user_id: Option<String>,
    ) {
        let tool_id = Self::tool_id(server_name, tool_name);
        let now = wf_common::time::now();

        let mut stats = self.stats.lock().unwrap();
        let entry = stats
            .entry(tool_id.clone())
            .or_insert_with(|| ToolStats::new(server_name, tool_name));

        entry.call_count += 1;
        if success {
            entry.success_count += 1;
        } else {
            entry.failure_count += 1;
        }

        entry.avg_execution_ms = (entry.avg_execution_ms * (entry.call_count - 1) as f64
            + execution_ms as f64)
            / entry.call_count as f64;
        entry.min_execution_ms = entry.min_execution_ms.min(execution_ms);
        entry.max_execution_ms = entry.max_execution_ms.max(execution_ms);
        entry.success_rate = (entry.success_count as f64 / entry.call_count as f64) * 100.0;

        if let Some(error) = error {
            *entry.error_counts.entry(error.clone()).or_insert(0) += 1;
            entry.last_error = Some(error);
        }
        if let Some(user_id) = &user_id {
            *entry.user_call_counts.entry(user_id.clone()).or_insert(0) += 1;
        }
        entry.last_executed_at = Some(now);
        if entry.first_executed_at.is_none() {
            entry.first_executed_at = Some(now);
        }
        let last_error = entry.last_error.clone();
        drop(stats);

        let mut history = self.history.lock().unwrap();
        history.push(ExecutionRecord {
            tool_id,
            execution_ms,
            success,
            error: last_error,
            user_id,
            timestamp: now,
        });
        if history.len() > self.max_history_size {
            let cut = history.len() - self.max_history_size;
            history.drain(0..cut);
        }
    }

    /// Statistics for one tool, if present.
    pub fn get_tool_stats(&self, server_name: &str, tool_name: &str) -> Option<ToolStats> {
        self.stats
            .lock()
            .unwrap()
            .get(&Self::tool_id(server_name, tool_name))
            .cloned()
    }

    /// All tool statistics.
    pub fn get_all_tool_stats(&self) -> Vec<ToolStats> {
        let mut values: Vec<ToolStats> = self.stats.lock().unwrap().values().cloned().collect();
        values.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
        values
    }

    /// Most frequently called tools.
    pub fn get_hot_tools(&self, limit: usize) -> Vec<ToolStats> {
        let mut values: Vec<ToolStats> = self.stats.lock().unwrap().values().cloned().collect();
        values.sort_by(|a, b| {
            b.call_count
                .cmp(&a.call_count)
                .then_with(|| a.tool_name.cmp(&b.tool_name))
        });
        values.truncate(limit);
        values
    }

    /// Least frequently called tools with at least one call.
    pub fn get_cold_tools(&self, limit: usize) -> Vec<ToolStats> {
        let mut values: Vec<ToolStats> = self
            .stats
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.call_count > 0)
            .cloned()
            .collect();
        values.sort_by_key(|t| t.call_count);
        values.truncate(limit);
        values
    }

    /// Tools with the lowest success rate (at least one call).
    pub fn get_problematic_tools(&self, limit: usize) -> Vec<ToolStats> {
        let mut values: Vec<ToolStats> = self
            .stats
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.call_count > 0)
            .cloned()
            .collect();
        values.sort_by(|a, b| {
            a.success_rate
                .partial_cmp(&b.success_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        values.truncate(limit);
        values
    }

    /// Rollup statistics, optionally filtered by server name.
    pub fn get_server_stats(&self, server_name: Option<&str>) -> HashMap<String, ServerStats> {
        let mut result: HashMap<String, ServerStats> = HashMap::new();
        for entry in self.stats.lock().unwrap().values() {
            if let Some(name) = server_name {
                if entry.server_name != name {
                    continue;
                }
            }
            let server = result.entry(entry.server_name.clone()).or_default();
            server.tool_count += 1;
            server.call_count += entry.call_count;
            server.success_count += entry.success_count;
        }
        result
    }

    /// Compose a full analytics report.
    pub fn generate_report(&self, limit: usize) -> AnalyticsReport {
        let all = self.stats.lock().unwrap();
        let active = all.values().filter(|t| t.call_count > 0).count();
        let total_calls: u64 = all.values().map(|t| t.call_count).sum();
        let total_successes: u64 = all.values().map(|t| t.success_count).sum();
        let overall = if total_calls == 0 {
            0.0
        } else {
            (total_successes as f64 / total_calls as f64) * 100.0
        };
        let total_tools = all.len();
        drop(all);

        AnalyticsReport {
            total_tools,
            active_tools: active,
            total_calls,
            overall_success_rate: overall,
            hot_tools: self.get_hot_tools(limit),
            cold_tools: self.get_cold_tools(limit),
            problematic_tools: self.get_problematic_tools(limit),
            server_stats: self.get_server_stats(None),
            generated_at_ms: wf_common::time::now(),
        }
    }

    /// Export the report as a JSON value (compact, lossless).
    pub fn export_json(&self) -> serde_json::Value {
        let report = self.generate_report(20);
        serde_json::json!({
            "total_tools": report.total_tools,
            "active_tools": report.active_tools,
            "total_calls": report.total_calls,
            "overall_success_rate": report.overall_success_rate,
            "hot_tools": report.hot_tools.iter().map(tool_stats_to_json).collect::<Vec<_>>(),
            "cold_tools": report.cold_tools.iter().map(tool_stats_to_json).collect::<Vec<_>>(),
            "problematic_tools": report.problematic_tools.iter().map(tool_stats_to_json).collect::<Vec<_>>(),
            "server_stats": report.server_stats.iter().map(|(k, v)| {
                serde_json::json!({
                    "server": k,
                    "tool_count": v.tool_count,
                    "call_count": v.call_count,
                    "success_rate": v.success_rate(),
                })
            }).collect::<Vec<_>>(),
            "generated_at_ms": report.generated_at_ms,
        })
    }

    /// Human-readable markdown summary.
    pub fn get_summary(&self) -> String {
        let report = self.generate_report(10);
        let mut lines = vec![
            "# MCP Tools Usage Analytics Summary".to_string(),
            String::new(),
            "## Overview".into(),
            format!("- Total Tools: {}", report.total_tools),
            format!("- Active Tools: {}", report.active_tools),
            format!("- Total Calls: {}", report.total_calls),
            format!(
                "- Overall Success Rate: {:.2}%",
                report.overall_success_rate
            ),
            String::new(),
            "## Top 5 Hot Tools".into(),
        ];
        for tool in report.hot_tools.iter().take(5) {
            lines.push(format!(
                "- `{}/{}`: {} calls ({:.2}% success)",
                tool.server_name, tool.tool_name, tool.call_count, tool.success_rate
            ));
        }
        lines.push(String::new());
        lines.push("## Top 5 Problematic Tools".into());
        for tool in report.problematic_tools.iter().take(5) {
            lines.push(format!(
                "- `{}/{}`: {:.2}% success rate",
                tool.server_name, tool.tool_name, tool.success_rate
            ));
        }
        lines.push(String::new());
        lines.push("## Per-Server Stats".into());
        let mut servers: Vec<_> = report.server_stats.into_iter().collect();
        servers.sort_by(|a, b| a.0.cmp(&b.0));
        for (server, stats) in servers {
            lines.push(format!(
                "- `{}`: {} tools, {} calls ({:.2}% success)",
                server,
                stats.tool_count,
                stats.call_count,
                stats.success_rate()
            ));
        }
        lines.join("\n")
    }

    /// Recent execution history (newest last), optionally filtered by tool.
    pub fn get_execution_history(
        &self,
        server_name: &str,
        tool_name: &str,
        limit: usize,
    ) -> Vec<serde_json::Value> {
        let tool_id = Self::tool_id(server_name, tool_name);
        let history = self.history.lock().unwrap();
        let mut recent: Vec<&ExecutionRecord> = history
            .iter()
            .rev()
            .filter(|r| r.tool_id == tool_id)
            .take(limit)
            .collect();
        recent.reverse();
        recent
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "tool_id": r.tool_id,
                    "execution_ms": r.execution_ms,
                    "success": r.success,
                    "error": r.error,
                    "user_id": r.user_id,
                    "timestamp": r.timestamp,
                })
            })
            .collect()
    }

    /// Clear all analytics data.
    pub fn clear(&self) {
        self.stats.lock().unwrap().clear();
        self.history.lock().unwrap().clear();
    }
}

fn tool_stats_to_json(stats: &ToolStats) -> serde_json::Value {
    serde_json::json!({
        "server_name": stats.server_name,
        "tool_name": stats.tool_name,
        "call_count": stats.call_count,
        "success_count": stats.success_count,
        "failure_count": stats.failure_count,
        "avg_execution_ms": stats.avg_execution_ms,
        "min_execution_ms": if stats.min_execution_ms == u64::MAX { 0 } else { stats.min_execution_ms },
        "max_execution_ms": stats.max_execution_ms,
        "success_rate": stats.success_rate,
        "last_error": stats.last_error,
        "first_executed_at": stats.first_executed_at,
        "last_executed_at": stats.last_executed_at,
        "error_counts": stats.error_counts,
        "user_call_counts": stats.user_call_counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_get_stats() {
        let analytics = McpUsageAnalytics::new(100);
        analytics.record_execution("db", "query", 50, true, None, Some("alice".into()));
        analytics.record_execution(
            "db",
            "query",
            150,
            false,
            Some("timeout".into()),
            Some("alice".into()),
        );
        analytics.record_execution("db", "query", 100, true, None, None);

        let stats = analytics.get_tool_stats("db", "query").unwrap();
        assert_eq!(stats.call_count, 3);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.min_execution_ms, 50);
        assert_eq!(stats.max_execution_ms, 150);
        assert!((stats.avg_execution_ms - 100.0).abs() < 0.001);
        assert!((stats.success_rate - 66.666).abs() < 0.01);
        assert_eq!(stats.last_error.as_deref(), Some("timeout"));
        assert_eq!(stats.user_call_counts.get("alice"), Some(&2));
    }

    #[test]
    fn test_hot_and_problematic_tools() {
        let analytics = McpUsageAnalytics::new(100);
        analytics.record_execution("s1", "hot", 10, true, None, None);
        analytics.record_execution("s1", "hot", 10, true, None, None);
        analytics.record_execution("s1", "hot", 10, true, None, None);
        analytics.record_execution("s1", "bad", 10, false, Some("boom".into()), None);
        analytics.record_execution("s1", "bad", 10, false, Some("boom".into()), None);

        let hot = analytics.get_hot_tools(5);
        assert_eq!(hot[0].tool_name, "hot");

        let problematic = analytics.get_problematic_tools(5);
        assert_eq!(problematic[0].tool_name, "bad");
        assert_eq!(problematic[0].success_rate, 0.0);
    }

    #[test]
    fn test_server_stats() {
        let analytics = McpUsageAnalytics::new(100);
        analytics.record_execution("s1", "a", 10, true, None, None);
        analytics.record_execution("s1", "b", 10, false, Some("e".into()), None);
        analytics.record_execution("s2", "c", 10, true, None, None);

        let all = analytics.get_server_stats(None);
        assert_eq!(all["s1"].tool_count, 2);
        assert_eq!(all["s1"].call_count, 2);
        assert_eq!(all["s2"].call_count, 1);

        let filtered = analytics.get_server_stats(Some("s1"));
        assert!(!filtered.contains_key("s2"));
    }

    #[test]
    fn test_bounded_history() {
        let analytics = McpUsageAnalytics::new(3);
        for _ in 0..5 {
            analytics.record_execution("s", "t", 10, true, None, None);
        }
        let history = analytics.get_execution_history("s", "t", 100);
        assert!(
            history.len() <= 3,
            "history should be bounded, got {}",
            history.len()
        );
    }

    #[test]
    fn test_report_and_export() {
        let analytics = McpUsageAnalytics::new(100);
        analytics.record_execution("s1", "a", 10, true, None, None);
        analytics.record_execution("s1", "b", 20, false, Some("x".into()), None);

        let report = analytics.generate_report(10);
        assert_eq!(report.total_tools, 2);
        assert_eq!(report.active_tools, 2);
        assert_eq!(report.total_calls, 2);
        assert!((report.overall_success_rate - 50.0).abs() < 0.001);

        let json = analytics.export_json();
        assert_eq!(json["total_tools"], 2);

        let summary = analytics.get_summary();
        assert!(summary.contains("s1"));
    }
}
