# template 域 HTTP 端点
> 操作数：58
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/templates/library` | `handle_query_library` | kind (q), name (q), category (q), author (q), tags (q), include_system (q) |
| GET | `/api/v1/templates/library/featured` | `handle_library_featured` | limit (q) |
| GET | `/api/v1/templates/library/popular` | `handle_library_popular` | category (q), limit (q) |
| POST | `/api/v1/templates/library/{id}/clone` | `handle_clone_template` | id (p), body |
| POST | `/api/v1/templates/library/{id}/usage` | `handle_record_usage` | id (p) |
| GET | `/api/v1/templates/node` | `handle_list_node_templates` | limit (q), offset (q), node_type (q), q (q) |
| POST | `/api/v1/templates/node` | `handle_save_node_template` | body |
| POST | `/api/v1/templates/node/batch-delete` | `handle_batch_delete_node_templates` | body |
| GET | `/api/v1/templates/node/count` | `handle_count_node_templates` |  |
| GET | `/api/v1/templates/node/drafts` | `get_templates_node_drafts` | definition_id (q) |
| POST | `/api/v1/templates/node/drafts` | `post_templates_node_drafts` | body |
| DELETE | `/api/v1/templates/node/drafts/{id}` | `delete_templates_node_drafts_id` | id (p) |
| GET | `/api/v1/templates/node/drafts/{id}` | `get_templates_node_drafts_id` | id (p) |
| POST | `/api/v1/templates/node/drafts/{id}/promote` | `post_templates_node_drafts_id_promote` | id (p) |
| GET | `/api/v1/templates/node/drafts/{id}/validate` | `get_templates_node_drafts_id_validate` | id (p) |
| POST | `/api/v1/templates/node/import` | `handle_import_node_template` | body |
| POST | `/api/v1/templates/node/validate` | `post_templates_node_validate` | body |
| DELETE | `/api/v1/templates/node/{id}` | `handle_delete_node_template` | id (p) |
| GET | `/api/v1/templates/node/{id}` | `handle_get_node_template` | id (p) |
| PUT | `/api/v1/templates/node/{id}` | `handle_update_node_template` | id (p), body |
| GET | `/api/v1/templates/node/{id}/export` | `handle_export_node_template` | id (p), download (q) |
| GET | `/api/v1/templates/node/{id}/lifecycle` | `get_templates_node_id_lifecycle` | id (p) |
| GET | `/api/v1/templates/node/{id}/lock` | `get_templates_node_id_lock` | id (p) |
| POST | `/api/v1/templates/node/{id}/lock/acquire` | `post_templates_node_id_lock_acquire` | id (p), body |
| POST | `/api/v1/templates/node/{id}/lock/heartbeat` | `post_templates_node_id_lock_heartbeat` | id (p), body |
| POST | `/api/v1/templates/node/{id}/lock/release` | `post_templates_node_id_lock_release` | id (p), body |
| POST | `/api/v1/templates/node/{id}/rollback` | `post_templates_node_id_rollback` | id (p), body |
| GET | `/api/v1/templates/node/{id}/versions` | `get_templates_node_id_versions` | id (p) |
| POST | `/api/v1/templates/node/{id}/versions` | `post_templates_node_id_versions` | id (p), body |
| DELETE | `/api/v1/templates/node/{id}/versions/{version}` | `delete_templates_node_id_versions_version` | id (p), version (p) |
| GET | `/api/v1/templates/node/{id}/versions/{version}` | `get_templates_node_id_versions_version` | id (p), version (p) |
| GET | `/api/v1/templates/trigger` | `handle_list_trigger_templates` | trigger_type (q), limit (q), offset (q), name (q), category (q), tags (q), enabled (q) |
| POST | `/api/v1/templates/trigger` | `handle_save_trigger_template` | body |
| POST | `/api/v1/templates/trigger/batch-delete` | `handle_batch_delete_trigger_templates` | body |
| GET | `/api/v1/templates/trigger/count` | `handle_count_trigger_templates` |  |
| GET | `/api/v1/templates/trigger/drafts` | `get_templates_trigger_drafts` | definition_id (q) |
| POST | `/api/v1/templates/trigger/drafts` | `post_templates_trigger_drafts` | body |
| DELETE | `/api/v1/templates/trigger/drafts/{id}` | `delete_templates_trigger_drafts_id` | id (p) |
| GET | `/api/v1/templates/trigger/drafts/{id}` | `get_templates_trigger_drafts_id` | id (p) |
| POST | `/api/v1/templates/trigger/drafts/{id}/promote` | `post_templates_trigger_drafts_id_promote` | id (p) |
| GET | `/api/v1/templates/trigger/drafts/{id}/validate` | `get_templates_trigger_drafts_id_validate` | id (p) |
| POST | `/api/v1/templates/trigger/import` | `handle_import_trigger_template` | body |
| GET | `/api/v1/templates/trigger/search` | `handle_search_trigger_templates` | q (q), limit (q), offset (q) |
| POST | `/api/v1/templates/trigger/validate` | `post_templates_trigger_validate` | body |
| DELETE | `/api/v1/templates/trigger/{id}` | `handle_delete_trigger_template` | id (p) |
| GET | `/api/v1/templates/trigger/{id}` | `handle_get_trigger_template` | id (p) |
| PUT | `/api/v1/templates/trigger/{id}` | `handle_update_trigger_template` | id (p), body |
| GET | `/api/v1/templates/trigger/{id}/export` | `handle_export_trigger_template` | id (p), download (q) |
| GET | `/api/v1/templates/trigger/{id}/lifecycle` | `get_templates_trigger_id_lifecycle` | id (p) |
| GET | `/api/v1/templates/trigger/{id}/lock` | `get_templates_trigger_id_lock` | id (p) |
| POST | `/api/v1/templates/trigger/{id}/lock/acquire` | `post_templates_trigger_id_lock_acquire` | id (p), body |
| POST | `/api/v1/templates/trigger/{id}/lock/heartbeat` | `post_templates_trigger_id_lock_heartbeat` | id (p), body |
| POST | `/api/v1/templates/trigger/{id}/lock/release` | `post_templates_trigger_id_lock_release` | id (p), body |
| POST | `/api/v1/templates/trigger/{id}/rollback` | `post_templates_trigger_id_rollback` | id (p), body |
| GET | `/api/v1/templates/trigger/{id}/versions` | `get_templates_trigger_id_versions` | id (p) |
| POST | `/api/v1/templates/trigger/{id}/versions` | `post_templates_trigger_id_versions` | id (p), body |
| DELETE | `/api/v1/templates/trigger/{id}/versions/{version}` | `delete_templates_trigger_id_versions_version` | id (p), version (p) |
| GET | `/api/v1/templates/trigger/{id}/versions/{version}` | `get_templates_trigger_id_versions_version` | id (p), version (p) |
