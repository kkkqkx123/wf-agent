# 智能体循环配置（agent-loops/）

样例文件：`config-examples/agent-loops/*.agent.example.toml`（加载器跳过 `.example.` 文件）。Agent 定义由 API 与存储管理，仓库样例不参与启动加载。

关键字段（`AgentDefinition` / `AgentConfig`）：

- `id`、`name`、`description`、`version`、`created_at`、`updated_at`
- `[config]`：
  - `profile_id`：引用 LLM profile（必须存在于已注册集合）
  - `system_prompt` / `system_prompt_template_id` / `system_prompt_template_variables`
  - `max_iterations`、`max_execution_time`、`max_retries`、`execution_timeout`、`max_pause_duration`
  - `token_limit`、`token_warning_threshold`、`enable_token_tracking`
  - `stream`、`tool_call_protocol`
  - `[[config.initial_messages]]`：`role`、`content`
- `[config.available_tools]`：`available`、`initial`、`discoverable`、`hidden`
- `[config.checkpoint]`：`enabled`、`interval_iterations`、`on_error`、`on_tool_call`、`[config.checkpoint.content]`
- `[[config.hooks]]`：`hook_type`、`condition`、`event_name`、`create_checkpoint`、`checkpoint_description`、`enabled`
- `[config.violation_policy]`：`max_violations`、`action`
- `[config.dynamic_context]`：`enabled`、`max_tokens`、`template`
- `[metadata]`：`author`、`tags`、`category`
