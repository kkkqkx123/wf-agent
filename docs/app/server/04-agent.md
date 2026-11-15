# agent 域 HTTP 端点
> 操作数：121
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/agent-checkpoints/stats` | `handle_checkpoint_statistics` |  |
| GET | `/api/v1/agent-definitions` | `handle_list_agent_templates` | limit (q), offset (q), name (q), category (q), author (q), tags (q) |
| POST | `/api/v1/agent-definitions` | `handle_register_agent_template` | body |
| POST | `/api/v1/agent-definitions/import` | `handle_import_agent_template` | body |
| DELETE | `/api/v1/agent-definitions/{id}` | `handle_delete_agent_template` | id (p) |
| GET | `/api/v1/agent-definitions/{id}` | `handle_get_agent_template` | id (p) |
| PUT | `/api/v1/agent-definitions/{id}` | `handle_update_agent_template` | id (p), body |
| GET | `/api/v1/agent-definitions/{id}/export` | `handle_export_agent_template` | id (p), download (q) |
| POST | `/api/v1/agent-definitions/{id}/publish` | `handle_publish_agent_template` | id (p) |
| POST | `/api/v1/agent-definitions/{id}/rollback` | `handle_rollback_agent_template` | id (p), body |
| POST | `/api/v1/agent-definitions/{id}/unpublish` | `handle_unpublish_agent_template` | id (p) |
| POST | `/api/v1/agent-definitions/{id}/validate` | `handle_validate_agent_template` | id (p) |
| GET | `/api/v1/agent-definitions/{id}/versions` | `handle_list_agent_template_versions` | id (p) |
| POST | `/api/v1/agent-definitions/{id}/versions` | `handle_save_agent_template_version` | id (p), body |
| DELETE | `/api/v1/agent-definitions/{id}/versions/{version}` | `handle_delete_agent_template_version` | id (p), version (p) |
| GET | `/api/v1/agent-definitions/{id}/versions/{version}` | `handle_get_agent_template_version` | id (p), version (p) |
| GET | `/api/v1/agent-executions` | `handle_agent_executions` | limit (q), offset (q), status (q), agent_id (q), execution_type (q), started_from (q), started_to (q), order (q) |
| GET | `/api/v1/agent-executions/by-definition/{defId}` | `handle_executions_by_definition` | defId (p), limit (q), offset (q) |
| GET | `/api/v1/agent-executions/by-status/{status}` | `handle_executions_by_status` | status (p), limit (q), offset (q) |
| GET | `/api/v1/agent-executions/count` | `handle_agent_executions_count` | status (q), agent_id (q), execution_type (q), started_from (q), started_to (q) |
| GET | `/api/v1/agent-executions/stats` | `handle_execution_statistics` |  |
| DELETE | `/api/v1/agent-executions/{id}` | `handle_delete_agent_execution` | id (p) |
| GET | `/api/v1/agent-executions/{id}` | `handle_get_agent_execution` | id (p) |
| GET | `/api/v1/agent-executions/{id}/errors` | `handle_error_records` | id (p) |
| GET | `/api/v1/agent-executions/{id}/errors/chain` | `handle_error_chain` | id (p), from_error_id (q) |
| GET | `/api/v1/agent-executions/{id}/errors/recovery/{errorId}` | `handle_recovery_proposal` | id (p), errorId (p) |
| GET | `/api/v1/agent-executions/{id}/errors/root-cause` | `handle_root_cause` | id (p) |
| GET | `/api/v1/agent-executions/{id}/errors/similar/{errorId}` | `handle_similar_errors` | id (p), errorId (p) |
| GET | `/api/v1/agent-executions/{id}/errors/statistics` | `handle_error_statistics` | id (p) |
| GET | `/api/v1/agent-executions/{id}/errors/statistics/advanced` | `handle_advanced_error_statistics` | id (p) |
| GET | `/api/v1/agent-loops` | `handle_list_loops` | limit (q), offset (q), status (q) |
| POST | `/api/v1/agent-loops` | `handle_save_loop` | body |
| POST | `/api/v1/agent-loops/cleanup-completed` | `handle_cleanup_completed` |  |
| POST | `/api/v1/agent-loops/import` | `handle_import_loop` | body |
| GET | `/api/v1/agent-loops/latest-session-anchor` | `handle_latest_session_anchor` | directory (q) |
| GET | `/api/v1/agent-loops/stats` | `handle_loop_statistics` |  |
| GET | `/api/v1/agent-loops/summaries` | `handle_loop_summaries` | status (q), profile_id (q), limit (q), offset (q), order (q), q (q), directory (q) |
| DELETE | `/api/v1/agent-loops/{id}` | `handle_delete_loop` | id (p) |
| GET | `/api/v1/agent-loops/{id}` | `handle_get_loop` | id (p) |
| PUT | `/api/v1/agent-loops/{id}` | `handle_update_loop` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/append` | `handle_append_loop` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/append-mode` | `handle_set_append_mode_loop` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/cancel` | `handle_cancel_loop` | id (p) |
| DELETE | `/api/v1/agent-loops/{id}/checkpoints` | `handle_delete_checkpoints` | id (p) |
| GET | `/api/v1/agent-loops/{id}/checkpoints` | `get_agent_loops_id_checkpoints` | id (p), limit (q), offset (q) |
| POST | `/api/v1/agent-loops/{id}/checkpoints` | `post_agent_loops_id_checkpoints` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/checkpoints/chain` | `get_agent_loops_id_checkpoints_chain` | id (p) |
| DELETE | `/api/v1/agent-loops/{id}/checkpoints/{cid}` | `delete_agent_loops_id_checkpoints_cid` | id (p), cid (p) |
| POST | `/api/v1/agent-loops/{id}/checkpoints/{cid}/restore` | `post_agent_loops_id_checkpoints_cid_restore` | id (p), cid (p) |
| POST | `/api/v1/agent-loops/{id}/checkpoints/{cid}/resume` | `handle_resume_checkpoint` | id (p), cid (p), body |
| GET | `/api/v1/agent-loops/{id}/context-evolution` | `handle_loop_context_evolution` | id (p) |
| GET | `/api/v1/agent-loops/{id}/conversation` | `get_agent_loops_id_conversation` | id (p), max_messages (q), limit (q), offset (q) |
| POST | `/api/v1/agent-loops/{id}/convert-pending` | `handle_convert_pending_to_steering_loop` | id (p) |
| GET | `/api/v1/agent-loops/{id}/execution-path` | `handle_loop_execution_path` | id (p) |
| POST | `/api/v1/agent-loops/{id}/fork` | `handle_fork_loop` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/graph` | `handle_decision_graph` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/alternatives` | `handle_all_alternatives` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/alternatives/iterations/{iteration}` | `handle_alternative_decisions` | id (p), iteration (p) |
| GET | `/api/v1/agent-loops/{id}/graph/edges` | `handle_decision_edges` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/efficiency` | `handle_path_efficiency` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/nodes` | `handle_decision_nodes` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/paths` | `handle_all_paths` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/paths/critical-path` | `handle_critical_path` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/paths/execution-path` | `handle_graph_execution_path` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/paths/path-stats` | `handle_path_statistics` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/paths/steps` | `handle_execution_path_steps` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/patterns` | `handle_decision_patterns` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/probabilities` | `handle_path_probabilities` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/sequences` | `handle_decision_sequence` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/sequences/iterations/{iteration}` | `handle_decisions_in_iteration` | id (p), iteration (p) |
| GET | `/api/v1/agent-loops/{id}/graph/sequences/types/{decisionType}` | `handle_decisions_by_type` | id (p), decisionType (p) |
| GET | `/api/v1/agent-loops/{id}/graph/tool-frequency` | `handle_tool_frequency` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/unexplored` | `handle_unexplored_alternatives` | id (p) |
| GET | `/api/v1/agent-loops/{id}/graph/unexplored/best` | `handle_most_promising_unexplored` | id (p) |
| GET | `/api/v1/agent-loops/{id}/iteration-history` | `handle_iteration_history` | id (p), limit (q), offset (q) |
| GET | `/api/v1/agent-loops/{id}/iteration-history/summary` | `handle_iteration_history_summary` | id (p) |
| GET | `/api/v1/agent-loops/{id}/messages` | `handle_recent_messages` | id (p), count (q), limit (q), offset (q) |
| POST | `/api/v1/agent-loops/{id}/messages/dedupe` | `handle_dedupe_messages` | id (p) |
| GET | `/api/v1/agent-loops/{id}/messages/search` | `get_agent_loops_id_messages_search` | id (p), q (q), limit (q), offset (q) |
| GET | `/api/v1/agent-loops/{id}/messages/stats` | `get_agent_loops_id_messages_stats` | id (p) |
| POST | `/api/v1/agent-loops/{id}/pause` | `handle_pause_loop` | id (p) |
| GET | `/api/v1/agent-loops/{id}/performance` | `get_agent_loops_id_performance` | id (p) |
| GET | `/api/v1/agent-loops/{id}/performance/comparison` | `get_agent_loops_id_performance_comparison` | id (p) |
| POST | `/api/v1/agent-loops/{id}/recap` | `handle_loop_recap` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/resume` | `handle_resume_loop` | id (p) |
| POST | `/api/v1/agent-loops/{id}/run` | `handle_run_loop` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/share` | `handle_share_loop` | id (p) |
| GET | `/api/v1/agent-loops/{id}/status` | `handle_loop_status` | id (p) |
| PATCH | `/api/v1/agent-loops/{id}/status` | `handle_update_loop_status` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/status/transition` | `handle_loop_status_transition` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/stream` | `handle_stream_loop` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/summary` | `handle_loop_summary` | id (p) |
| GET | `/api/v1/agent-loops/{id}/timeline` | `handle_loop_timeline` | id (p), limit (q), cursor (q) |
| PATCH | `/api/v1/agent-loops/{id}/title` | `handle_update_loop_title` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/title` | `handle_loop_title` | id (p), body |
| POST | `/api/v1/agent-loops/{id}/transfer` | `handle_transfer_loop` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/variable-history/{name}` | `get_agent_loops_id_variable_history_name` | id (p), name (p), limit (q), offset (q) |
| GET | `/api/v1/agent-loops/{id}/variables` | `get_agent_loops_id_variables` | id (p), limit (q), offset (q) |
| POST | `/api/v1/agent-loops/{id}/variables/batch` | `handle_batch_set_loop_variables` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/variables/export` | `get_agent_loops_id_variables_export` | id (p), download (q) |
| GET | `/api/v1/agent-loops/{id}/variables/stats` | `get_agent_loops_id_variables_stats` | id (p) |
| DELETE | `/api/v1/agent-loops/{id}/variables/{name}` | `delete_agent_loops_id_variables_name` | id (p), name (p) |
| GET | `/api/v1/agent-loops/{id}/variables/{name}` | `get_agent_loops_id_variables_name` | id (p), name (p) |
| PUT | `/api/v1/agent-loops/{id}/variables/{name}` | `put_agent_loops_id_variables_name` | id (p), name (p), body |
| GET | `/api/v1/agents` | `get_agents` | limit (q), offset (q), name (q), is_default (q) |
| POST | `/api/v1/agents` | `handle_save_profile` | body |
| GET | `/api/v1/agents/drafts` | `get_agents_drafts` | definition_id (q) |
| POST | `/api/v1/agents/drafts` | `post_agents_drafts` | body |
| DELETE | `/api/v1/agents/drafts/{id}` | `delete_agents_drafts_id` | id (p) |
| GET | `/api/v1/agents/drafts/{id}` | `get_agents_drafts_id` | id (p) |
| POST | `/api/v1/agents/drafts/{id}/promote` | `post_agents_drafts_id_promote` | id (p) |
| GET | `/api/v1/agents/drafts/{id}/validate` | `get_agents_drafts_id_validate` | id (p) |
| POST | `/api/v1/agents/validate` | `handle_validate_agent` | body |
| DELETE | `/api/v1/agents/{id}` | `delete_agents_id` | id (p) |
| GET | `/api/v1/agents/{id}` | `get_agents_id` | id (p) |
| PUT | `/api/v1/agents/{id}` | `put_agents_id` | id (p), body |
| GET | `/api/v1/agents/{id}/lifecycle` | `get_agents_id_lifecycle` | id (p) |
| GET | `/api/v1/agents/{id}/lock` | `get_agents_id_lock` | id (p) |
| POST | `/api/v1/agents/{id}/lock/acquire` | `post_agents_id_lock_acquire` | id (p), body |
| POST | `/api/v1/agents/{id}/lock/heartbeat` | `post_agents_id_lock_heartbeat` | id (p), body |
| POST | `/api/v1/agents/{id}/lock/release` | `post_agents_id_lock_release` | id (p), body |
