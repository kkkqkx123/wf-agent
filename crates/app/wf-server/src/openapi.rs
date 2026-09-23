//! OpenAPI documentation for wf-server REST API.
//!
//! Central `ApiDoc` aggregates annotated route handlers and schemas.
//! Served at `/api-docs/openapi.json` in debug builds or with the
//! `openapi-docs` feature. The committed snapshot under
//! `apps/web-app/openapi.json` is the offline codegen source of truth.
//!
//! Success bodies use the typed envelope (`ApiEnvelope<T>`) with pagination
//! shells (`PageView<T>` / `CappedView<T>`); `data` stays generic JSON
//! (`serde_json::Value`) except for wf-server local view types, so domain
//! types do not need `ToSchema`. SSE uses `text/event-stream`, file
//! downloads use `String` with their file content type. Errors always use
//! `ErrorResponse`.

use utoipa::OpenApi;

/// Central OpenAPI document for the wf-server REST API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "wf-server API",
        version = "v1",
        description = "Workflow Agent Framework HTTP API for managing agent loops, workflows, checkpoints, templates, and LLM integrations."
    ),
    paths(
        // ── agent ──
        crate::api::agent::analysis::handle_error_records,
        crate::api::agent::analysis::handle_error_chain,
        crate::api::agent::analysis::handle_root_cause,
        crate::api::agent::analysis::handle_error_statistics,
        crate::api::agent::analysis::handle_advanced_error_statistics,
        crate::api::agent::analysis::handle_recovery_proposal,
        crate::api::agent::analysis::handle_similar_errors,
        crate::api::agent::analysis::handle_performance,
        crate::api::agent::analysis::handle_iteration_comparison,
        crate::api::agent::drafts::handle_list_drafts,
        crate::api::agent::drafts::handle_save_draft,
        crate::api::agent::drafts::handle_get_draft,
        crate::api::agent::drafts::handle_delete_draft,
        crate::api::agent::drafts::handle_promote_draft,
        crate::api::agent::drafts::handle_validate_draft,
        crate::api::agent::drafts::handle_lifecycle,
        crate::api::agent::executions::handle_agent_executions,
        crate::api::agent::executions::handle_get_agent_execution,
        crate::api::agent::executions::handle_delete_agent_execution,
        crate::api::agent::executions::handle_executions_by_definition,
        crate::api::agent::executions::handle_execution_statistics,
        crate::api::agent::executions::handle_executions_by_status,
        crate::api::agent::executions::handle_create_checkpoint,
        crate::api::agent::executions::handle_list_checkpoints,
        crate::api::agent::executions::handle_restore_checkpoint,
        crate::api::agent::executions::handle_resume_checkpoint,
        crate::api::agent::executions::handle_checkpoint_chain,
        crate::api::agent::executions::handle_delete_checkpoints,
        crate::api::agent::executions::handle_checkpoint_statistics,
        crate::api::agent::graphs::handle_decision_graph,
        crate::api::agent::graphs::handle_decision_nodes,
        crate::api::agent::graphs::handle_decision_edges,
        crate::api::agent::graphs::handle_all_paths,
        crate::api::agent::graphs::handle_graph_execution_path,
        crate::api::agent::graphs::handle_path_statistics,
        crate::api::agent::graphs::handle_critical_path,
        crate::api::agent::graphs::handle_all_alternatives,
        crate::api::agent::graphs::handle_alternative_decisions,
        crate::api::agent::graphs::handle_decision_sequence,
        crate::api::agent::graphs::handle_decisions_in_iteration,
        crate::api::agent::graphs::handle_decisions_by_type,
        crate::api::agent::graphs::handle_unexplored_alternatives,
        crate::api::agent::graphs::handle_most_promising_unexplored,
        crate::api::agent::graphs::handle_execution_path_steps,
        crate::api::agent::graphs::handle_tool_frequency,
        crate::api::agent::graphs::handle_decision_patterns,
        crate::api::agent::graphs::handle_path_efficiency,
        crate::api::agent::graphs::handle_path_probabilities,
        crate::api::agent::loops::handle_list_loops,
        crate::api::agent::loops::handle_save_loop,
        crate::api::agent::loops::handle_get_loop,
        crate::api::agent::loops::handle_update_loop,
        crate::api::agent::loops::handle_delete_loop,
        crate::api::agent::loops::handle_update_loop_status,
        crate::api::agent::loops::handle_loop_status,
        crate::api::agent::loops::handle_loop_status_transition,
        crate::api::agent::loops::handle_cleanup_completed,
        crate::api::agent::loops::handle_run_loop,
        crate::api::agent::loops::handle_stream_loop,
        crate::api::agent::loops::handle_pause_loop,
        crate::api::agent::loops::handle_resume_loop,
        crate::api::agent::loops::handle_cancel_loop,
        crate::api::agent::loops::handle_loop_summaries,
        crate::api::agent::loops::handle_loop_statistics,
        crate::api::agent::loops::handle_loop_summary,
        crate::api::agent::loops::handle_iteration_history,
        crate::api::agent::loops::handle_iteration_history_summary,
        crate::api::agent::loops::handle_loop_timeline,
        crate::api::agent::loops::handle_variable_history,
        crate::api::agent::loops::handle_loop_context_evolution,
        crate::api::agent::loops::handle_loop_execution_path,
        crate::api::agent::profiles::handle_validate_agent,
        crate::api::agent::profiles::handle_list_profiles,
        crate::api::agent::profiles::handle_save_profile,
        crate::api::agent::profiles::handle_get_profile,
        crate::api::agent::profiles::handle_update_profile,
        crate::api::agent::profiles::handle_delete_profile,
        crate::api::agent::variables::handle_recent_messages,
        crate::api::agent::variables::handle_dedupe_messages,
        crate::api::agent::variables::handle_search_messages,
        crate::api::agent::variables::handle_message_stats,
        crate::api::agent::variables::handle_conversation,
        crate::api::agent::variables::handle_list_variables,
        crate::api::agent::variables::handle_variable_stats,
        crate::api::agent::variables::handle_variable_export,
        crate::api::agent::variables::handle_get_variable,
        crate::api::agent::variables::handle_set_variable,
        crate::api::agent::variables::handle_delete_variable,
        crate::api::agent::variables::handle_batch_set_loop_variables,
        // ── workflow ──
        crate::api::workflow::approvals::handle_request_approval,
        crate::api::workflow::approvals::handle_check_approval,
        crate::api::workflow::approvals::handle_execute_tool,
        crate::api::workflow::approvals::handle_list_interactions,
        crate::api::workflow::approvals::handle_save_interaction,
        crate::api::workflow::approvals::handle_get_interaction,
        crate::api::workflow::approvals::handle_delete_interaction,
        crate::api::workflow::approvals::handle_interactions_by_execution,
        crate::api::workflow::approvals::handle_interactions_by_status,
        crate::api::workflow::approvals::handle_respond_interaction,
        crate::api::workflow::approvals::handle_interaction_stats,
        crate::api::workflow::drafts::handle_list_drafts,
        crate::api::workflow::drafts::handle_save_draft,
        crate::api::workflow::drafts::handle_get_draft,
        crate::api::workflow::drafts::handle_delete_draft,
        crate::api::workflow::drafts::handle_promote_draft,
        crate::api::workflow::drafts::handle_promote_all,
        crate::api::workflow::drafts::handle_validate_draft,
        crate::api::workflow::drafts::handle_lifecycle,
        crate::api::workflow::execution_analysis::handle_execution_graph,
        crate::api::workflow::execution_analysis::handle_execution_graph_nodes,
        crate::api::workflow::execution_analysis::handle_execution_graph_edges,
        crate::api::workflow::execution_analysis::handle_execution_graph_neighbors,
        crate::api::workflow::execution_analysis::handle_execution_path_stats,
        crate::api::workflow::execution_analysis::handle_analysis_paths,
        crate::api::workflow::execution_analysis::handle_execution_graph_reachability,
        crate::api::workflow::execution_analysis::handle_clear_execution_graph,
        crate::api::workflow::execution_analysis::handle_enumerate_paths,
        crate::api::workflow::execution_analysis::handle_decision_points,
        crate::api::workflow::execution_analysis::handle_slow_nodes,
        crate::api::workflow::execution_analysis::handle_analysis_efficiency,
        crate::api::workflow::execution_analysis::handle_analysis_alternatives,
        crate::api::workflow::execution_analysis::handle_analysis_probabilities,
        crate::api::workflow::execution_analysis::handle_execution_nodes,
        crate::api::workflow::execution_analysis::handle_node_analysis,
        crate::api::workflow::execution_analysis::handle_nodes_by_type,
        crate::api::workflow::execution_analysis::handle_node_input_context,
        crate::api::workflow::execution_analysis::handle_node_transitions,
        crate::api::workflow::execution_analysis::handle_llm_reasoning_path,
        crate::api::workflow::execution_analysis::handle_tool_chain,
        crate::api::workflow::execution_analysis::handle_execution_path,
        crate::api::workflow::execution_analysis::handle_optimizations,
        crate::api::workflow::execution_analysis::handle_node_stats,
        crate::api::workflow::execution_analysis::handle_failed_nodes,
        crate::api::workflow::execution_analysis::handle_iterations,
        crate::api::workflow::execution_state::handle_state,
        crate::api::workflow::execution_state::handle_variables,
        crate::api::workflow::execution_state::handle_transitions,
        crate::api::workflow::execution_state::handle_context,
        crate::api::workflow::execution_state::handle_call_stack,
        crate::api::workflow::execution_state::handle_memory,
        crate::api::workflow::execution_state::handle_variable_snapshots,
        crate::api::workflow::execution_state::handle_context_evolution,
        crate::api::workflow::execution_state::handle_state_analysis,
        crate::api::workflow::execution_state::handle_context_transitions,
        crate::api::workflow::execution_state::handle_key_context_snapshots,
        crate::api::workflow::execution_state::handle_agent_execution_state,
        crate::api::workflow::execution_state::handle_agent_execution_iterations,
        crate::api::workflow::execution_state::handle_agent_execution_variables,
        crate::api::workflow::execution_state::handle_state_records,
        crate::api::workflow::execution_state::handle_clear_state_records,
        crate::api::workflow::execution_state::handle_state_at_iteration,
        crate::api::workflow::execution_state::handle_variable_snapshot_at,
        crate::api::workflow::execution_state::handle_state_variable_history,
        crate::api::workflow::execution_state::handle_most_changed_variables,
        crate::api::workflow::execution_state::handle_variable_mutation_count,
        crate::api::workflow::execution_state::handle_state_call_stack,
        crate::api::workflow::execution_state::handle_state_memory,
        crate::api::workflow::execution_state::handle_state_memory_peak,
        crate::api::workflow::executions::handle_execute_workflow,
        crate::api::workflow::executions::handle_execute_stream,
        crate::api::workflow::executions::handle_list_executions,
        crate::api::workflow::executions::handle_get_execution,
        crate::api::workflow::executions::handle_delete_execution,
        crate::api::workflow::executions::handle_pause,
        crate::api::workflow::executions::handle_resume,
        crate::api::workflow::executions::handle_cancel,
        crate::api::workflow::executions::handle_status,
        crate::api::workflow::executions::handle_trigger_history,
        crate::api::workflow::graphs::handle_graph,
        crate::api::workflow::graphs::handle_graph_summary,
        crate::api::workflow::graphs::handle_graph_nodes,
        crate::api::workflow::graphs::handle_graph_edges,
        crate::api::workflow::graphs::handle_graph_neighbors,
        crate::api::workflow::graphs::handle_graph_analysis,
        crate::api::workflow::graphs::handle_graph_cycles,
        crate::api::workflow::graphs::handle_graph_topology,
        crate::api::workflow::graphs::handle_graph_reachability,
        crate::api::workflow::versions::handle_list_versions,
        crate::api::workflow::versions::handle_get_version,
        crate::api::workflow::versions::handle_save_version,
        crate::api::workflow::versions::handle_increment_version,
        crate::api::workflow::versions::handle_rollback_workflow,
        crate::api::workflow::workflows::handle_list_workflows,
        crate::api::workflow::workflows::handle_create_workflow,
        crate::api::workflow::workflows::handle_update_workflow,
        crate::api::workflow::workflows::handle_delete_workflow,
        crate::api::workflow::workflows::handle_get_workflow,
        crate::api::workflow::workflows::handle_clone_workflow,
        crate::api::workflow::workflows::handle_validate_workflow,
        crate::api::workflow::workflows::handle_validate_node,
        crate::api::workflow::workflows::handle_parse_workflow,
        crate::api::workflow::workflows::handle_transform_workflow,
        crate::api::workflow::workflows::handle_workflow_summaries,
        crate::api::workflow::workflows::handle_export_workflow,
        crate::api::workflow::workflows::handle_search_workflows,
        crate::api::workflow::workflows::handle_workflow_by_name,
        crate::api::workflow::workflows::handle_workflows_by_tags,
        crate::api::workflow::workflows::handle_workflows_by_category,
        crate::api::workflow::workflows::handle_workflows_by_author,
        crate::api::workflow::workflows::handle_export_workflows,
        crate::api::workflow::workflows::handle_import_workflow,
        crate::api::workflow::workflows::handle_import_many,
        crate::api::workflow::workflows::handle_update_metadata,
        // ── checkpoint ──
        crate::api::checkpoint::checkpoints::handle_create_checkpoint,
        crate::api::checkpoint::checkpoints::handle_checkpoint_chain,
        crate::api::checkpoint::checkpoints::handle_restore_checkpoint,
        crate::api::checkpoint::checkpoints::handle_restore_and_resume,
        crate::api::checkpoint::checkpoints::handle_list_checkpoints,
        crate::api::checkpoint::checkpoints::handle_get_checkpoint,
        crate::api::checkpoint::checkpoints::handle_delete_checkpoint,
        crate::api::checkpoint::checkpoints::handle_list_checkpoints_by_entity,
        crate::api::checkpoint::checkpoints::handle_latest_checkpoint,
        crate::api::checkpoint::checkpoints::handle_delete_checkpoints_by_entity,
        crate::api::checkpoint::checkpoints::handle_checkpoint_entity_metadata,
        crate::api::checkpoint::checkpoints::handle_set_checkpoint_entity_metadata,
        crate::api::checkpoint::checkpoints::handle_list_checkpoints_by_entities,
        crate::api::checkpoint::checkpoints::handle_checkpoints_by_time_range,
        crate::api::checkpoint::file_approvals::handle_list_pending_approvals,
        crate::api::checkpoint::file_approvals::handle_approve_changes,
        crate::api::checkpoint::file_approvals::handle_reject_changes,
        crate::api::checkpoint::file_provenance::handle_list_partitions,
        crate::api::checkpoint::file_provenance::handle_get_actor_workspace,
        crate::api::checkpoint::file_provenance::handle_diff_actors,
        crate::api::checkpoint::file_provenance::handle_diff_against_staged,
        crate::api::checkpoint::file_provenance::handle_file_timeline,
        crate::api::checkpoint::file_provenance::handle_read_content,
        crate::api::checkpoint::file_provenance::handle_list_tree,
        crate::api::checkpoint::file_provenance::handle_list_changes_paged,
        crate::api::checkpoint::file_provenance::handle_begin_session,
        crate::api::checkpoint::file_provenance::handle_list_sessions,
        crate::api::checkpoint::file_provenance::handle_rollback_session,
        crate::api::checkpoint::file_provenance::handle_undo_edit,
        crate::api::checkpoint::file_provenance::handle_redo_edit,
        crate::api::checkpoint::file_provenance::handle_rename_file,
        crate::api::checkpoint::file_provenance::handle_run_gc,
        // ── trigger ──
        crate::api::trigger::executions::handle_list_trigger_executions,
        crate::api::trigger::executions::handle_save_trigger_execution,
        crate::api::trigger::executions::handle_get_trigger_execution,
        crate::api::trigger::executions::handle_delete_trigger_execution,
        crate::api::trigger::executions::handle_trigger_execution_stats,
        crate::api::trigger::executions::handle_trigger_executions_by_trigger,
        crate::api::trigger::executions::handle_trigger_executions_by_execution,
        crate::api::trigger::executions::handle_cleanup_trigger_executions,
        crate::api::trigger::executions::handle_trigger_executions_by_workflow,
        crate::api::trigger::executions::handle_trigger_history,
        crate::api::trigger::hooks::handle_webhook_fire,
        // ── web ──
        crate::api::web::batch::handle_batch_cancel,
        crate::api::web::batch::handle_batch_delete,
        crate::api::web::batch::handle_batch_delete_loops,
        crate::api::web::batch::handle_batch_respond,
        crate::api::web::favorites::handle_list_favorites,
        crate::api::web::favorites::handle_upsert_favorite,
        crate::api::web::favorites::handle_delete_favorite,
        crate::api::web::preferences::handle_get_preferences,
        crate::api::web::preferences::handle_replace_preferences,
        crate::api::web::preferences::handle_get_preference,
        crate::api::web::preferences::handle_set_preference,
        crate::api::web::preferences::handle_delete_preference,
        // ── llm ──
        crate::api::llm::llm::handle_generate,
        crate::api::llm::llm::handle_generate_batch,
        crate::api::llm::llm::handle_generate_stream,
        crate::api::llm::llm::handle_count_tokens,
        crate::api::llm::llm::handle_list_profiles,
        crate::api::llm::llm::handle_create_profile,
        crate::api::llm::llm::handle_get_profile,
        crate::api::llm::llm::handle_update_profile,
        crate::api::llm::llm::handle_delete_profile,
        crate::api::llm::llm::handle_set_default,
        crate::api::llm::llm::handle_get_default,
        crate::api::llm::llm::handle_export_profile,
        crate::api::llm::llm::handle_import_profile,
        crate::api::llm::llm::handle_export_all_profiles,
        crate::api::llm::llm::handle_import_all_profiles,
        crate::api::llm::llm::handle_list_templates,
        crate::api::llm::llm::handle_add_template,
        crate::api::llm::llm::handle_remove_template,
        crate::api::llm::llm::handle_get_template_by_name,
        crate::api::llm::llm::handle_validate_profile,
        crate::api::llm::llm::handle_create_from_template,
        crate::api::llm::llm::handle_list_providers,
        crate::api::llm::llm::handle_create_provider,
        crate::api::llm::llm::handle_get_provider,
        crate::api::llm::llm::handle_delete_provider,
        crate::api::llm::llm::handle_list_models,
        crate::api::llm::scripts::handle_execute_script,
        crate::api::llm::scripts::handle_validate_script,
        crate::api::llm::scripts::handle_list_scripts,
        crate::api::llm::scripts::handle_save_script,
        crate::api::llm::scripts::handle_search_scripts,
        crate::api::llm::scripts::handle_get_script,
        crate::api::llm::scripts::handle_update_script,
        crate::api::llm::scripts::handle_delete_script,
        crate::api::llm::scripts::handle_enable_script,
        crate::api::llm::scripts::handle_disable_script,
        crate::api::llm::tools::handle_list_tools,
        crate::api::llm::tools::handle_search_tools,
        crate::api::llm::tools::handle_execute_tool,
        crate::api::llm::tools::handle_validate_tool_params,
        crate::api::llm::tools::handle_get_tool,
        crate::api::llm::tools::handle_enable_tool,
        crate::api::llm::tools::handle_disable_tool,
        crate::api::llm::tools::handle_list_tool_registry,
        crate::api::llm::tools::handle_save_tool,
        crate::api::llm::tools::handle_delete_tool,
        crate::api::llm::tools::handle_tool_stats,
        // ── template ──
        crate::api::template::library::handle_query_library,
        crate::api::template::library::handle_library_featured,
        crate::api::template::library::handle_library_popular,
        crate::api::template::library::handle_record_usage,
        crate::api::template::library::handle_clone_template,
        crate::api::template::library::handle_list_workflow_templates,
        crate::api::template::library::handle_get_workflow_template,
        crate::api::template::library::handle_register_workflow_template,
        crate::api::template::library::handle_delete_workflow_template,
        crate::api::template::library::handle_list_agent_templates,
        crate::api::template::library::handle_get_agent_template,
        crate::api::template::library::handle_register_agent_template,
        crate::api::template::library::handle_delete_agent_template,
        crate::api::template::queries::handle_query_agent_trigger_templates,
        crate::api::template::queries::handle_agent_trigger_summaries,
        crate::api::template::queries::handle_query_agent_templates,
        crate::api::template::queries::handle_agent_template_summaries,
        crate::api::template::queries::handle_agent_template_featured,
        crate::api::template::queries::handle_agent_template_popular,
        crate::api::template::templates::handle_list_node_templates,
        crate::api::template::templates::handle_save_node_template,
        crate::api::template::templates::handle_get_node_template,
        crate::api::template::templates::handle_update_node_template,
        crate::api::template::templates::handle_delete_node_template,
        crate::api::template::templates::handle_export_node_template,
        crate::api::template::templates::handle_import_node_template,
        crate::api::template::templates::handle_list_trigger_templates,
        crate::api::template::templates::handle_save_trigger_template,
        crate::api::template::templates::handle_get_trigger_template,
        crate::api::template::templates::handle_update_trigger_template,
        crate::api::template::templates::handle_delete_trigger_template,
        crate::api::template::templates::handle_export_trigger_template,
        crate::api::template::templates::handle_import_trigger_template,
        // ── entity ──
        crate::api::entity::interactions::handle_list_interactions,
        crate::api::entity::interactions::handle_get_interaction,
        crate::api::entity::interactions::handle_respond_interaction,
        crate::api::entity::messages::handle_list_messages,
        crate::api::entity::messages::handle_save_message,
        crate::api::entity::messages::handle_get_message,
        crate::api::entity::messages::handle_delete_message,
        crate::api::entity::messages::handle_search_messages,
        crate::api::entity::messages::handle_message_stats,
        crate::api::entity::messages::handle_messages_by_execution,
        crate::api::entity::messages::handle_conversation,
        crate::api::entity::skills::handle_list_skills,
        crate::api::entity::skills::handle_query_skills,
        crate::api::entity::skills::handle_get_skill,
        crate::api::entity::skills::handle_enable_skill,
        crate::api::entity::skills::handle_enabled_skills,
        crate::api::entity::skills::handle_disabled_skills,
        crate::api::entity::skills::handle_clear_skill_cache,
        crate::api::entity::skills::handle_clear_skill_cache_by_name,
        crate::api::entity::skills::handle_skill_content,
        crate::api::entity::skills::handle_disable_skill,
        crate::api::entity::skills::handle_scan_skills,
        crate::api::entity::skills::handle_reload_skills,
        crate::api::entity::skills::handle_skill_resources,
        crate::api::entity::skills::handle_skill_prompt,
        crate::api::entity::tasks::handle_list_tasks,
        crate::api::entity::tasks::handle_save_task,
        crate::api::entity::tasks::handle_get_task,
        crate::api::entity::tasks::handle_delete_task,
        crate::api::entity::tasks::handle_task_stats,
        crate::api::entity::tasks::handle_cancel_task,
        crate::api::entity::tasks::handle_tasks_by_execution,
        crate::api::entity::tasks::handle_cleanup_tasks,
        crate::api::entity::variables::handle_list_variables,
        crate::api::entity::variables::handle_set_variable,
        crate::api::entity::variables::handle_get_variable,
        crate::api::entity::variables::handle_delete_variable,
        crate::api::entity::variables::handle_variable_stats,
        crate::api::entity::variables::handle_batch_set_variables,
        crate::api::entity::variables::handle_import_variables,
        crate::api::entity::variables::handle_variable_scopes,
        crate::api::entity::variables::handle_variables_by_scope,
        crate::api::entity::variables::handle_variables_at_node,
        crate::api::entity::variables::handle_variable_export,
        crate::api::entity::variables::handle_variable_history,
        // ── observation ──
        crate::api::observation::analysis::handle_progress,
        crate::api::observation::analysis::handle_search,
        crate::api::observation::analysis::handle_llm_metrics,
        crate::api::observation::analysis::handle_performance_compare,
        crate::api::observation::analysis::handle_stats,
        crate::api::observation::analysis::handle_top_workflows,
        crate::api::observation::analysis::handle_top_node_types,
        crate::api::observation::analysis::handle_agent_stats_by_profile,
        crate::api::observation::analysis::handle_error_analysis,
        crate::api::observation::analysis::handle_error_analysis_advanced,
        crate::api::observation::analysis::handle_error_root_cause,
        crate::api::observation::analysis::handle_error_context,
        crate::api::observation::analysis::handle_error_context_one,
        crate::api::observation::analysis::handle_error_recovery_recommendations,
        crate::api::observation::analysis::handle_error_recovery,
        crate::api::observation::analysis::handle_error_similar,
        crate::api::observation::analysis::handle_error_chain_stream,
        crate::api::observation::analysis::handle_performance,
        crate::api::observation::analysis::handle_performance_summary,
        crate::api::observation::analysis::handle_performance_bottlenecks,
        crate::api::observation::analysis::handle_iteration_comparison,
        crate::api::observation::audit::handle_audit_summary,
        crate::api::observation::audit::handle_audit_report,
        crate::api::observation::audit::handle_audit_timeline,
        crate::api::observation::audit::handle_audit_iterations,
        crate::api::observation::audit::handle_audit_tool_calls,
        crate::api::observation::audit::handle_audit_llm_calls,
        crate::api::observation::audit::handle_audit_node_executions,
        crate::api::observation::query::handle_query,
        crate::api::observation::query::handle_export,
        crate::api::observation::query::handle_aggregate,
        crate::api::observation::query::handle_distinct,
        crate::api::observation::query::handle_group_by,
        crate::api::observation::query::handle_evaluate,
        // ── system ──
        crate::api::system::dependencies::handle_dependents,
        crate::api::system::dependencies::handle_impact,
        crate::api::system::dependencies::handle_audit,
        crate::api::system::dependencies::handle_stale,
        crate::api::system::events::handle_list_events,
        crate::api::system::events::handle_clear_events,
        crate::api::system::events::handle_event_stats,
        crate::api::system::events::handle_search_events,
        crate::api::system::events::handle_event_size,
        crate::api::system::events::handle_event_time_range,
        crate::api::system::events::handle_execution_timeline,
        crate::api::system::events::handle_agent_timeline,
        crate::api::system::events::handle_execution_timeline_view,
        crate::api::system::events::handle_execution_timeline_summary,
        crate::api::system::events::handle_listener_stats,
        crate::api::system::events::handle_agent_loop_statistics,
        crate::api::system::events::handle_agent_events,
        crate::api::system::events::handle_agent_turn_events,
        crate::api::system::events::handle_agent_tool_execution_events,
        crate::api::system::events::handle_event_stream,
        crate::api::system::health::handle_root,
        crate::api::system::health::handle_health,
        crate::api::system::health::handle_info,
        crate::api::system::health::handle_storage_diagnose,
        crate::api::system::health::handle_storage_health,
        crate::api::system::health::handle_storage_stats,
        crate::api::system::health::handle_diagnostics,
        crate::api::system::health::handle_event_health,
        crate::api::system::metrics::handle_workflow,
        crate::api::system::metrics::handle_node_templates,
        crate::api::system::metrics::handle_agents,
        crate::api::system::metrics::handle_report,
        crate::api::system::metrics::handle_export,
        crate::api::system::metrics::handle_collectors,
        crate::metrics::handle_metrics,

    ),
    components(schemas(
        crate::envelope::ApiErrorBody,
        crate::envelope::ErrorResponse,
        crate::envelope::ApiEnvelope<serde_json::Value>,
        crate::envelope::ApiEnvelope<String>,
        crate::envelope::ApiEnvelope<bool>,
        crate::envelope::ApiEnvelope<usize>,
        crate::paged::PageView<serde_json::Value>,
        crate::paged::CappedView<serde_json::Value>,
        crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>,
        crate::envelope::ApiEnvelope<crate::paged::CappedView<serde_json::Value>>,
        crate::api::workflow::executions::ExecuteView,
        crate::envelope::ApiEnvelope<crate::api::workflow::executions::ExecuteView>,
        crate::api::agent::loops::AgentRunView,
        crate::envelope::ApiEnvelope<crate::api::agent::loops::AgentRunView>,
        crate::api::agent::executions::AgentResumeView,
        crate::envelope::ApiEnvelope<crate::api::agent::executions::AgentResumeView>,
        crate::api::trigger::hooks::FireResponse,
        crate::envelope::ApiEnvelope<crate::api::trigger::hooks::FireResponse>,
        crate::api::checkpoint::file_approvals::RejectResponse,
        crate::envelope::ApiEnvelope<crate::api::checkpoint::file_approvals::RejectResponse>,
        crate::api::agent::graphs::CappedPaths,
        crate::envelope::ApiEnvelope<crate::api::agent::graphs::CappedPaths>,
        crate::api::system::health::HealthView,
        crate::api::entity::interactions::AgentRespondBody,
        crate::api::observation::query::AggregateBody,
        crate::api::workflow::approvals::ApprovalCheckBody,
        crate::api::workflow::approvals::ApprovalRequestBody,
        crate::api::checkpoint::file_approvals::ApproveRequest,
        crate::api::web::batch::BatchRespondBody,
        crate::api::entity::variables::BatchSetVariablesBody,
        crate::api::checkpoint::file_provenance::BeginSessionRequest,
        crate::api::entity::tasks::CleanupTasksBody,
        crate::api::trigger::executions::CleanupTriggerExecutionsBody,
        crate::api::workflow::workflows::CloneBody,
        crate::api::template::library::CloneTemplateBody,
        crate::api::agent::executions::CreateCheckpointBody,
        crate::api::llm::llm::CreateFromTemplateBody,
        crate::api::observation::query::EvaluateBody,
        crate::api::workflow::executions::ExecuteBody,
        crate::api::workflow::approvals::ExecuteToolBody,
        crate::api::observation::query::ExportBody,
        crate::api::workflow::workflows::ExportManyBody,
        crate::api::observation::query::GroupByBody,
        crate::api::web::batch::IdsBody,
        crate::api::workflow::workflows::ImportBody,
        crate::api::llm::llm::ImportProfileBody,
        crate::api::entity::variables::ImportVariablesBody,
        crate::api::workflow::versions::IncrementVersionBody,
        crate::api::agent::variables::LoopVariableBatchBody,
        crate::api::workflow::workflows::ParseWorkflowBody,
        crate::api::observation::query::QueryBody,
        crate::api::checkpoint::file_approvals::RejectRequest,
        crate::api::checkpoint::file_provenance::RenameFileRequest,
        crate::api::web::preferences::ReplacePreferencesBody,
        crate::api::workflow::approvals::RespondBody,
        crate::api::agent::executions::ResumeCheckpointBody,
        crate::api::workflow::versions::RollbackBody,
        crate::api::agent::loops::RunAgentLoopBody,
        crate::api::workflow::versions::SaveVersionBody,
        crate::api::llm::scripts::ScriptExecuteBody,
        crate::api::web::preferences::SetPreferenceBody,
        crate::api::agent::variables::SetVariableBody,
        crate::api::workflow::workflows::TransformWorkflowBody,
        crate::api::agent::loops::UpdateLoopStatusBody,
        crate::api::web::favorites::UpsertFavoriteBody,
        crate::api::workflow::workflows::ValidateNodeBody,
        crate::api::llm::tools::ValidateToolParamsBody,
        crate::api::entity::variables::VariableBody,
    )),
    tags(
        (name = "agent", description = "Agent loop management: CRUD, execution control, status, variables, and analysis"),
        (name = "workflow", description = "Workflow definition, versioning, execution, graph analysis, and approvals"),
        (name = "checkpoint", description = "Execution checkpoints and file workspace provenance"),
        (name = "trigger", description = "Trigger-led execution and webhook hooks"),
        (name = "template", description = "Template management, rendering, and query library"),
        (name = "llm", description = "LLM profiles, providers, scripts, tools, and generation"),
        (name = "entity", description = "Low-level entity storage: messages, tasks, variables, skills, interactions"),
        (name = "observation", description = "Query, audit, and analysis over executions"),
        (name = "system", description = "Health, events, dependencies, and metrics"),
        (name = "web", description = "User preferences, favorites, and batch operations"),
    ),
    servers(
        (url = "/", description = "Server root; paths include the /api/v1 prefix where applicable")
    ),
    security(
        ("api_key" = [])
    ),
    modifiers(&SecurityAddon, &GlobalErrorResponses)
)]
pub struct ApiDoc;

/// Security scheme modifier for API key authentication.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            let value = utoipa::openapi::security::ApiKeyValue::with_description(
                "x-api-key",
                "API key via X-API-Key header or api_key query parameter",
            );
            components.add_security_scheme(
                "api_key",
                utoipa::openapi::security::SecurityScheme::ApiKey(
                    utoipa::openapi::security::ApiKey::Header(value),
                ),
            );
        }
    }
}

/// Injects middleware-level error responses into every operation so auth,
/// rate-limit, overload and timeout failures are visible to codegen without
/// repeating them in 400+ annotations.
struct GlobalErrorResponses;

impl utoipa::Modify for GlobalErrorResponses {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        const EXTRA: [(&str, &str); 5] = [
            ("401", "Unauthorized: missing or invalid API key"),
            ("403", "Forbidden: API key lacks access"),
            (
                "429",
                "Too many requests: rate limit exceeded (see Retry-After)",
            ),
            ("503", "Service unavailable: resource limit reached"),
            ("504", "Gateway timeout: upstream operation timed out"),
        ];
        for item in openapi.paths.paths.values_mut() {
            for operation in path_item_operations(item) {
                for (status, description) in EXTRA {
                    operation
                        .responses
                        .responses
                        .entry(status.to_string())
                        .or_insert_with(|| {
                            utoipa::openapi::RefOr::T(utoipa::openapi::Response::new(description))
                        });
                }
            }
        }
    }
}

fn path_item_operations(
    item: &mut utoipa::openapi::path::PathItem,
) -> Vec<&mut utoipa::openapi::path::Operation> {
    let mut ops = Vec::new();
    for slot in [
        &mut item.get,
        &mut item.put,
        &mut item.post,
        &mut item.delete,
        &mut item.options,
        &mut item.head,
        &mut item.patch,
        &mut item.trace,
    ] {
        if let Some(operation) = slot.as_mut() {
            ops.push(operation);
        }
    }
    ops
}

/// Serve the generated OpenAPI document as JSON. Mounted at
/// `/api-docs/openapi.json` in dev/debug builds or with the
/// `openapi-docs` feature. Offline codegen reads the committed snapshot
/// instead of this route.
pub async fn serve_openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}

/// Absolute path to the committed OpenAPI snapshot consumed by web-app codegen.
#[cfg(test)]
fn snapshot_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../apps/web-app/openapi.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    const HTTP_METHODS: [&str; 8] = [
        "get", "put", "post", "patch", "delete", "head", "options", "trace",
    ];

    fn doc() -> serde_json::Value {
        serde_json::to_value(ApiDoc::openapi()).expect("ApiDoc must serialize to JSON")
    }

    #[test]
    fn document_is_valid_openapi() {
        let v = doc();
        assert_eq!(v["openapi"].as_str().unwrap(), "3.1.0");
        assert_eq!(v["info"]["title"].as_str().unwrap(), "wf-server API");
        assert!(
            v["paths"].as_object().unwrap().len() > 300,
            "expected the full REST surface"
        );
    }

    #[test]
    fn every_operation_declares_a_success_response() {
        let v = doc();
        let mut ops = 0usize;
        for (_path, item) in v["paths"].as_object().unwrap() {
            for method in HTTP_METHODS {
                if let Some(op) = item.get(method) {
                    ops += 1;
                    assert!(
                        op["responses"].as_object().unwrap().contains_key("200"),
                        "operation {method} on {_path} is missing a 200 response"
                    );
                }
            }
        }
        assert_eq!(ops, 452, "one operation per annotated handler");
    }

    #[test]
    fn global_error_responses_are_declared() {
        let v = doc();
        let mut sample = None;
        for (_path, item) in v["paths"].as_object().unwrap() {
            if let Some(op) = item.get("get") {
                sample = Some(op);
                break;
            }
        }
        let op = sample.expect("at least one GET operation");
        for status in ["401", "403", "429", "503", "504"] {
            assert!(
                op["responses"].as_object().unwrap().contains_key(status),
                "missing global {status} response"
            );
        }
    }

    #[test]
    fn api_key_security_scheme_is_registered() {
        let v = doc();
        let schemes = &v["components"]["securitySchemes"];
        assert_eq!(schemes["api_key"]["type"].as_str().unwrap(), "apiKey");
        assert_eq!(schemes["api_key"]["in"].as_str().unwrap(), "header");
        assert_eq!(schemes["api_key"]["name"].as_str().unwrap(), "x-api-key");
    }

    #[test]
    fn committed_snapshot_matches_document() {
        let actual =
            serde_json::to_string_pretty(&ApiDoc::openapi()).expect("ApiDoc must serialize") + "\n";
        let path = snapshot_path();
        if std::env::var_os("WF_REFRESH_OPENAPI").is_some() {
            std::fs::create_dir_all(path.parent().expect("snapshot parent"))
                .expect("create snapshot dir");
            std::fs::write(&path, &actual).expect("write openapi snapshot");
            return;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing OpenAPI snapshot at {}: {e}; run with WF_REFRESH_OPENAPI=1 to create it",
                path.display()
            )
        });
        assert_eq!(
            actual,
            expected,
            "OpenAPI snapshot drifted; run with WF_REFRESH_OPENAPI=1 to refresh apps/web-app/openapi.json"
        );
    }

    /// Source-level parity between axum `.route` tables and `#[utoipa::path]`
    /// annotations. Catches handlers wired only on the router (or annotated
    /// only) that the paths-registration count test cannot see. WebSocket and
    /// docs routes are intentionally outside the OpenAPI surface.
    #[test]
    fn routes_match_utoipa_annotations() {
        use std::collections::BTreeSet;

        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut annotated: BTreeSet<(String, String)> = BTreeSet::new();
        let mut routed: BTreeSet<(String, String)> = BTreeSet::new();

        for entry in walk_rs(&src_root) {
            let rel = entry
                .strip_prefix(&src_root)
                .expect("source under src root")
                .to_string_lossy()
                .replace('\\', "/");
            if should_skip_route_file(&rel) {
                continue;
            }
            let text = std::fs::read_to_string(&entry).expect("read source file");
            for (method, path) in parse_utoipa_annotations(&text) {
                annotated.insert((method, path));
            }
            let prefix = route_prefix_for(&rel);
            for (method, path) in parse_axum_routes(&text) {
                let full = if rel == "api/system/health.rs" || rel == "metrics.rs" {
                    path
                } else {
                    format!("{prefix}{path}")
                };
                routed.insert((method, full));
            }
        }

        let only_annotation: Vec<_> = annotated.difference(&routed).collect();
        let only_route: Vec<_> = routed.difference(&annotated).collect();
        assert!(
            only_annotation.is_empty() && only_route.is_empty(),
            "route/annotation drift\nonly in annotations: {only_annotation:?}\nonly in routes: {only_route:?}"
        );
        assert!(!annotated.is_empty(), "expected a non-empty annotation set");
    }

    fn walk_rs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read source dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_rs(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    fn should_skip_route_file(rel: &str) -> bool {
        matches!(rel, "ws.rs" | "middleware.rs" | "router.rs" | "main.rs")
            || (!rel.starts_with("api/") && rel != "metrics.rs")
    }

    fn route_prefix_for(rel: &str) -> &'static str {
        if rel == "api/system/metrics.rs" {
            "/api/v1/metrics"
        } else {
            "/api/v1"
        }
    }

    fn parse_utoipa_annotations(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(pos) = rest.find("#[utoipa::path(") {
            let after = &rest[pos + "#[utoipa::path(".len()..];
            let window = &after[..after.len().min(400)];
            if let Some((method, path)) = parse_annotation_head(window) {
                out.push((method, path));
            }
            rest = &after[1..];
        }
        out
    }

    fn parse_annotation_head(window: &str) -> Option<(String, String)> {
        let methods = [
            "get", "put", "post", "patch", "delete", "head", "options", "trace",
        ];
        let lower = window.to_ascii_lowercase();
        let trimmed = lower.trim_start();
        let mut method = None;
        for m in methods {
            if let Some(after) = trimmed.strip_prefix(m) {
                if after.trim_start().starts_with(',') {
                    method = Some(m.to_uppercase());
                    break;
                }
            }
        }
        let method = method?;
        let ppos = lower.find("path")?;
        let q1 = window[ppos..].find('=')? + ppos;
        let q2 = window[q1 + 1..].find('"')? + q1 + 1;
        let q3 = window[q2 + 1..].find('"')? + q2 + 1;
        let path = window[q2 + 1..q3].to_string();
        Some((method, path))
    }

    fn parse_axum_routes(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let positions: Vec<usize> = text.match_indices(".route(").map(|(i, _)| i).collect();
        for (idx, &pos) in positions.iter().enumerate() {
            let after = &text[pos + ".route(".len()..];
            let body_len = match positions.get(idx + 1) {
                Some(&next) => next - (pos + ".route(".len()),
                None => cut_routes_tail(after),
            };
            let body = &after[..body_len.min(after.len())];
            let Some(path) = first_string_literal(body) else {
                continue;
            };
            for method in parse_route_methods(body) {
                out.push((method, path.clone()));
            }
        }
        out
    }

    /// Last `.route` segment: stop before the next top-level item in the file.
    fn cut_routes_tail(seg: &str) -> usize {
        let mut end = seg.len();
        for pat in ["\nfn ", "\npub ", "\nasync ", "\n#["] {
            if let Some(i) = seg.find(pat) {
                end = end.min(i);
            }
        }
        end
    }

    fn first_string_literal(s: &str) -> Option<String> {
        let start = s.find('"')?;
        let end = s[start + 1..].find('"')? + start + 1;
        Some(s[start + 1..end].to_string())
    }

    fn parse_route_methods(body: &str) -> Vec<String> {
        const METHODS: [&str; 8] = [
            "get", "put", "post", "patch", "delete", "head", "options", "trace",
        ];
        let lower = body.to_ascii_lowercase();
        let mut out = Vec::new();
        let mut search = 0usize;
        while search < lower.len() {
            let Some(found) = lower[search..].find(|c: char| c.is_ascii_alphabetic() || c == '_')
            else {
                break;
            };
            let start = search + found;
            let mut end = start;
            while end < lower.len()
                && (lower.as_bytes()[end].is_ascii_alphanumeric() || lower.as_bytes()[end] == b'_')
            {
                end += 1;
            }
            let word = &lower[start..end];
            search = end.max(start + 1);
            if METHODS.contains(&word) && lower[end..].trim_start().starts_with('(') {
                out.push(word.to_uppercase());
            }
        }
        out
    }
}
