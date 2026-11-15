# checkpoint 域 HTTP 端点
> 操作数：56
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/checkpoints` | `get_checkpoints` | limit (q), offset (q), entity_id (q), entity_type (q) |
| GET | `/api/v1/checkpoints/entities` | `handle_list_checkpoints_by_entities` | entity_ids (q), entity_type (q), limit (q), offset (q) |
| DELETE | `/api/v1/checkpoints/entity/{entityId}` | `handle_delete_checkpoints_by_entity` | entityId (p), entity_type (q) |
| GET | `/api/v1/checkpoints/entity/{entityId}` | `handle_list_checkpoints_by_entity` | entityId (p), limit (q), offset (q), entity_type (q) |
| GET | `/api/v1/checkpoints/entity/{entityId}/latest` | `handle_latest_checkpoint` | entityId (p), entity_type (q) |
| GET | `/api/v1/checkpoints/entity/{entityId}/metadata` | `handle_checkpoint_entity_metadata` | entityId (p) |
| PUT | `/api/v1/checkpoints/entity/{entityId}/metadata` | `handle_set_checkpoint_entity_metadata` | entityId (p), body |
| GET | `/api/v1/checkpoints/time-range` | `handle_checkpoints_by_time_range` | workflowId (q), start (q), end (q), limit (q), offset (q) |
| DELETE | `/api/v1/checkpoints/{id}` | `handle_delete_checkpoint` | id (p) |
| GET | `/api/v1/checkpoints/{id}` | `handle_get_checkpoint` | id (p) |
| PUT | `/api/v1/checkpoints/{id}/tags` | `handle_update_checkpoint_tags` | id (p), body |
| GET | `/api/v1/executions/checkpoints/{cid}/preview` | `handle_preview_restore_checkpoint` | cid (p) |
| POST | `/api/v1/executions/checkpoints/{cid}/restore` | `post_executions_checkpoints_cid_restore` | cid (p) |
| POST | `/api/v1/executions/checkpoints/{cid}/resume` | `handle_restore_and_resume` | cid (p) |
| POST | `/api/v1/executions/{id}/checkpoints` | `post_executions_id_checkpoints` | id (p) |
| GET | `/api/v1/executions/{id}/checkpoints/chain` | `get_executions_id_checkpoints_chain` | id (p) |
| POST | `/api/v1/executions/{id}/checkpoints/gc` | `post_executions_id_checkpoints_gc` | id (p), before (q), domain (q) |
| GET | `/api/v1/file-checkpoint/approvals/pending` | `handle_list_pending_approvals` |  |
| POST | `/api/v1/file-checkpoint/approvals/{id}/approve` | `handle_approve_changes` | id (p), body |
| POST | `/api/v1/file-checkpoint/approvals/{id}/reject` | `handle_reject_changes` | id (p), body |
| GET | `/api/v1/file-checkpoint/changes` | `handle_list_changes_paged` | actor (q), path (q), start (q), end (q), limit (q), offset (q) |
| POST | `/api/v1/file-checkpoint/checkpoints` | `handle_create_checkpoint` | body |
| GET | `/api/v1/file-checkpoint/conflicts` | `handle_list_conflicts` |  |
| POST | `/api/v1/file-checkpoint/conflicts/resolve` | `handle_resolve_conflicts` | body |
| GET | `/api/v1/file-checkpoint/content` | `handle_read_content` | actor (q), path (q), version (q) |
| GET | `/api/v1/file-checkpoint/diff/actors/{a}/{b}` | `handle_diff_actors` | a (p), b (p) |
| GET | `/api/v1/file-checkpoint/diff/main/{id}` | `handle_diff_against_main` | id (p) |
| POST | `/api/v1/file-checkpoint/gc` | `handle_run_gc` | keep_recent_heads (q) |
| GET | `/api/v1/file-checkpoint/gc/stats` | `handle_store_stats` |  |
| POST | `/api/v1/file-checkpoint/index/rebuild` | `handle_rebuild_index` |  |
| GET | `/api/v1/file-checkpoint/index/status` | `handle_index_status` |  |
| POST | `/api/v1/file-checkpoint/merge-to-feature` | `handle_merge_to_feature` | body |
| POST | `/api/v1/file-checkpoint/merge-to-main` | `handle_merge_to_main` | body |
| GET | `/api/v1/file-checkpoint/partitions` | `handle_list_partitions` |  |
| GET | `/api/v1/file-checkpoint/partitions/{id}/history` | `handle_partition_history` | id (p), limit (q), offset (q) |
| POST | `/api/v1/file-checkpoint/redo/{id}` | `handle_redo_edit` | id (p) |
| POST | `/api/v1/file-checkpoint/rename` | `handle_rename_file` | body |
| POST | `/api/v1/file-checkpoint/restore` | `handle_restore_checkpoint` | body |
| POST | `/api/v1/file-checkpoint/rollback-feature` | `handle_rollback_feature` | body |
| GET | `/api/v1/file-checkpoint/sessions` | `handle_list_sessions` | limit (q), offset (q) |
| POST | `/api/v1/file-checkpoint/sessions` | `handle_begin_session` | body |
| POST | `/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}` | `handle_rollback_session` | id (p), actor (p) |
| GET | `/api/v1/file-checkpoint/state-links` | `handle_state_file_links` | ids (q) |
| POST | `/api/v1/file-checkpoint/submit` | `handle_submit_review` | body |
| GET | `/api/v1/file-checkpoint/sync/config` | `handle_sync_config` |  |
| POST | `/api/v1/file-checkpoint/sync/export` | `handle_export` | body |
| GET | `/api/v1/file-checkpoint/sync/hint` | `handle_hint` |  |
| POST | `/api/v1/file-checkpoint/sync/import` | `handle_import` |  |
| POST | `/api/v1/file-checkpoint/sync/rebase` | `handle_rebase` | body |
| GET | `/api/v1/file-checkpoint/sync/reconcile` | `handle_reconcile` |  |
| GET | `/api/v1/file-checkpoint/timeline/{id}` | `handle_file_timeline` | id (p) |
| GET | `/api/v1/file-checkpoint/tree/{id}` | `handle_list_tree` | id (p), prefix (q) |
| POST | `/api/v1/file-checkpoint/undo/apply` | `handle_apply_undo_last_input` | body |
| POST | `/api/v1/file-checkpoint/undo/preview` | `handle_preview_undo_last_input` | body |
| POST | `/api/v1/file-checkpoint/undo/{id}` | `handle_undo_edit` | id (p) |
| GET | `/api/v1/file-checkpoint/workspace/{id}` | `handle_get_actor_workspace` | id (p) |
