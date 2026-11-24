# 基础设施配置（infrastructure/）

基础设施配置位于 `{layer}/infrastructure/`，共九个固定文件名。由 `ConfigOrchestrator` 按优先级链加载并合并，随后应用 `WF_*` 环境覆盖与程序化覆盖。

| 文件 | 配置类型 | 作用 |
|---|---|---|
| `storage.toml` | `StorageConfig` | 工作流 / 检查点 / 任务持久化后端 |
| `metrics.toml` | `MetricsConfig` | 指标采集器、保留窗口、异常阈值、导出地址 |
| `sandbox.toml` | `SandboxGlobalConfig` | 脚本沙箱 profile 与路由规则 |
| `file-checkpoint.toml` | `FileCheckpointConfig` | 工作区文件快照与恢复 |
| `tool-approval.toml` | `ToolApprovalConfig` | 工具级审批策略 |
| `presets.toml` | `PresetsConfig` | 预定义工具 / 提示词启用 |
| `tools.toml` | `ToolConfigs` | 文件工具与 shell / code-context 覆盖 |
| `limits.toml` | `LimitsConfig` | agent / workflow / 执行 / 压缩限制 |
| `retention.toml` | `RetentionConfig` | 执行历史 / 任务 / 检查点的按时间保留策略 |

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
- `[code_context]`：代码上下文服务配置（文件折叠与检索工具）。主开关 `enabled`；传输 `transport_mode`（`external` 走 `base_url`，`managed` 拉起 `managed_binary` 并用 `managed_port`/`managed_startup_timeout_ms`/`managed_max_restarts`/`managed_log_level` 托管，最后一项只映射子进程 `RUST_LOG`）、`timeout_ms`、`auth_token_env`（承载 Bearer 令牌的环境变量名，声明不含密钥）；折叠 `fold_min_tokens`/`fold_max_tokens`/`fold_max_items`/`fold_max_retries`/`fold_total_budget_ms`；检索 `default_project_id`/`default_limit`/`max_results`；项目供给 `supply_mode`（`managed` 由运行时解析/创建 workspace 项目并触发首次索引，`external` 由部署方负责；未声明时按 `transport_mode` 派生）、`managed_watch`（托管供给下由服务侧 watch 维护增量索引）、`watch_timeout_ms`。
  - 托管模式的数据目录固定为 `{project_root}/.wf/code-context`，由监督任务持有的单实例锁 `runtime.lock` 保护：锁在监督任务启动时抢占、彻底停止时释放，重试期间保持独占。
  - 子进程 stdout/stderr 由监督器通过管道并发读取：每行原样转发到宿主 stderr，由服务管理器的 stderr 捕获承担长期留存与轮转；同时写入监督器持有的有界内存尾部，启动失败、就绪超时和意外退出的告警附带最近若干行。数据目录不再产生子进程日志文件，旧版本遗留的日志文件不再读取或迁移；删除数据目录等于重建索引缓存。
  - 项目与索引状态（未解析 / 解析中 / 索引中 / 已就绪 / 失败）、健康结论、断路器与子进程相位都可通过 `GET /api/v1/system/code-context` 只读查看；该端点不触发探测、不改变断路器状态。
  - 维护面（会真正访问服务、只由人触发）：`GET /api/v1/system/code-context/dead-letters` 列出被放弃的文件与模块，
    `POST .../dead-letters/retry`（可为空 `files` 表示全部）、`POST .../dead-letters/acknowledge`（可限定 `module`）
    执行重试与确认，`POST /api/v1/system/code-context/reindex` 重新跑当前 workspace 的全量索引——索引失败不会自动重试，
    这个动作是唯一的人工恢复入口（外部供给模式下被拒绝，重索引归属在部署方）。
- `[network]`：network 工具组的搜索后端与抓取传输配置，见下文；未声明时使用内置 DuckDuckGo 搜索后端和默认抓取参数
- 未知段：启动失败；`tools.toml` 只接受 `read_file`、`glob`、`list_files`、`shell`、`code_context`、`network` 六个段，段名拼写错误不会被静默忽略

### network 搜索后端

`tools.toml [network.web_search]` 包含 `mode`（`all` 并发调用配置的所有后端并汇总结果，或 `priority_first_success` 按优先级取第一个非空结果）、`timeout_ms`（整次聚合的硬上限）、`max_results`（缺省结果数，硬上限 50）、`max_retries` 与 `retry_delay_ms`（单后端失败重试）、`cache_ttl_ms`（结果缓存，0 关闭）、`min_interval_ms`（单后端调用间隔下限）、`default_backends`（允许使用的后端 id 列表，同时也限制显式 `backend` 参数，缺省为全部启用后端）以及 `backends` 数组。优先级默认来自后端注册顺序；声明 `default_backends` 时以该列表顺序为准。

后端通过 `kind` 区分：`duckduckgo_html` 是声明式 HTML 搜索的简写预设（接受 `endpoint`、`user_agent`、超时、代理与 `max_redirects`，内部翻译成一条 `http` 后端配置）；`http` 接受 `request`（`method`、`url`、`headers`、`query`、`json_body`）与 `response`（`format`、`items_path`、`map`、`url_prefix`）。所有后端都在构建时设置各自的逐跳重定向上限 `max_redirects`（默认 10，与抓取共用同一 client 构造与域名策略）。请求模板支持 `$query`、`$max_results`、`$language`、`$region`、`$freshness`、`$page` 与 `$offset`（等于 `(page-1) * max_results`，即 DuckDuckGo 的分页偏移）占位符；模型侧对应的可选参数会随请求传给后端，DuckDuckGo 预设把它们映射到 `kl`、`df` 与 `s`。请求头中的密钥用环境变量占位符注入，配置校验拒绝未知占位符。

每个后端可独立设置 `proxy.url` 与 `proxy.no_proxy`。代理 URL 只允许路由，不能内嵌 userinfo；凭据按路由从 credential store 读取并仅在构建该后端的 HTTP client 时临时组合，传输错误中的 userinfo 也会被脱敏。响应体在流式读取阶段就受统一上限约束，超限直接失败。插件贡献的搜索后端使用 `tool` + `tool-type` 声明，并为工具标记 `search-backend:web`；联网插件限定 native 后端且需要 `network` 权限，插件停用时会同步注销其后端。插件必须恰好贡献一个 tool-type executor，权限不满足、executor 数量不符或后端 id 冲突会让插件激活失败，而不是静默跳过。

### network 抓取传输

`tools.toml [network.web_fetch]` 配置 `timeout_ms`、`max_content_bytes`（流式下载硬上限）、`user_agent`、`max_redirects`、`backend`（内置 `http` 或 native 插件后端 id）和 `proxy`。另可配置 `respect_robots_txt`（默认关闭，开启后按 robots.txt 的 Disallow 规则拒绝路径）、`cache_ttl_ms`（默认 60 秒，0 关闭条件请求缓存）、`cache_capacity`（默认 256）与 `min_interval_ms`（同一主机的最小请求间隔，默认 0）。

`web_fetch` 的 `format` 为 `markdown`（默认）、`text` 与 `html`：前两者用 HTML5 DOM 解析页面并渲染结构化内容，其中 `markdown` 保留标题、列表、表格、代码块和绝对化链接，`text` 去掉标记但保留行结构；`html` 返回原始响应体。工具还支持 `selector`（仅支持标签、`#id`、`.class` 的简单组合）、`max_chars`、`render` 与 `wait_for_ms`；内置 HTTP 后端不支持 `render` 与 `wait_for_ms`，会显式报错，浏览器渲染可由插件后端提供。`selector` 只作用于 HTML 正文，与 `format = "html"` 组合会报错。响应按 HTTP charset、HTML meta charset、BOM、UTF-8 的顺序解码。响应体超限在接收过程中立即失败，超长文本再走统一落盘。

抓取后端注册表与搜索后端一样按插件 owner 管理生命周期；插件用 `fetch-backend:web` 标记贡献抓取能力，搜索后端仍使用 `search-backend:web`，两者都要求 native 实现、`network` 权限和唯一 executor。当前选中的抓取后端 id 由注册表持有，路由器在每次调用时读取，因此运行时可切换；当被移除的插件后端正是当前选中项时，自动回退到内置 `http` 后端。调用选项会透传给插件工具，由插件自行消费；此时 handler 不再重复应用选择器。页面渲染与输出后处理是宿主可注入的扩展点，未注入时使用默认 HTML5 DOM 渲染规则。

审批配置中的网络域名规则会下推到内置传输层：拒绝列表始终生效；允许列表仅在 `always_allow_network = true` 时约束抓取目标，否则由人工审批决定。搜索后端只接收拒绝规则，因为搜索引擎端点并不是被搜索的目标站点；对每次重定向目标都会重新校验域名。`web_search_backends` 工具列出当前可用后端及能力；`web_search` 工具定义携带已解析的后端端点域名，审批阶段可据此执行域名策略，不再因为缺少 URL 参数而直接转人工。

后端调用按 `backend` 标签记录次数、耗时与错误数，使用共享工具指标采集器，便于观察单个搜索后端的成功率与延迟。

## limits.toml

- `[agent]`：`max_iterations_cap`（默认 1000）、`default_max_iterations`（默认 10）、`max_concurrent`（0 = CPU 核心数）、`max_sub_agent_depth`（默认 8）、`max_pause_duration_ms`（0 = 无限制）
- `[workflow]`：`loop_max_iterations_cap`（默认 10000）、`loop_default_max_iterations`（默认 100）、`max_navigation_multiplier`（默认 5）、`max_concurrent`
- `[execution_defaults]`：`node_timeout_ms`（默认 30000）、`max_execution_time_ms`（0 = 无限制）
- `[compression]`：`max_retries`、`timeout_ms`、`settle_timeout_ms`、`tail_keep`
- `[compaction]`：`h_target_ratio`、`t_handoff_ratio`、`t_compact_ratio`、`t_hard_ratio`、`r_repress`、`r_min_range`、`k_repress`、`recent_raw_turns`、`tool_distill`、`file_dedup`、`plan_max_items`、`summary_call_budget_tokens`、`strategy`
- 环境覆盖：`WF_AGENT_*`、`WF_WORKFLOW_*`、`WF_EXEC_*`

## retention.toml

- 全局：`enabled`（默认 false）、`scan_interval_secs`（默认 3600）、`batch_size`（默认 200）、`dry_run`（默认 true）
- `[checkpoints]`：`max_age_days`（默认 30）、`max_checkpoints`（默认 50）、`protect_favorites`（默认 true）
- `[file_checkpoints]`：`history_max_age_days`（默认 30）、`keep_recent_heads`（默认 0）
- `[executions]`：`history_retention_days`（默认 180）、`keep_recent`（默认 20）、`protect_running`（默认 true）
- `[tasks]`：`retention_days`、`trigger_retention_days`（均默认 30）
- 初始值来自文件；设置页写入的类型化策略保存在服务端快照 `system:retention`，覆盖文件默认值。删除默认只在 `enabled = true && dry_run = false` 时执行。
