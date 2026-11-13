# 基础设施配置（infrastructure/）

基础设施配置位于 `{layer}/infrastructure/`，共九个固定文件名。由 `ConfigOrchestrator` 按优先级链加载并合并，随后应用 `WF_*` 环境覆盖与程序化覆盖。

| 文件 | 配置类型 | 作用 |
|---|---|---|
| `storage.toml` | `StorageConfig` | 工作流 / 检查点 / 任务持久化后端 |
| `metrics.toml` | `MetricsConfig` | 指标采集器、保留窗口、异常阈值、导出地址 |
| `output.toml` | `OutputConfig` | 输出目录、日志文件模式、SDK 日志 |
| `sandbox.toml` | `SandboxGlobalConfig` | 脚本沙箱 profile 与路由规则 |
| `file-checkpoint.toml` | `FileCheckpointConfig` | 工作区文件快照与恢复 |
| `tool-approval.toml` | `ToolApprovalConfig` | 工具级审批策略 |
| `presets.toml` | `PresetsConfig` | 预定义工具 / 提示词启用 |
| `tools.toml` | `ToolConfigs` | 文件工具与 shell / code-context 覆盖 |
| `limits.toml` | `LimitsConfig` | agent / workflow / 执行 / 压缩限制 |

## storage.toml

- `type`: `sqlite` | `postgres` | `memory`
- `app_name`: 默认数据库路径派生使用的应用名
- `[sqlite]`: `db_path`、`enable_wal`、`enable_logging`、`readonly`、`file_must_exist`、`timeout`（ms）、`auto_vacuum`（`NONE`/`FULL`/`INCREMENTAL`）、`journal_size_limit`、`page_size`、`maintenance_interval_ms`
- `[postgres]`: `host`、`port`（默认 5432）、`username`、`password`（支持 `${DB_PASSWORD}`）、`database`、`ssl`、`pool_size`、`min_connections`、`idle_timeout`、`connection_timeout`、`max_uses`
- 环境覆盖：`WF_STORAGE_TYPE`、`WF_STORAGE_SQLITE_DB_PATH`

## metrics.toml

- 全局：`enabled`、`enable_periodic_reporting`、`reporting_interval`、`retention_ms`（默认 3600000）、`http_addr`（可选导出服务）
- `[anomaly_thresholds]`：`max_error_count`、`min_success_rate`、`max_tool_error_rate`、`max_checkpoint_failures`
- 每个采集器段（如 `[workflow_metrics]`）：`buffer_size`、`flush_interval`、`enabled`、`retention_ms`、`strict_labels`、`allowed_label_keys`
- 采集器包括：workflow、node、agent、event、tool、token、config、error、resource、agent_loop、subgraph、template、retry_budget、timeout、checkpoint、compaction、http
- 环境覆盖：`WF_METRICS_ENABLED`

## output.toml

- `dir`：基础输出目录（默认 `./outputs`）
- `log_file_pattern`：日志文件命名模式（默认 `app-{date}.log`）
- `enable_log_terminal`：是否同时写终端
- `enable_sdk_logs`：是否包含 SDK 内部日志
- `sdk_log_level`：`silent` | `error` | `warn` | `info` | `debug`
- 环境覆盖：`WF_OUTPUT_DIR`

## sandbox.toml

全局沙箱配置，fail-fast 校验。

- `mode`：`Disabled` | `Lenient` | `Strict`
- `audit_logging`：是否记录审计事件
- `default_profile`：无规则匹配时使用的 profile
- `[[profiles]]`：命名 profile，含 `name`、`description`、`mode`、各语言策略链（`shell_strategy` 等）与 `[profiles.policy.*]`
- `[profiles.policy.shell]`：`denied_commands`、`dangerous_patterns`、`read_commands`、`write_commands`
- `[profiles.policy.python]` / `javascript` / `lua`：模块允许/拒绝列表、Lua 能力开关
- `[profiles.policy.filesystem]`：允许读写删执行路径、`copy_on_write`、`max_file_size`
- `[profiles.policy.process]`：子进程、fork/exec、syscall allowlist
- `[profiles.policy.network]`：`access`、域名/端口、DNS
- `[profiles.policy.resource]`：CPU / 内存 / 磁盘 / 超时限制
- `[[rules]]`：`match_field`（`language` 或 `script_name`）、`match_pattern`（glob）、`profile`
- 环境覆盖：`WF_SANDBOX_MODE`

## file-checkpoint.toml

- `enabled`：是否启用工作区文件快照
- `workspace_root`：覆盖工作区根目录
- `custom_ignore_patterns`：追加忽略模式
- `[storage]`：`type`（仅 `sqlite`）、`db_path`
- `failure_behavior`：`warn` | `error` | `ignore`
- `approval_policy`：`none` | `auto` | `llm` | `manual`
- `conflict_behavior`：`marker` | `fail` | `approval`
- `manual_watch`、`manual_poll_ms`：手动 watcher
- `gc_interval_secs`、`gc_retention.keep_recent_heads`：物理 GC
- `default_scope`：无显式 scope 时的默认执行文件范围（`enabled`、`read_prefixes`、`write_prefixes`、`checkpoint_prefixes`、`workdir`；缺失继承，显式空列表拒绝全部）
- `store_in_workspace`：对象库存放在工作区 `.wf-checkpoint-git`
- `max_file_size`：单文件内容上限（字节）
- `scope_serialization`：shell 采样窗口串行化（`enabled`、`lock_timeout_ms`）
- `baseline`：基线选择（`mode`、`branch`、`commit_prefix`、`behind`、`tag`、`failure_behavior`）
- `sync`：Git 同步（`auto_commit`、`auto_import`）
- 环境覆盖：`WF_FILE_CHECKPOINT_*`（enabled、workspace_root、manual_watch、manual_poll_ms、gc_interval_secs）

## tool-approval.toml

- `enabled`：是否启用宿主默认工具级审批
- `[options]`：可选策略覆盖；未设置字段继承 balanced 基线
- 覆盖字段包括：`security_preset`、`categories`（`always_allow_read_only/write/execute/mcp/network/interaction`）、`auto_approve_patterns`、`file_permissions`、`command`、`mcp`、`network`、`allow_write_protected`
- 空 `[options]` 仍解析为基线，不会退化为全部询问

## presets.toml

- `[predefined_tools]`：`enabled`、可选 `tools` 列表
- `[predefined_prompts]`：`enabled`、可选 `prompts` 列表

## tools.toml

- `[read_file]`：`max_file_size`、`max_chars`、`max_lines`、`enable_ignore`、`enable_protect`、`model_id`
- `[glob]`：`workspace_dir`、`max_results`、`enable_ignore`
- `[list_files]`：`workspace_dir`、`max_results`、`enable_ignore`
- `[shell]`：`workspace_dir`、`max_timeout_ms`、`allowed_commands`、`denied_commands`、`read_commands`、`write_commands`
- `[code_context]`：代码上下文服务配置（文件折叠与检索工具）。主开关 `enabled`；传输 `transport_mode`（`external` 走 `base_url`，`managed` 拉起 `managed_binary` 并用 `managed_port`/`managed_startup_timeout_ms`/`managed_max_restarts`/`managed_log_level` 托管）、`timeout_ms`、`auth_token_env`（承载 Bearer 令牌的环境变量名，声明不含密钥）；折叠 `fold_min_tokens`/`fold_max_tokens`/`fold_max_items`/`fold_max_retries`/`fold_total_budget_ms`；检索 `default_project_id`/`default_limit`/`max_results`；项目供给 `supply_mode`（`managed` 由运行时解析/创建 workspace 项目并触发首次索引，`external` 由部署方负责；未声明时按 `transport_mode` 派生）、`managed_watch`（托管供给下由服务侧 watch 维护增量索引）、`watch_timeout_ms`。
  - 托管模式的数据目录固定为 `{project_root}/.wf/code-context`，由监督任务持有的单实例锁 `runtime.lock` 保护：锁在监督任务启动时抢占、彻底停止时释放，重试期间保持独占。
  - 子进程 stdout/stderr 追加写入数据目录的 `cce.log`，失败告警会附带该文件的尾部若干行。日志随数据目录存续、不做自动轮转：需要时直接删除该文件（服务会重新创建）即可，删除数据目录等于重建索引缓存。
  - 项目与索引状态（未解析 / 解析中 / 索引中 / 已就绪 / 失败）、健康结论、断路器与子进程相位都可通过 `GET /api/v1/system/code-context` 只读查看；该端点不触发探测、不改变断路器状态。
  - 维护面（会真正访问服务、只由人触发）：`GET /api/v1/system/code-context/dead-letters` 列出被放弃的文件与模块，
    `POST .../dead-letters/retry`（可为空 `files` 表示全部）、`POST .../dead-letters/acknowledge`（可限定 `module`）
    执行重试与确认，`POST /api/v1/system/code-context/reindex` 重新跑当前 workspace 的全量索引——索引失败不会自动重试，
    这个动作是唯一的人工恢复入口（外部供给模式下被拒绝，重索引归属在部署方）。
- 其他表：作为 passthrough 原样保留，但必须是 table

## limits.toml

- `[agent]`：`max_iterations_cap`（默认 1000）、`default_max_iterations`（默认 10）、`max_concurrent`（0 = CPU 核心数）、`max_sub_agent_depth`（默认 8）、`max_pause_duration_ms`（0 = 无限制）
- `[workflow]`：`loop_max_iterations_cap`（默认 10000）、`loop_default_max_iterations`（默认 100）、`max_navigation_multiplier`（默认 5）、`max_concurrent`
- `[execution_defaults]`：`node_timeout_ms`（默认 30000）、`max_execution_time_ms`（0 = 无限制）
- `[compression]`：`max_retries`、`timeout_ms`、`settle_timeout_ms`、`tail_keep`
- `[compaction]`：`h_target_ratio`、`t_handoff_ratio`、`t_compact_ratio`、`t_hard_ratio`、`r_repress`、`r_min_range`、`k_repress`、`recent_raw_turns`、`tool_distill`、`file_dedup`、`plan_max_items`、`summary_call_budget_tokens`、`strategy`
- 环境覆盖：`WF_AGENT_*`、`WF_WORKFLOW_*`、`WF_EXEC_*`
