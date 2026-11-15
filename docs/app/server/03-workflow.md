# workflow 域 HTTP 端点
> 操作数：132
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/v1/approvals/check` | `handle_check_approval` | body |
| POST | `/api/v1/approvals/execute-tool` | `post_approvals_execute_tool` | body |
| POST | `/api/v1/approvals/request` | `handle_request_approval` | body |
| GET | `/api/v1/executions` | `handle_list_executions` | limit (q), offset (q), workflow_id (q), status (q), started_from (q), started_to (q), order (q) |
| GET | `/api/v1/executions/count` | `handle_count_executions` | workflow_id (q), status (q), started_from (q), started_to (q) |
| DELETE | `/api/v1/executions/{id}` | `handle_delete_execution` | id (p), full (q) |
| GET | `/api/v1/executions/{id}` | `handle_get_execution` | id (p) |
| GET | `/api/v1/executions/{id}/agent-loops` | `handle_list_workflow_agent_loops` | id (p) |
| POST | `/api/v1/executions/{id}/agent-loops/{node_id}/append` | `handle_append_workflow_agent_loop` | id (p), node_id (p), body |
| POST | `/api/v1/executions/{id}/agent-loops/{node_id}/append-mode` | `handle_set_workflow_agent_loop_append_mode` | id (p), node_id (p), body |
| POST | `/api/v1/executions/{id}/agent-loops/{node_id}/convert-pending` | `handle_convert_workflow_agent_loop_pending` | id (p), node_id (p) |
| GET | `/api/v1/executions/{id}/analysis/alternatives` | `handle_analysis_alternatives` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/decision-points` | `handle_decision_points` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/efficiency` | `handle_analysis_efficiency` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/paths` | `handle_analysis_paths` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/paths/enumerate` | `handle_enumerate_paths` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/probabilities` | `handle_analysis_probabilities` | id (p) |
| GET | `/api/v1/executions/{id}/analysis/slow-nodes` | `handle_slow_nodes` | id (p), percentile (q) |
| GET | `/api/v1/executions/{id}/call-stack` | `handle_call_stack` | id (p) |
| POST | `/api/v1/executions/{id}/cancel` | `handle_cancel` | id (p) |
| GET | `/api/v1/executions/{id}/context` | `handle_context` | id (p) |
| GET | `/api/v1/executions/{id}/context-evolution` | `handle_context_evolution` | id (p) |
| GET | `/api/v1/executions/{id}/context-snapshots` | `handle_key_context_snapshots` | id (p) |
| GET | `/api/v1/executions/{id}/context-transitions` | `handle_context_transitions` | id (p) |
| GET | `/api/v1/executions/{id}/failed-nodes` | `handle_failed_nodes` | id (p) |
| GET | `/api/v1/executions/{id}/graph` | `handle_execution_graph` | id (p) |
| POST | `/api/v1/executions/{id}/graph/clear` | `handle_clear_execution_graph` | id (p) |
| GET | `/api/v1/executions/{id}/graph/edges` | `handle_execution_graph_edges` | id (p) |
| GET | `/api/v1/executions/{id}/graph/neighbors/{nodeId}` | `handle_execution_graph_neighbors` | id (p), nodeId (p) |
| GET | `/api/v1/executions/{id}/graph/nodes` | `handle_execution_graph_nodes` | id (p) |
| GET | `/api/v1/executions/{id}/graph/overview` | `handle_execution_graph_overview` | id (p) |
| GET | `/api/v1/executions/{id}/graph/path-stats` | `handle_execution_path_stats` | id (p) |
| GET | `/api/v1/executions/{id}/graph/reachability` | `handle_execution_graph_reachability` | id (p) |
| GET | `/api/v1/executions/{id}/iterations` | `handle_iterations` | id (p) |
| GET | `/api/v1/executions/{id}/llm-reasoning-path/{nodeId}` | `handle_llm_reasoning_path` | id (p), nodeId (p) |
| GET | `/api/v1/executions/{id}/memory` | `handle_memory` | id (p) |
| GET | `/api/v1/executions/{id}/node-stats` | `handle_node_stats` | id (p), node_id (q), node_type (q), status (q), has_errors (q), min_duration (q) |
| GET | `/api/v1/executions/{id}/nodes` | `handle_execution_nodes` | id (p) |
| GET | `/api/v1/executions/{id}/nodes/by-type/{nodeType}` | `handle_nodes_by_type` | id (p), nodeType (p) |
| GET | `/api/v1/executions/{id}/nodes/{nodeId}` | `handle_node_analysis` | id (p), nodeId (p) |
| GET | `/api/v1/executions/{id}/nodes/{nodeId}/input-context` | `handle_node_input_context` | id (p), nodeId (p) |
| GET | `/api/v1/executions/{id}/nodes/{nodeId}/transitions` | `handle_node_transitions` | id (p), nodeId (p), from (q), to (q) |
| GET | `/api/v1/executions/{id}/optimizations` | `handle_optimizations` | id (p) |
| GET | `/api/v1/executions/{id}/path` | `handle_execution_path` | id (p) |
| POST | `/api/v1/executions/{id}/pause` | `handle_pause` | id (p) |
| POST | `/api/v1/executions/{id}/rerun` | `post_executions_id_rerun` | id (p) |
| POST | `/api/v1/executions/{id}/resume` | `handle_resume` | id (p) |
| GET | `/api/v1/executions/{id}/state` | `handle_state` | id (p) |
| GET | `/api/v1/executions/{id}/state-analysis` | `handle_state_analysis` | id (p) |
| DELETE | `/api/v1/executions/{id}/state-records` | `handle_clear_state_records` | id (p) |
| GET | `/api/v1/executions/{id}/state-records` | `handle_state_records` | id (p) |
| GET | `/api/v1/executions/{id}/state-records/call-stack` | `handle_state_call_stack` | id (p) |
| GET | `/api/v1/executions/{id}/state-records/iterations/{iteration}` | `handle_state_at_iteration` | id (p), iteration (p) |
| GET | `/api/v1/executions/{id}/state-records/memory` | `handle_state_memory` | id (p) |
| GET | `/api/v1/executions/{id}/state-records/memory/peak` | `handle_state_memory_peak` | id (p) |
| GET | `/api/v1/executions/{id}/state-records/most-changed` | `handle_most_changed_variables` | id (p), limit (q) |
| GET | `/api/v1/executions/{id}/state-records/mutation-count` | `handle_variable_mutation_count` | id (p) |
| GET | `/api/v1/executions/{id}/state-records/snapshots/{timestamp}` | `handle_variable_snapshot_at` | id (p), timestamp (p) |
| GET | `/api/v1/executions/{id}/state-records/variables/{name}/history` | `handle_state_variable_history` | id (p), name (p), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/status` | `handle_status` | id (p) |
| GET | `/api/v1/executions/{id}/tool-chain/{nodeId}` | `handle_tool_chain` | id (p), nodeId (p) |
| GET | `/api/v1/executions/{id}/transitions` | `handle_transitions` | id (p) |
| GET | `/api/v1/executions/{id}/triggers` | `get_executions_id_triggers` | id (p), trigger_name (q), limit (q), offset (q) |
| GET | `/api/v1/executions/{id}/variable-injections` | `handle_list_variable_injections` | id (p) |
| POST | `/api/v1/executions/{id}/variable-injections` | `handle_submit_variable_injection` | id (p), body |
| POST | `/api/v1/executions/{id}/variable-injections/apply` | `handle_apply_variable_injection` | id (p), body |
| DELETE | `/api/v1/executions/{id}/variable-injections/{injection_id}` | `handle_delete_variable_injection` | id (p), injection_id (p) |
| GET | `/api/v1/executions/{id}/variable-snapshots` | `handle_variable_snapshots` | id (p), start (q), end (q) |
| GET | `/api/v1/executions/{id}/variables` | `handle_variables` | id (p) |
| GET | `/api/v1/interactions` | `get_interactions` | limit (q), offset (q), execution_id (q), status (q), interaction_type (q) |
| POST | `/api/v1/interactions` | `handle_save_interaction` | body |
| GET | `/api/v1/interactions/by-execution/{executionId}` | `handle_interactions_by_execution` | executionId (p), limit (q), offset (q) |
| GET | `/api/v1/interactions/by-status/{status}` | `handle_interactions_by_status` | status (p), limit (q), offset (q) |
| GET | `/api/v1/interactions/stats` | `handle_interaction_stats` |  |
| DELETE | `/api/v1/interactions/{id}` | `handle_delete_interaction` | id (p) |
| GET | `/api/v1/interactions/{id}` | `get_interactions_id` | id (p) |
| POST | `/api/v1/interactions/{id}/respond` | `post_interactions_id_respond` | id (p), body |
| GET | `/api/v1/workflows` | `handle_list_workflows` | limit (q), offset (q), name (q), type (q), published (q) |
| POST | `/api/v1/workflows` | `handle_create_workflow` | body |
| GET | `/api/v1/workflows/by-author/{author}` | `handle_workflows_by_author` | author (p), limit (q), offset (q) |
| GET | `/api/v1/workflows/by-category/{category}` | `handle_workflows_by_category` | category (p), limit (q), offset (q) |
| GET | `/api/v1/workflows/by-name/{name}` | `handle_workflow_by_name` | name (p) |
| GET | `/api/v1/workflows/by-tags` | `handle_workflows_by_tags` | q (q), tags (q), category (q), author (q), limit (q), offset (q) |
| GET | `/api/v1/workflows/drafts` | `get_workflows_drafts` | definition_id (q) |
| POST | `/api/v1/workflows/drafts` | `post_workflows_drafts` | body |
| POST | `/api/v1/workflows/drafts/promote-all` | `handle_promote_all` |  |
| DELETE | `/api/v1/workflows/drafts/{id}` | `delete_workflows_drafts_id` | id (p) |
| GET | `/api/v1/workflows/drafts/{id}` | `get_workflows_drafts_id` | id (p) |
| POST | `/api/v1/workflows/drafts/{id}/promote` | `post_workflows_drafts_id_promote` | id (p) |
| GET | `/api/v1/workflows/drafts/{id}/validate` | `get_workflows_drafts_id_validate` | id (p) |
| POST | `/api/v1/workflows/export-all` | `handle_export_workflows` | body |
| POST | `/api/v1/workflows/import` | `handle_import_workflow` | body |
| POST | `/api/v1/workflows/import-many` | `handle_import_many` | body |
| POST | `/api/v1/workflows/parse` | `handle_parse_workflow` | body |
| GET | `/api/v1/workflows/search` | `handle_search_workflows` | q (q), tags (q), category (q), author (q), limit (q), offset (q) |
| GET | `/api/v1/workflows/summaries` | `handle_workflow_summaries` | limit (q), offset (q), name (q), type (q), published (q) |
| POST | `/api/v1/workflows/transform` | `handle_transform_workflow` | body |
| POST | `/api/v1/workflows/validate` | `handle_validate_workflow` | body |
| POST | `/api/v1/workflows/validate/node` | `handle_validate_node` | body |
| DELETE | `/api/v1/workflows/{id}` | `handle_delete_workflow` | id (p) |
| GET | `/api/v1/workflows/{id}` | `handle_get_workflow` | id (p) |
| PUT | `/api/v1/workflows/{id}` | `handle_update_workflow` | id (p), body |
| POST | `/api/v1/workflows/{id}/clone` | `handle_clone_workflow` | id (p), body |
| POST | `/api/v1/workflows/{id}/execute` | `handle_execute_workflow` | id (p), body |
| POST | `/api/v1/workflows/{id}/execute/background` | `handle_execute_background` | id (p), body |
| POST | `/api/v1/workflows/{id}/execute/stream` | `handle_execute_stream` | id (p), body |
| GET | `/api/v1/workflows/{id}/export` | `handle_export_workflow` | id (p), format (q), download (q) |
| GET | `/api/v1/workflows/{id}/graph` | `handle_graph` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/analysis` | `handle_graph_analysis` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/cycles` | `handle_graph_cycles` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/edges` | `handle_graph_edges` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/neighbors/{nodeId}` | `handle_graph_neighbors` | id (p), nodeId (p) |
| GET | `/api/v1/workflows/{id}/graph/nodes` | `handle_graph_nodes` | id (p), node_type (q) |
| GET | `/api/v1/workflows/{id}/graph/reachability` | `handle_graph_reachability` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/summary` | `handle_graph_summary` | id (p) |
| GET | `/api/v1/workflows/{id}/graph/topology` | `handle_graph_topology` | id (p) |
| DELETE | `/api/v1/workflows/{id}/layout` | `handle_delete_workflow_layout` | id (p) |
| GET | `/api/v1/workflows/{id}/layout` | `handle_get_workflow_layout` | id (p) |
| PUT | `/api/v1/workflows/{id}/layout` | `handle_update_workflow_layout` | id (p), body |
| GET | `/api/v1/workflows/{id}/lifecycle` | `get_workflows_id_lifecycle` | id (p) |
| GET | `/api/v1/workflows/{id}/lock` | `handle_get_lock` | id (p) |
| POST | `/api/v1/workflows/{id}/lock/acquire` | `handle_acquire_lock` | id (p), body |
| POST | `/api/v1/workflows/{id}/lock/heartbeat` | `handle_heartbeat_lock` | id (p), body |
| POST | `/api/v1/workflows/{id}/lock/release` | `handle_release_lock` | id (p), body |
| PATCH | `/api/v1/workflows/{id}/metadata` | `handle_update_metadata` | id (p), body |
| POST | `/api/v1/workflows/{id}/publish` | `handle_publish_workflow` | id (p) |
| POST | `/api/v1/workflows/{id}/rollback` | `handle_rollback_workflow` | id (p), body |
| POST | `/api/v1/workflows/{id}/unpublish` | `handle_unpublish_workflow` | id (p) |
| GET | `/api/v1/workflows/{id}/versions` | `handle_list_versions` | id (p) |
| POST | `/api/v1/workflows/{id}/versions` | `handle_save_version` | id (p), body |
| POST | `/api/v1/workflows/{id}/versions/increment` | `handle_increment_version` | id (p), level (q), body |
| GET | `/api/v1/workflows/{id}/versions/{version}` | `handle_get_version` | id (p), version (p) |
