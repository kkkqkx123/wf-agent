# trigger 域 HTTP 端点
> 操作数：11
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/v1/hooks/{name}` | `handle_webhook_fire` | name (p), body |
| GET | `/api/v1/trigger-executions` | `handle_list_trigger_executions` | limit (q), offset (q), trigger_name (q), execution_id (q), workflow_id (q), outcome (q) |
| POST | `/api/v1/trigger-executions` | `handle_save_trigger_execution` | body |
| GET | `/api/v1/trigger-executions/by-execution/{executionId}` | `handle_trigger_executions_by_execution` | executionId (p), limit (q), offset (q) |
| GET | `/api/v1/trigger-executions/by-trigger/{name}` | `handle_trigger_executions_by_trigger` | name (p), limit (q), offset (q) |
| GET | `/api/v1/trigger-executions/by-workflow/{id}` | `handle_trigger_executions_by_workflow` | id (p), limit (q), offset (q) |
| POST | `/api/v1/trigger-executions/cleanup` | `handle_cleanup_trigger_executions` | body |
| GET | `/api/v1/trigger-executions/stats` | `handle_trigger_execution_stats` |  |
| DELETE | `/api/v1/trigger-executions/{id}` | `handle_delete_trigger_execution` | id (p) |
| GET | `/api/v1/trigger-executions/{id}` | `handle_get_trigger_execution` | id (p) |
| GET | `/api/v1/triggers/history` | `get_triggers_history` | execution_id (q), trigger_name (q), limit (q), offset (q) |
