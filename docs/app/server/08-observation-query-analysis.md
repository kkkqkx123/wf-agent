# observation 域 HTTP 端点
> 操作数：51
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/analysis/llm-metrics` | `handle_llm_metrics` |  |
| GET | `/api/v1/analysis/llm-metrics/execution/{id}` | `handle_execution_llm_metrics` | id (p) |
| GET | `/api/v1/analysis/performance/compare` | `handle_performance_compare` | baseline (q), compared (q) |
| GET | `/api/v1/analysis/stats` | `handle_stats` |  |
| GET | `/api/v1/analysis/stats/agent-profiles` | `handle_agent_stats_by_profile` | limit (q) |
| GET | `/api/v1/analysis/stats/top-node-types` | `handle_top_node_types` | limit (q) |
| GET | `/api/v1/analysis/stats/top-workflows` | `handle_top_workflows` | limit (q) |
| GET | `/api/v1/artifacts` | `handle_query_artifacts` | limit (q), cursor (q), execution_id (q), kind (q), name (q) |
| GET | `/api/v1/executions/{id}/artifacts` | `handle_execution_artifacts` | id (p), limit (q), cursor (q), kind (q), name (q) |
| GET | `/api/v1/executions/{id}/audit/iterations` | `handle_audit_iterations` | id (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/audit/llm-calls` | `handle_audit_llm_calls` | id (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/audit/node-executions` | `handle_audit_node_executions` | id (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/audit/report` | `handle_audit_report` | id (p), download (q) |
| GET | `/api/v1/executions/{id}/audit/summary` | `handle_audit_summary` | id (p) |
| GET | `/api/v1/executions/{id}/audit/timeline` | `handle_audit_timeline` | id (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/audit/tool-calls` | `handle_audit_tool_calls` | id (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/bundle` | `get_executions_id_bundle` | id (p), checkpoint_synced (q) |
| GET | `/api/v1/executions/{id}/error-analysis` | `handle_error_analysis` | id (p) |
| GET | `/api/v1/executions/{id}/error-analysis/advanced` | `handle_error_analysis_advanced` | id (p) |
| GET | `/api/v1/executions/{id}/error-analysis/context` | `handle_error_context` | id (p) |
| GET | `/api/v1/executions/{id}/error-analysis/context/{errorId}` | `handle_error_context_one` | id (p), errorId (p) |
| GET | `/api/v1/executions/{id}/error-analysis/recovery-recommendations` | `handle_error_recovery_recommendations` | id (p) |
| GET | `/api/v1/executions/{id}/error-analysis/recovery/{errorId}` | `handle_error_recovery` | id (p), errorId (p) |
| GET | `/api/v1/executions/{id}/error-analysis/root-cause` | `handle_error_root_cause` | id (p) |
| GET | `/api/v1/executions/{id}/error-analysis/similar` | `handle_error_similar` | id (p), limit (q) |
| GET | `/api/v1/executions/{id}/error-analysis/stream` | `handle_error_chain_stream` | id (p) |
| GET | `/api/v1/executions/{id}/hierarchy` | `get_executions_id_hierarchy` | id (p) |
| GET | `/api/v1/executions/{id}/history` | `get_executions_id_history` | id (p), include (q) |
| GET | `/api/v1/executions/{id}/logs` | `handle_execution_logs` | id (p), limit (q), cursor (q), event_types (q), since (q), message (q) |
| GET | `/api/v1/executions/{id}/performance` | `get_executions_id_performance` | id (p) |
| GET | `/api/v1/executions/{id}/performance/bottlenecks` | `handle_performance_bottlenecks` | id (p) |
| GET | `/api/v1/executions/{id}/performance/iteration-comparison` | `get_executions_id_performance_iteration_comparison` | id (p) |
| GET | `/api/v1/executions/{id}/performance/summary` | `handle_performance_summary` | id (p) |
| GET | `/api/v1/executions/{id}/progress` | `handle_progress` | id (p) |
| GET | `/api/v1/executions/{id}/subtree` | `get_executions_id_subtree` | id (p), limit (q), cursor (q) |
| GET | `/api/v1/logs` | `handle_query_logs` | limit (q), cursor (q), execution_id (q), event_types (q), since (q), message (q) |
| GET | `/api/v1/mentions/search` | `handle_mention_search` | q (q), kinds (q), limit (q) |
| POST | `/api/v1/query` | `handle_query` | body |
| POST | `/api/v1/query/aggregate` | `handle_aggregate` | body |
| GET | `/api/v1/query/distinct` | `handle_distinct` | field (q), workflow_id (q), status (q), start_time_from (q), start_time_to (q), tags (q), custom (q) |
| POST | `/api/v1/query/evaluate` | `handle_evaluate` | body |
| POST | `/api/v1/query/export` | `post_query_export` | download (q), body |
| POST | `/api/v1/query/group-by` | `handle_group_by` | body |
| GET | `/api/v1/search` | `handle_search` | q (q), types (q), limit_per_type (q), limit (q), cursor (q) |
| GET | `/api/v1/unified-executions` | `handle_unified_executions` | limit (q), cursor (q), status (q), execution_type (q), definition_id (q), started_from (q), started_to (q) |
| GET | `/api/v1/unified-executions/count` | `handle_unified_executions_count` | status (q), execution_type (q), definition_id (q), started_from (q), started_to (q) |
| GET | `/api/v1/unified-executions/{id}` | `handle_unified_execution` | id (p) |
| POST | `/api/v1/unified-executions/{id}/cancel` | `handle_unified_cancel` | id (p) |
| POST | `/api/v1/unified-executions/{id}/pause` | `handle_unified_pause` | id (p) |
| POST | `/api/v1/unified-executions/{id}/rerun` | `handle_unified_rerun` | id (p) |
| POST | `/api/v1/unified-executions/{id}/resume` | `handle_unified_resume` | id (p) |
