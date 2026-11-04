# LLM 配置（llm-providers / llm-profiles / llm-auth）

LLM 相关配置均为"一记录一文件"家族，支持 `index.json` 清单或扁平 glob；项目层按记录 `id` 覆盖 settings 层。样例文件（`*.example.*`）永不加载。

## llm-providers/

定义公开端点与连接默认值（无秘密），供 profile 通过 `provider_id` 引用。

文件：`{layer}/llm-providers/*.toml`，索引：`index.json`（`type: llm_providers`，路径如 `./*.toml`）。

关键字段（`LlmProviderDefinition`）：

- `id`、`name`、`description`
- `base_url`、`auth_type`（`native`/`bearer`/`basic`/`header`/`custom`）、`format`（`OPENAI_CHAT`/`OPENAI_RESPONSE`/`ANTHROPIC`/`GEMINI_NATIVE`/自定义）
- `api_version`
- `[model_discovery]`：`kind`（`models_endpoint` 或 `static_list`）、`path`、`json_path`，或内联 `[[model_discovery.models]]`（`id`、`name`、`context_window_size`）
- `[metadata].tags`

## llm-profiles/

定义模型档案，profile 是 LLM 调用的最小单位，agent / workflow 节点通过 `profile_id` 引用。

目录：`{layer}/llm-profiles/<group>/*.profile.toml`（如 `openai/`、`anthropic/`、`gemini-native/`），索引：`llm-profiles/index.json`（`type: llm_profiles`）。

关键字段（`LlmProfile`）：

- `id`、`name`、`format`、`model`
- `provider_id`：引用 provider 定义；显式字段优先
- `api_key`：支持 `${OPENAI_API_KEY}` 插值
- `base_url`、`auth_type`、`custom_headers`、`custom_body`、`query_params`
- `generation`：`temperature`、`max_tokens`、`thinking`、`tool_choice`、`parallel_tool_calls` 等
- `parameters`：格式特定 passthrough
- `timeout`、`max_retries`、`retry_delay`
- `tool_call_protocol`：`native` / `json_wrapped` / `json_unwrapped` / `json_schema` / `strict_json_schema`
- `stream_options.include_usage`
- `context_window_size`、`max_output_tokens`
- `proxy`、`no_proxy`
- `circuit_breaker`
- `auth`：引用凭据库（`provider_id` + `method`）
- `compat`、`identity`、`transport`、`env`
- `prompt_caching`
- `[metadata]`：`tags`、`context_budget_percent`

## llm-auth/

声明式认证（不含密钥）：端点、client id、环境变量名。驱动内置认证表面；插件可注册更丰富实现。

文件：`{layer}/llm-auth/*.auth.toml`，索引：`index.json`（`type: llm_auth`）。

关键字段（`AuthProviderConfig`）：

- `id`、`method`（`api_key` 或 `oauth`）、`display_name`、`login_label`
- `api_key` 方法：`env`（环境变量名）
- `oauth` 方法：`flow`（`device` 或 `pkce`）、`is_subscription`、端点（`device_authorization_url`、`authorize_url`、`token_url`）、`client_id`、`scope`、`redirect_uri`、`loopback`、`token_content_type`（`form`/`json`）、`preset`（内置 OAuth 预设名）
- 内置 OAuth 预设：`openai-codex`、`github-copilot`、`google-vertex`、`xai`、`kimi-coding`、`anthropic`、`meta-muse`
- `store_path`：凭据库路径（`AuthConfig` 层），默认 `~/.wf/auth.json`

## 作用总结

- `llm-providers`：连接与发现默认值，可被 profile 覆盖
- `llm-profiles`：模型能力与生成参数，agent/workflow 通过 `profile_id` 引用
- `llm-auth`：认证声明，驱动登录流程与凭据库；密钥永不写入配置文件
