# chat 域 HTTP 端点
> 操作数：14
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/v1/chat/recap` | `handle_chat_recap` | body |
| GET | `/api/v1/chat/starters` | `handle_list_starters` | locale (q), q (q), tags (q), enabled (q), limit (q), offset (q) |
| POST | `/api/v1/chat/starters` | `handle_create_starter` | body |
| POST | `/api/v1/chat/starters/import` | `handle_import_starter` | body |
| DELETE | `/api/v1/chat/starters/{id}` | `handle_delete_starter` | id (p) |
| GET | `/api/v1/chat/starters/{id}` | `handle_get_starter` | id (p) |
| PUT | `/api/v1/chat/starters/{id}` | `handle_update_starter` | id (p), body |
| POST | `/api/v1/chat/starters/{id}/disable` | `handle_disable_starter` | id (p) |
| POST | `/api/v1/chat/starters/{id}/enable` | `handle_enable_starter` | id (p) |
| GET | `/api/v1/chat/starters/{id}/export` | `handle_export_starter` | id (p), download (q) |
| POST | `/api/v1/chat/starters/{id}/resolve` | `handle_resolve_starter` | id (p), body |
| POST | `/api/v1/chat/starters/{id}/usage` | `handle_record_starter_usage` | id (p) |
| POST | `/api/v1/chat/stream` | `handle_chat_stream` | body |
| POST | `/api/v1/chat/title` | `handle_chat_title` | body |
