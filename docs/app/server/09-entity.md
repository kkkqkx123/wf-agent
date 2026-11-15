# entity 域 HTTP 端点
> 操作数：45
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/agent-interactions/{id}` | `get_agent_interactions_id` | id (p) |
| POST | `/api/v1/agent-interactions/{id}/respond` | `post_agent_interactions_id_respond` | id (p), body |
| GET | `/api/v1/agent-loops/{id}/interactions` | `get_agent_loops_id_interactions` | id (p), status (q), limit (q), offset (q) |
| GET | `/api/v1/messages` | `handle_list_messages` | limit (q), offset (q), execution_id (q), agent_loop_id (q), role (q) |
| POST | `/api/v1/messages` | `handle_save_message` | body |
| GET | `/api/v1/messages/by-execution/{executionId}` | `handle_messages_by_execution` | executionId (p), limit (q), offset (q) |
| GET | `/api/v1/messages/conversation/{executionId}` | `get_messages_conversation_executionid` | executionId (p), limit (q), offset (q) |
| GET | `/api/v1/messages/search` | `get_messages_search` | q (q), limit (q), offset (q) |
| GET | `/api/v1/messages/stats` | `get_messages_stats` |  |
| DELETE | `/api/v1/messages/{id}` | `handle_delete_message` | id (p) |
| GET | `/api/v1/messages/{id}` | `handle_get_message` | id (p) |
| GET | `/api/v1/skills` | `handle_list_skills` |  |
| POST | `/api/v1/skills/cache/clear` | `handle_clear_skill_cache` |  |
| POST | `/api/v1/skills/cache/clear/{name}` | `handle_clear_skill_cache_by_name` | name (p) |
| GET | `/api/v1/skills/disabled` | `handle_disabled_skills` |  |
| GET | `/api/v1/skills/enabled` | `handle_enabled_skills` |  |
| GET | `/api/v1/skills/prompt` | `handle_skill_prompt` |  |
| GET | `/api/v1/skills/query` | `handle_query_skills` | name (q), version (q), tags (q) |
| POST | `/api/v1/skills/reload` | `handle_reload_skills` | dir (q) |
| POST | `/api/v1/skills/scan` | `handle_scan_skills` | dir (q) |
| GET | `/api/v1/skills/{name}` | `handle_get_skill` | name (p) |
| GET | `/api/v1/skills/{name}/content` | `handle_skill_content` | name (p) |
| POST | `/api/v1/skills/{name}/disable` | `handle_disable_skill` | name (p) |
| POST | `/api/v1/skills/{name}/enable` | `handle_enable_skill` | name (p) |
| GET | `/api/v1/skills/{name}/resources` | `handle_skill_resources` | name (p), resource_type (q) |
| GET | `/api/v1/tasks` | `handle_list_tasks` | limit (q), offset (q), status (q), task_type (q) |
| POST | `/api/v1/tasks` | `handle_save_task` | body |
| GET | `/api/v1/tasks/by-execution/{executionId}` | `handle_tasks_by_execution` | executionId (p), limit (q), offset (q) |
| POST | `/api/v1/tasks/cleanup` | `handle_cleanup_tasks` | body |
| GET | `/api/v1/tasks/stats` | `handle_task_stats` |  |
| DELETE | `/api/v1/tasks/{id}` | `handle_delete_task` | id (p) |
| GET | `/api/v1/tasks/{id}` | `handle_get_task` | id (p) |
| POST | `/api/v1/tasks/{id}/cancel` | `handle_cancel_task` | id (p) |
| GET | `/api/v1/variables` | `get_variables` | limit (q), offset (q), scope (q), execution_id (q) |
| POST | `/api/v1/variables` | `post_variables` | body |
| POST | `/api/v1/variables/batch` | `handle_batch_set_variables` | body |
| GET | `/api/v1/variables/by-node/{executionId}/{nodeId}` | `handle_variables_at_node` | executionId (p), nodeId (p) |
| GET | `/api/v1/variables/export/{executionId}` | `get_variables_export_executionid` | executionId (p), download (q) |
| GET | `/api/v1/variables/history` | `get_variables_history` | name (q), scope (q), execution_id (q), limit (q), offset (q) |
| POST | `/api/v1/variables/import` | `handle_import_variables` | body |
| GET | `/api/v1/variables/scope/{scope}` | `handle_variables_by_scope` | scope (p), limit (q), offset (q) |
| GET | `/api/v1/variables/scopes/{executionId}` | `handle_variable_scopes` | executionId (p) |
| GET | `/api/v1/variables/stats` | `get_variables_stats` |  |
| DELETE | `/api/v1/variables/{name}` | `delete_variables_name` | name (p), scope (q), execution_id (q) |
| GET | `/api/v1/variables/{name}` | `get_variables_name` | name (p), scope (q), execution_id (q) |
