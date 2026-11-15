# system 域 HTTP 端点
> 操作数：58
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/` | `handle_root` |  |
| GET | `/api/v1/dependencies/audit` | `handle_audit` |  |
| GET | `/api/v1/dependencies/dependents` | `handle_dependents` | kind (q), id (q) |
| GET | `/api/v1/dependencies/impact` | `handle_impact` | kind (q), id (q) |
| DELETE | `/api/v1/events` | `handle_clear_events` | force (q) |
| GET | `/api/v1/events` | `handle_list_events` | limit (q), cursor (q), execution_id (q), agent_loop_id (q), workflow_id (q) |
| GET | `/api/v1/events/agent-timeline/{id}` | `handle_agent_timeline` | id (p), limit (q), cursor (q) |
| GET | `/api/v1/events/agent/stats` | `handle_agent_loop_statistics` |  |
| GET | `/api/v1/events/agent/{agentLoopId}` | `handle_agent_events` | agentLoopId (p), limit (q), offset (q) |
| GET | `/api/v1/events/agent/{agentLoopId}/tool-executions` | `handle_agent_tool_execution_events` | agentLoopId (p), limit (q), offset (q) |
| GET | `/api/v1/events/agent/{agentLoopId}/turns` | `handle_agent_turn_events` | agentLoopId (p), limit (q), offset (q) |
| GET | `/api/v1/events/execution-timeline/{executionId}` | `handle_execution_timeline_view` | executionId (p), limit (q), cursor (q) |
| GET | `/api/v1/events/execution-timeline/{executionId}/summary` | `handle_execution_timeline_summary` | executionId (p) |
| GET | `/api/v1/events/listener-stats/{executionId}` | `handle_listener_stats` | executionId (p) |
| GET | `/api/v1/events/search` | `handle_search_events` | q (q), execution_id (q), agent_loop_id (q), workflow_id (q), limit (q), cursor (q) |
| GET | `/api/v1/events/size` | `handle_event_size` |  |
| GET | `/api/v1/events/stats` | `handle_event_stats` |  |
| GET | `/api/v1/events/stream` | `handle_event_stream` | execution_id (q), agent_loop_id (q), workflow_id (q), checkpoint_only (q), file_path (q), event_types (q), types (q), snapshot_id (q), since (q), ticket (q) |
| POST | `/api/v1/events/stream-ticket` | `post_events_stream_ticket` |  |
| GET | `/api/v1/events/time-range` | `handle_event_time_range` |  |
| GET | `/api/v1/events/timeline/{executionId}` | `handle_execution_timeline` | executionId (p), limit (q), cursor (q) |
| GET | `/api/v1/info` | `handle_info` |  |
| GET | `/api/v1/metrics/agents` | `handle_agents` | profile_id (q) |
| GET | `/api/v1/metrics/collectors` | `handle_collectors` |  |
| GET | `/api/v1/metrics/export` | `get_metrics_export` | format (q) |
| GET | `/api/v1/metrics/node-templates` | `handle_node_templates` | top_n (q) |
| GET | `/api/v1/metrics/report` | `handle_report` |  |
| GET | `/api/v1/metrics/token` | `handle_token` |  |
| GET | `/api/v1/metrics/workflow` | `handle_workflow` | workflow_id (q) |
| GET | `/api/v1/server/config` | `handle_server_config` |  |
| PUT | `/api/v1/server/config` | `handle_server_config_update` |  |
| GET | `/api/v1/server/connect` | `handle_server_connect_query` | code (q) |
| GET | `/api/v1/server/connect/{code}` | `handle_server_connect` | code (p) |
| GET | `/api/v1/server/info` | `handle_server_info` |  |
| GET | `/api/v1/server/migration-status` | `handle_migration_status` |  |
| POST | `/api/v1/server/pair` | `handle_server_pair` |  |
| GET | `/api/v1/server/projects` | `handle_server_projects` |  |
| POST | `/api/v1/server/projects` | `handle_server_projects_refresh` |  |
| GET | `/api/v1/server/shells` | `handle_server_shells` |  |
| GET | `/api/v1/server/status` | `handle_server_status` |  |
| GET | `/api/v1/storage/diagnose` | `handle_storage_diagnose` |  |
| GET | `/api/v1/storage/health` | `handle_storage_health` |  |
| GET | `/api/v1/storage/stats` | `handle_storage_stats` |  |
| GET | `/api/v1/system/code-context` | `handle_code_context` |  |
| GET | `/api/v1/system/code-context/dead-letters` | `handle_dead_letters` |  |
| POST | `/api/v1/system/code-context/dead-letters/acknowledge` | `handle_dead_letters_acknowledge` | body |
| POST | `/api/v1/system/code-context/dead-letters/retry` | `handle_dead_letters_retry` | body |
| POST | `/api/v1/system/code-context/reindex` | `handle_code_context_reindex` |  |
| GET | `/api/v1/system/debug/heap` | `handle_debug_heap` |  |
| GET | `/api/v1/system/debug/location` | `get_system_debug_location` |  |
| GET | `/api/v1/system/directories` | `get_system_directories` |  |
| POST | `/api/v1/system/directories/invalidate` | `post_system_directories_invalidate` | body |
| GET | `/api/v1/system/stale` | `handle_stale` |  |
| GET | `/api/v1/system/tasks` | `handle_system_tasks` |  |
| GET | `/health` | `handle_health` |  |
| GET | `/metrics` | `handle_metrics` |  |
| GET | `/system/diagnostics` | `handle_diagnostics` |  |
| GET | `/system/event-health` | `handle_event_health` |  |
