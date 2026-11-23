# chat 域 HTTP 端点
> 操作数：38
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
| POST | `/api/v1/chat/sessions` | `handle_create_session` | body |
| GET | `/api/v1/chat/sessions/{id}` | `handle_snapshot` | id (p), leaf (q) |
| DELETE | `/api/v1/chat/sessions/{id}` | `handle_delete_session` | id (p) |
| GET | `/api/v1/chat/sessions/{id}/path` | `handle_path` | id (p), leaf (q) |
| GET | `/api/v1/chat/sessions/{id}/dirty` | `handle_dirty` | id (p), leaf (q) |
| GET | `/api/v1/chat/sessions/{id}/stream` | `handle_stream` | id (p), since (q) |
| GET | `/api/v1/chat/sessions/{id}/grafts` | `handle_grafts` | id (p) |
| POST | `/api/v1/chat/sessions/{id}/turns` | `handle_submit_turn` | id (p), body |
| POST | `/api/v1/chat/sessions/{id}/turns/{turn_id}/cancel` | `handle_cancel_turn` | id (p), turn_id (p) |
| POST | `/api/v1/chat/sessions/{id}/step` | `handle_step` | id (p), body |
| POST | `/api/v1/chat/sessions/{id}/step/{job_id}/cancel` | `handle_cancel_step` | id (p), job_id (p) |
| POST | `/api/v1/chat/sessions/{id}/leaf` | `handle_set_leaf` | id (p), body |
| POST | `/api/v1/chat/sessions/{id}/detach` | `handle_detach` | id (p), body |
| POST | `/api/v1/chat/sessions/{id}/attach` | `handle_attach` | id (p), body |
| DELETE | `/api/v1/chat/sessions/{id}/nodes/{node_id}` | `handle_delete_node` | id (p), node_id (p), if_version (q) |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/insert-before` | `handle_insert_before` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/splice` | `handle_splice` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/reparent` | `handle_reparent` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/content` | `handle_write_content` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/edit` | `handle_edit_branch` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/reply` | `handle_write_reply` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/reply/discard` | `handle_discard_reply` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/reply/restore` | `handle_restore_reply` | id (p), node_id (p), body |
| POST | `/api/v1/chat/sessions/{id}/nodes/{node_id}/acknowledge` | `handle_acknowledge` | id (p), node_id (p), body |
