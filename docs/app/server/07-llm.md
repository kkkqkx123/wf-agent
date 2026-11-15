# llm 域 HTTP 端点
> 操作数：57
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/llm/auth` | `handle_list_auth_providers` |  |
| POST | `/api/v1/llm/auth/login` | `handle_auth_login_start` | body |
| GET | `/api/v1/llm/auth/login/{id}` | `handle_auth_login_poll` | id (p) |
| POST | `/api/v1/llm/auth/login/{id}/cancel` | `handle_auth_login_cancel` | id (p) |
| POST | `/api/v1/llm/auth/login/{id}/submit` | `handle_auth_login_submit` | id (p), body |
| POST | `/api/v1/llm/auth/logout` | `handle_auth_logout` | body |
| POST | `/api/v1/llm/count-tokens` | `handle_count_tokens` | body |
| POST | `/api/v1/llm/generate` | `handle_generate` | body |
| POST | `/api/v1/llm/generate-batch` | `handle_generate_batch` | body |
| POST | `/api/v1/llm/generate-stream` | `handle_generate_stream` | body |
| GET | `/api/v1/llm/models` | `handle_list_all_models` | refresh (q) |
| DELETE | `/api/v1/llm/profile-templates` | `handle_remove_template` | name (q) |
| GET | `/api/v1/llm/profile-templates` | `handle_list_templates` |  |
| POST | `/api/v1/llm/profile-templates` | `handle_add_template` | body |
| GET | `/api/v1/llm/profile-templates/{name}` | `handle_get_template_by_name` | name (p) |
| GET | `/api/v1/llm/profiles` | `get_llm_profiles` | id (q), name (q), format (q), model (q), limit (q), offset (q) |
| POST | `/api/v1/llm/profiles` | `handle_create_profile` | body |
| GET | `/api/v1/llm/profiles/default` | `handle_get_default` |  |
| GET | `/api/v1/llm/profiles/export-all` | `handle_export_all_profiles` |  |
| POST | `/api/v1/llm/profiles/from-template` | `handle_create_from_template` | body |
| POST | `/api/v1/llm/profiles/import` | `handle_import_profile` | body |
| POST | `/api/v1/llm/profiles/import-all` | `handle_import_all_profiles` | body |
| POST | `/api/v1/llm/profiles/validate` | `handle_validate_profile` | body |
| DELETE | `/api/v1/llm/profiles/{id}` | `delete_llm_profiles_id` | id (p) |
| GET | `/api/v1/llm/profiles/{id}` | `get_llm_profiles_id` | id (p) |
| PUT | `/api/v1/llm/profiles/{id}` | `put_llm_profiles_id` | id (p), body |
| POST | `/api/v1/llm/profiles/{id}/default` | `handle_set_default` | id (p) |
| GET | `/api/v1/llm/profiles/{id}/export` | `handle_export_profile` | id (p) |
| GET | `/api/v1/llm/providers` | `handle_list_providers` |  |
| POST | `/api/v1/llm/providers` | `handle_create_provider` | body |
| DELETE | `/api/v1/llm/providers/{id}` | `handle_delete_provider` | id (p) |
| GET | `/api/v1/llm/providers/{id}` | `handle_get_provider` | id (p) |
| GET | `/api/v1/llm/providers/{id}/models` | `handle_list_models` | id (p), refresh (q) |
| GET | `/api/v1/llm/proxy-credentials` | `handle_list_proxy_credentials` |  |
| POST | `/api/v1/llm/proxy-credentials` | `handle_set_proxy_credential` | body |
| DELETE | `/api/v1/llm/proxy-credentials/{id}` | `handle_delete_proxy_credential` | id (p) |
| GET | `/api/v1/scripts` | `handle_list_scripts` | limit (q), offset (q), language (q) |
| POST | `/api/v1/scripts` | `handle_save_script` | body |
| POST | `/api/v1/scripts/execute` | `handle_execute_script` | body |
| GET | `/api/v1/scripts/search` | `handle_search_scripts` | q (q), limit (q), offset (q) |
| POST | `/api/v1/scripts/validate` | `handle_validate_script` | body |
| DELETE | `/api/v1/scripts/{id}` | `handle_delete_script` | id (p), force (q) |
| GET | `/api/v1/scripts/{id}` | `handle_get_script` | id (p) |
| PUT | `/api/v1/scripts/{id}` | `handle_update_script` | id (p), body |
| POST | `/api/v1/scripts/{id}/disable` | `handle_disable_script` | id (p) |
| POST | `/api/v1/scripts/{id}/enable` | `handle_enable_script` | id (p) |
| GET | `/api/v1/tool-registry` | `handle_list_tool_registry` | limit (q), offset (q), tool_type (q) |
| POST | `/api/v1/tool-registry` | `handle_save_tool` | body |
| GET | `/api/v1/tool-registry/stats` | `handle_tool_stats` |  |
| DELETE | `/api/v1/tool-registry/{id}` | `handle_delete_tool` | id (p), force (q) |
| GET | `/api/v1/tools` | `handle_list_tools` | limit (q), offset (q) |
| POST | `/api/v1/tools/execute` | `post_tools_execute` | body |
| GET | `/api/v1/tools/search` | `handle_search_tools` | q (q), limit (q), offset (q) |
| POST | `/api/v1/tools/validate-params` | `handle_validate_tool_params` | body |
| GET | `/api/v1/tools/{id}` | `handle_get_tool` | id (p) |
| POST | `/api/v1/tools/{id}/disable` | `handle_disable_tool` | id (p) |
| POST | `/api/v1/tools/{id}/enable` | `handle_enable_tool` | id (p) |
