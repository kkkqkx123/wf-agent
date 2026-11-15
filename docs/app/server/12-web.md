# web 域 HTTP 端点
> 操作数：13
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/v1/agent-loops/batch-delete` | `handle_batch_delete_loops` | body |
| POST | `/api/v1/executions/batch-cancel` | `handle_batch_cancel` | body |
| POST | `/api/v1/executions/batch-delete` | `handle_batch_delete` | body |
| POST | `/api/v1/executions/batch-pause` | `handle_batch_pause` | body |
| GET | `/api/v1/favorites` | `handle_list_favorites` | limit (q), offset (q), kind (q), pinned_only (q) |
| DELETE | `/api/v1/favorites/{kind}/{id}` | `handle_delete_favorite` | kind (p), id (p) |
| PUT | `/api/v1/favorites/{kind}/{id}` | `handle_upsert_favorite` | kind (p), id (p), body |
| POST | `/api/v1/interactions/batch-respond` | `handle_batch_respond` | body |
| GET | `/api/v1/preferences` | `handle_get_preferences` |  |
| PUT | `/api/v1/preferences` | `handle_replace_preferences` | body |
| DELETE | `/api/v1/preferences/{id}` | `handle_delete_preference` | id (p) |
| GET | `/api/v1/preferences/{id}` | `handle_get_preference` | id (p) |
| PUT | `/api/v1/preferences/{id}` | `handle_set_preference` | id (p), body |
