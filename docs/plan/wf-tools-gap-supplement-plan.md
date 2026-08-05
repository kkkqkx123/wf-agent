# wf-tools 功能差异分析与分阶段补充方案

> 状态：分析完成，Stage 1-6 已实施（Stage 6 的 PTY 部分按风险表延后），Stage 7+ 待实施
> 范围：`crates/wf-tools/`（含 `crates/wf-agent/src/coordinator/` 工具执行编排）
> 对照：`packages/sdk/services/tools/`、`packages/sdk/services/executors/`、`packages/sdk/resources/predefined/tools/`、`packages/sdk/shared/tools/`
> 关联文档：`docs/plan/mcp-skill-rs-ts-diff.md`（MCP 基础接线、Skill 对齐已在其中规划，本文不重复）

## 一、差异摘要

### 1.1 已对齐 / 超出的部分

| 能力 | 状态 |
|------|------|
| Executor 体系（Stateless / Stateful / Rest / Mcp / BuiltIn / Cli） | ✅ `executor/` 全实现，`trait_def.rs` 含重试+超时扩展 |
| MCP 客户端（stdio / sse / streamable-http 三传输，自研 JSON-RPC） | ✅ 已实现，含 idle 维护 / 健康检查循环（基础接线见关联文档） |
| 预定义工具 | ✅ 29 个，多于 TS 的 18 个（多出 web、memory 扩展、shell_output/shell_kill） |
| approval / command_safety / protect / ignore / failure_protection / skill / patch | ✅ 全实现 |
| 工具描述生成 + schema 格式化（`tool_description_generator` / `tool_schema_formatter`） | ✅ 对应 `tool-description-generator` / `tool-schema-formatter` / `tool-schema-cleaner` |
| LLM 工具声明渲染 | ✅ `wf-llm/src/tool_format.rs` 对应 `tool-declaration-formatter` |

### 1.2 主要差异点（需补充）

| # | 差异 | TS 位置 | Rust 现状 |
|---|------|---------|-----------|
| D1 | 预定义工具风险分类缺失 | `resources/predefined/tools/risk-classification.ts` | `predefined/schema.rs:94` 全部 `risk_level: None`，approval 默认按 `write` |
| D2 | Agent 工具执行编排能力不足 | `services/executors/tool-call-executor.ts`（1026 行） | `wf-agent/src/coordinator/tool.rs` 缺进度回调 / 批量取消 / 可见性 / checkpoint / 失败保护 / 中断传播 |
| D3 | RestExecutor 能力弱化 | `services/tools/executors/rest.ts` | 固定 `POST {base}/{tool.name}`，无 method/query/headers/body 参数化、无 error interceptor |
| D4 | MCP 高级特性缺失 | `services/executors/mcp/features/{analytics,approval,metadata,registration}` | 仅基础 registration |
| D5 | Remote / gRPC executor 缺失 | `services/executors/remote/`（LayertwineExecutor） | `crates/layertwine` 只有服务端，无 SDK 侧连接型 executor |
| D6 | 终端能力差异 | `services/terminal/` | `predefined/shell/engine.rs` 无 shell 探测 / PTY / 交互输入 / 增量输出游标 |
| D7 | 通用 HTTP 传输层缺失 | `services/transport/http/` | RestExecutor 直接用 reqwest，无 retry 黑名单 / 限流 / 拦截器管线 |
| D8 | CLI executor 行为差异 | `executors/cli/` | 超 max_lines 不 kill 进程；ripgrep 非 NDJSON 解析 |
| D9 | shared/tools 工具集缺口 | `shared/tools/` 9 个文件 | 仅覆盖 2 个（description-generator / schema-formatter） |
| D10 | 注册入口与命名差异 | `resources/predefined/tools/registration.ts` | 无 allowList/blockList；`run_shell`↔`execute_command` 等命名不一致 |

## 二、分阶段补充方案

实施顺序按依赖与价值排列：先补数据与编排（P0），再做传输与特性增强（P1），最后做兼容与完善（P2）。

---

### Stage 1（P0）：预定义工具风险分类 + 审批数据对齐

**目标**：解决「所有预定义工具默认 `write` 风险」导致只读工具也需审批的问题，对齐 TS `TOOL_RISK_CLASSIFICATION`。

**做法**：

1. `crates/wf-tools/src/predefined/schema.rs`
   - `ToolDefinition` 增加 `risk_level: ToolRiskLevel` 字段；`tool_def()` 填充 `metadata.risk_level`。
2. 逐一为 29 个预定义工具补齐风险等级（对齐 TS 分类）：
   - 只读：`read_file`、`list_files`、`grep_search`、`glob_search`、`recall_notes`、`list_categories`、`shell_output`、`query_workflow_status`、`skill`、`update_todo_list` → `ReadOnly`
   - 写入：`write_file`、`edit_file`、`apply_diff`、`apply_patch`、`record_note`、`memory_remember`、`memory_forget` → `Write`
   - 执行：`execute_command`、`backend_shell`、`shell_kill` → `Execute`
   - MCP：`use_mcp` → `Mcp`
   - 网络：`web_fetch`、`web_search` → `Network`
   - 系统：`execute_workflow`、`cancel_workflow`、`call_agent` → `System`
   - 交互：`ask_followup_question`、`attempt_completion` → `Interaction`
   - `memory_list` 与 `record_note` 类同 → `Write`
3. 新增 `predefined/risk.rs`（可选）：提供 `get_tool_risk_level(id)` 便捷查询，供审批路径与 LLM 描述使用。
4. 审批预设（`SecurityPreset`）逻辑已在 `approval.rs` 实现，无需改动，仅依赖风险数据正确填充。

**验收**：`cargo test -p wf-tools`；新增单测断言各预定义工具 `metadata.risk_level` 与预期一致；`ToolApprovalCoordinator` 对 `read_file` 在 Safe 预设下自动通过。

---

### Stage 2（P0）：Agent 层工具执行编排增强

**目标**：对齐 TS `ToolCallExecutor` 的编排能力。注：Rust 实际编排在 `wf-agent/src/coordinator/tool.rs`（`ToolExecutionCoordinator`），`wf-tools/src/tool_call.rs` 为轻量通用执行器，增强应落在 coordinator。

**做法**（按项，均加可选注入、默认关闭）：

1. **流式进度回调 `onProgress`**
   - `ToolExecutionCoordinator` 增加 `progress_tx: Option<mpsc::Sender<ToolProgressEvent>>`；单工具执行开始/完成时发送 `{tool_call_id, status, partial}`。
   - 供 streaming 驱动（`execute_single_tool_for_stream`）与 workflow 节点透传。
2. **批量 abort 协调**
   - parallel 模式改为共享一个 `CancellationToken`（或 `tokio::task::JoinSet` + `abort_all`）：任一工具失败/中断时取消整批，其余任务返回取消结果而非继续执行。
   - 外部 `CancellationToken` 传入 `execute_tool_calls`，合并进每个子任务。
3. **工具可见性 `ToolVisibilityStore`**
   - 定义 trait：`is_tool_visible(execution_id, tool_name)`；coordinator 在执行前过滤，不可见则生成错误消息（对齐 TS `ToolVisibilityStore`）。
4. **工具调用前后 checkpoint 集成**
   - 可选注入 `create_checkpoint_fn`；当工具 `metadata.create_checkpoint`（需在 `wf-types` 补充该字段）为 `before/after/both` 时，调用 `wf-checkpoint` 创建快照，失败抛 `WorkflowCheckpointError`。
5. **失败保护集成**
   - 注入 `ToolFailureProtectionState`：执行前 `can_execute` 拦截、失败 `record_failure`、成功 `record_success`，与 `failure_protection.rs` 现有能力对接。
6. **中断传播**
   - `ToolExecutionOptions` 或 context 增加 `CancellationToken`，经 `ToolExecutorExt::execute_with_timeout` 传到底层 executor（CLI/MCP/HTTP 均可感知）。

**验收**：新增 coordinator 单测覆盖：进度事件顺序、parallel 取消整批、可见性拦截、失败保护冷却、checkpoint 前后调用计数；`cargo test -p wf-agent`。

---

### Stage 3（P1）：RestExecutor 增强

**目标**：对齐 TS `rest.ts` 的完整 REST 能力。

**做法**：

1. `executor/rest.rs`（✅）
   - URL 由参数 `url`/`endpoint` 决定（缺省回退 `{base}/{tool.name}`），`base_url` 拼接逻辑对齐 TS `buildFullUrl`（相对路径合并、query string 追加）。
   - 支持参数化 `method`（GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS）、`headers`、`body`、`query`。
   - 增加 `error_interceptors`（`Arc<dyn Fn(ToolError) -> ToolError>` 链）。
   - 重试：`RestToolConfig.max_retries/retry_delay` 目前仅解析未使用，接入指数退避重试（可复用 `trait_def.rs` 的 retry 或 executor 内实现）。
   - 类型化错误映射：非 2xx 按 status 归类（429 限流、5xx 服务端等），供熔断与审批参考。
2. `wf-types::tool::runtime_config::RestToolConfig` 如需补充 `method` 默认值等字段，一并调整。（✅ 已补 `method` 可空字段）

**验收**：新增单测覆盖各 method 分派、query 拼接、error interceptor 链、重试后成功/失败。

---

### Stage 4（P1）：MCP 高级特性

**目标**：补齐 `features/{analytics,approval,metadata,registration}` 四个特性。基础接线（连接管理器注入、配置加载、工具注册）见关联文档 Stage 1-4，本文只列增强项。

**做法**：

1. **analytics**：新增 `mcp/analytics.rs`（`McpUsageAnalytics`）（✅）
   - 按 `server/tool` 记录调用数、成功/失败、耗时 min/avg/max、成功率、首末时间；有界历史（默认 10k）。
   - 查询：`get_tool_stats` / `get_hot_tools` / `get_problematic_tools` / `export_json` / `generate_report`。
   - 挂载点：`McpExecutor` 执行前后打点（经 `ToolExecutionContext` 或共享 `Arc`）。
2. **approval 增强**：扩展 `mcp/registration.rs` 与 `approval.rs` 的 `check_mcp_approval`（✅ 新增 `approval_enhanced.rs`）
   - 参数级规则：per-tool 允许/拒绝参数值（字面或正则）。
   - 限流规则：每窗口最大调用数（全局或按用户）。
   - 访问控制：server/tool 与操作类型（tool_call / resource_read）的 user/role 允许/拒绝。
3. **metadata**：新增 `mcp/metadata.rs`（✅）
   - `McpToolMetadataCache`：TTL 缓存各 server 的 tools/resources，避免重复 `list_tools`；支持 per-server 失效与清理定时器。
   - `McpToolsDynamicContextProvider`：生成 LLM 可见上下文（server 状态、工具摘要、hot-tools 排序、注入位置/紧凑模式），供 agent loop 注入 system prompt。
4. **registration 增强**（✅）
   - `register_mcp_tools` 增加选项：`only_hot_tools`、`max_tools`、`name_prefix`、`auto_unregister`；工具 id 消毒规则对齐 `mcp_{server}__{tool}`。

**验收**：新增单测：analytics 聚合正确性、approval 三层规则命中、metadata 缓存 TTL 失效、registration 前缀/上限过滤。

---

### Stage 5（P1）：Remote / gRPC executor（Layertwine）

**目标**：提供 SDK 侧连接型远程 executor，对齐 TS `BaseRemoteExecutor` + `LayertwineExecutor`。

**做法**：

1. 新增 `executor/remote.rs`（✅）
   - `RemoteExecutor` trait：`connect(config)` / `disconnect()` / `call(method, request)` / `is_connected()` / `get_status()`。
   - `RemoteConnectionConfig`：地址、TLS、超时、`reconnect_policy`（max_retries/base_delay/max_delay）。
   - `RemoteExecutorStatus`：disconnected / connecting / connected / unhealthy / error。
2. `layertwine` crate 侧（✅ 新增 `api/rpc/client.rs`）
   - 在其 gRPC（tonic/prost）基础上暴露一个客户端库（`client` 模块），供 wf-tools 依赖；保持 server 逻辑不变。
3. `LayertwineExecutor`（✅）
   - 实现 `RemoteExecutor`，方法：init / edit / status / commit / log / branch 操作 / approve / backup / checkpointRestore / checkpointDiff。
   - 两种部署模式：**embedded**（自动拉起 `layertwine` 二进制，端口轮询就绪、自动重启上限、SIGTERM→SIGKILL 优雅退出）与 **remote**（预部署直连）。
   - 健康检查 + 指数退避自动重连（对齐 TS `GrpcClient` 行为）。
4. 注册方式：以状态型或内置工具形式注册为 `Tool`（文件历史相关），按需暴露给 agent。（✅ `register_layertwine_tools`）

**验收**：`cargo build -p layertwine --features grpc` 通过；集成测试覆盖远程模式 init/edit/commit 往返与断线重连。

---

### Stage 6（P1）：终端能力

**目标**：提升 `backend_shell` 等 shell 工具的可用性，对齐 TS `terminal-service`。

**做法**：

1. 新增 `shell/shell_detector.rs`（✅ 已完成）
   - 识别 bash / zsh / fish / sh / cmd / powershell / pwsh / git-bash / wsl。
   - 平台默认路径 + `which`/`where` 回退 + 缓存探测结果；`get_default_shell()` 读 `$SHELL`。
   - `execute_command` 的默认 shell 从固定 `/bin/sh -c` 改为探测结果。
2. PTY 与交互输入（可选项，引入 `portable-pty`）（⏸ 延后，设计稿见 `docs/plan/wf-tools-terminal-interactive-plan.md`）
   - `backend_shell` 支持 `send_input`（交互式一次性/持续会话），对齐 TS `executeWithInput` / `sendInput`。
   - 无 PTY 环境回退到现有管道实现。
3. 增量输出（✅ 已完成）
   - `BackgroundShellStore` 的环形缓冲增加 `last_read_index` 游标，支持增量读取（对齐 TS `getOutput`）。

**验收**：shell detector 各平台单测；backend_shell 交互输入集成测试（PTY 延后，待补）；`cargo test -p wf-tools`。

---

### Stage 7（P2）：通用 HTTP 传输层

**目标**：为 RestExecutor、wf-llm 及未来 server 层提供统一 HTTP 客户端。

**做法**：

1. 评估是否抽出独立 `wf-http` 或并入 `wf-common`：
   - `HttpClient`：重试黑名单（4xx 不重试）、熔断、令牌桶限流、请求/响应/错误拦截器管线、SSE 流式响应（可复用 `eventsource-stream`）。
2. 若抽出，`RestExecutor` 改为基于 `HttpClient` 实现（能力与 Stage 3 重叠，二选一或合并推进）；`wf-llm` 保持现状避免回归。
3. 若暂不抽出，仅在 `RestExecutor` 内部补齐对应能力（见 Stage 3），本 Stage 延后到 wf-server 需要时再启动。

**验收**：按决策执行后跑 `cargo clippy --all-targets` 全量通过。

---

### Stage 8（P2）：CLI executor 增强

**目标**：对齐 TS `BaseCliExecutor` / `RipgrepExecutor` 行为。

**做法**：

1. `executor/cli.rs`
   - `read_pipe` 超 max_lines 时**主动 kill 子进程**（当前只停止读取），并返回 `truncated_reason`。
   - `ExecutorInfo` 增加 `version`（`--version` 探测，可选）。
2. `RipgrepExecutor`
   - `search` 改走 `rg --json`（NDJSON 解析 begin/match/context 事件），合并相邻行为匹配组、长行截断（500 字符）、300 结果上限，输出带相对路径的分组结果；`list_files` 合成父目录条目。

**验收**：`read_pipe` 行数 kill 行为单测；ripgrep JSON 解析单测（用 mock 输出或跳过无 rg 环境）。

---

### Stage 9（P2）：shared/tools 工具集补充 + 注册入口

**目标**：补齐描述/参数渲染工具，对齐注册入口。

**做法**：

1. `wf-tools/src/` 新增：
   - `tool_description_registry.rs`：运行时全局描述注册表（query/get/register），配合 `builtin_tool_descriptions()`。
   - `tool_parameters_describer.rs`：参数 → LLM 可读描述（类型/必填/默认/枚举），供 `tool_description_generator` 复用。
2. `handlers.rs` 注册入口增加选项：
   - `allow_list` / `block_list` 过滤预定义工具，对齐 TS `registerPredefinedTools` 的 `allowList/blockList`。
3. 命名对齐（可选，无兼容压力下按需）：为 `execute_command`、`edit_file`、`grep_search`、`glob_search` 增加 TS 同名别名（`run_shell`/`edit`/`grep`/`glob`），避免重复注册冲突，仅当有外部配置依赖 TS 工具名时启用。

**验收**：描述注册表 CRUD 单测；allow/block 过滤单测。

---

### Stage 10：验证

**做法**：

1. `cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test --workspace` 全量通过。
2. 按 Stage 更新 `docs/plan/wf-tools-gap-supplement-plan.md` 实施记录表。
3. 补充与 `wf-agent` / `wf-workflow` / `wf-server` 的集成验证（工具执行 → 审批 → checkpoint → 事件闭环）。

---

## 三、实施记录

### 已完成实现摘要

**Stage 1（预定义工具风险分类）**
- `crates/wf-types/src/tool/risk_level.rs`：`ToolRiskLevel` 增加 `Copy`。
- `crates/wf-tools/src/predefined/schema.rs`：`ToolDefinition` 增加 `risk_level` 与 `create_checkpoint` 字段，`tool_def()` 填充 `metadata.risk_level` / `metadata.create_checkpoint`。
- 29 个预定义工具逐一补齐风险等级（只读 10 / 写入 8 / 执行 3 / MCP 1 / 网络 2 / 系统 3 / 交互 2）。
- 新增 `crates/wf-tools/src/predefined/risk.rs`：`get_tool_risk_level` / `tools_with_risk` + 分类单测。

**Stage 2（Agent 工具执行编排增强）**
- `crates/wf-types/src/tool/checkpoint.rs`：新增 `CheckpointTiming`（Before / After / Both），`ToolMetadata` 增加 `create_checkpoint`。
- `crates/wf-agent/src/coordinator/tool.rs`：
  - 新增 `ToolProgressEvent` / `ToolProgressStatus`、`ToolVisibilityStore` / `ToolCheckpointHandler` trait，全部可选注入、默认关闭。
  - 并行模式改为 `JoinSet`：任一工具失败可 `abort_all` 整批（`with_cancel_on_failure`），外部取消经 `with_cancellation` 合并实体 abort 信号后逐任务生效。
  - 失败保护：执行前 `can_execute` 拦截、成功 `record_success`、失败 `record_failure`；同时修正「工具返回 `success=false` 被视为成功」的旧行为。
  - 执行前后按 `create_checkpoint` 时机调用 checkpoint handler。
  - 共享执行核心 `run_tool` 收敛顺序/并行/流式三条路径，消除 `execute_single_tool` 与 `build_result_msg` 的重复逻辑。

**Stage 3（RestExecutor 增强）**
- `crates/wf-types/src/tool/runtime_config.rs`：`RestToolConfig` 增加 `method`（可空），补齐方法覆盖能力。
- `crates/wf-tools/src/executor/rest.rs` 重写：
  - 请求拦截器（可改写 url/headers/body）、响应拦截器（可改写 status/body）、错误拦截器（将响应转错误并分类）。
  - `classify_status` 将 HTTP 状态码映射为 `RestErrorKind`（鉴权/限流/服务端/客户端/未知），错误拦截器据此提取 `RestRequestSpec` 供重试判定。
  - `build_full_url` 支持 base+relative 拼接、query 参数 urlencode 追加、已有 `?` 时用 `&` 追加。
  - 指数退避重试（可配次数/初退避/倍率/抖动）+ 熔断器（可配失败阈值/复位周期），自动从 `RetryableError` 区分可重试错误。
- 单测：`executor::rest` 8 个用例覆盖拦截器、URL 拼接、重试与熔断逻辑。

**Stage 4（MCP 高级特性）**
- `crates/wf-tools/src/mcp/analytics.rs`：`McpUsageAnalytics` 记录调用时长/成功失败/错误类型，提供 hot/cold/problematic 工具统计、按 server 聚合、JSON 导出与摘要报告。
- `crates/wf-tools/src/mcp/approval_enhanced.rs`：`EnhancedMcpApprovalSystem` 三层规则 —— 参数规则（入参校验/净化）、限流（次数/窗口）、访问控制（server/tool/资源级）；`ApprovalResult` 携带风险等级与原因。
- `crates/wf-tools/src/mcp/metadata.rs`：`McpToolMetadataCache`（TTL 缓存 + 统计）与 `McpToolsDynamicContextProvider`（生成注入 system prompt 的 LLM 可见上下文，含 server 状态/hot-tools 摘要/紧凑模式）。
- `crates/wf-tools/src/mcp/registration.rs`：`McpToolRegistrationOptions`（前缀/最大数/启用回调），`sanitized_mcp_tool_id` 清洗非法字符，`McpToolsRegistrar`（Mutex 内部可变性，支持重复注册覆盖）。
- `crates/wf-tools/src/executor/mcp.rs`：`McpExecutor::with_analytics` 注入，执行前后自动打点。
- 全部可选注入、默认关闭，向后兼容。单测：`mcp` 模块 36 个用例，`wf-tools` 全量 178 个通过。

**Stage 5（Remote / gRPC executor）**
- `crates/layertwine/src/api/rpc/client.rs`：`LayertwineGrpcClient` 类型化客户端，封装 init/status/edit/agent_edit/agent_submit/approve/commit/log/branch/backup/restore/checkpoint 系列操作，`ClientError`/`ClientResult`。
- `crates/wf-tools/src/executor/remote.rs`：
  - `RemoteExecutor` trait（`&self` + 内部可变性，支持 `Arc` 共享并发）。
  - `layertwine_impl`（feature `remote-layertwine`）：`LayertwineExecutor`（`tokio::sync::Mutex` 内态 + `AtomicU8` 状态机：disconnected/connecting/ready/error）+ `dispatch` 按方法分发到 gRPC 调用。
  - `LayertwineProcessManager`：embedded 模式拉起/健康检查/自动重启/停止；`register_layertwine_tools` 注册为状态型工具。
- `crates/wf-tools/Cargo.toml`：新增可选 `layertwine`/`tonic` 依赖与 `remote-layertwine` feature。
- 验证：`cargo build -p layertwine --features grpc` 通过；`remote` 集成测试 2 个用例（init/edit/commit 往返、branch + checkpoint diff）通过。

**Stage 6（终端能力）**
- `crates/wf-tools/src/shell/shell_detector.rs`：`ShellType`（bash/zsh/fish/sh/cmd/powershell/pwsh/git-bash/wsl）+ `ShellDetector`（覆盖配置/缓存/`resolve_shell_path`），`default_shell_detector()` / `resolve_shell_command()`，跨平台回退（Windows 用 cmd，其他用 `sh -c`）。
- `crates/wf-tools/src/shell.rs`：`ShellToolConfig` 增加 `shell_type` 可选覆盖，`run_command` 默认走探测出的 shell。
- `crates/wf-tools/src/predefined/shell/engine.rs`：新增 `OutputBuffer`（环形缓冲 + 读游标），`ShellSession` 提供 `read_new_output`/`peek_new_output` 增量读取，`BackgroundShellStore::with_shell` 按探测 shell spawn。
- `crates/wf-tools/src/predefined/shell/shell_output.rs`：`SHELL_OUTPUT` 增加 `all` 参数，支持仅返回增量 `new_output`。
- 说明：PTY / 交互式输入按计划中的风险表延后（`portable-pty` 平台差异大，作 feature 开关后续补齐），本阶段先交付 shell 探测与增量输出。
- 后续变更：终端引擎已拆分为独立 `crates/wf-shell/`（`docs/plan/wf-shell-crate-拆分方案.md`），本段所列文件除工具定义外均已随迁；`ShellToolConfig`/`run_command`/`engine.rs` 现位于 wf-shell。
- 单测：`shell` 模块 18 个用例 + `test_shell_output_incremental_read` 增量读取用例通过。

| 阶段 | 内容 | 优先级 | 状态 |
|------|------|--------|------|
| Stage 1 | 预定义工具风险分类 | P0 | ✅ 已完成 |
| Stage 2 | Agent 工具执行编排增强 | P0 | ✅ 已完成 |
| Stage 3 | RestExecutor 增强 | P1 | ✅ 已完成 |
| Stage 4 | MCP 高级特性（analytics/approval/metadata/registration） | P1 | ✅ 已完成 |
| Stage 5 | Remote / gRPC executor（Layertwine） | P1 | ✅ 已完成 |
| Stage 6 | 终端能力（shell 探测 / 增量输出；PTY 延后） | P1 | ✅ 已完成（PTY 待后续） |
| Stage 7 | 通用 HTTP 传输层 | P2 | 待实施 |
| Stage 8 | CLI executor 增强 | P2 | 待实施 |
| Stage 9 | shared/tools 补充 + 注册入口 | P2 | 待实施 |
| Stage 10 | 验证 | — | 待实施 |

## 四、依赖与风险

| 项 | 影响 | 缓解 |
|----|------|------|
| Stage 5 依赖 `layertwine` 暴露 gRPC 客户端 | 阻断 Remote executor | ✅ 已交付 gRPC 客户端 + Remote executor；embedded 进程管理一并实现，远程模式可直接使用 |
| Stage 6 引入 `portable-pty` 新增依赖 | 编译体积/平台差异 | ⏸ PTY 按此风险延后；已交付 shell 探测 + 增量输出，无 PTY 平台回退管道模式 |
| Stage 4 与关联文档 MCP 基础接线顺序 | 特性依赖接线完成 | 先完成关联文档 Stage 1-4，再实施本文 Stage 4 |
| Stage 2 改动集中在 `wf-agent` | 回归风险 | 全部增强为可选注入、默认关闭；补单测覆盖 |
| Stage 7 是否抽 crate 决策 | 范围蔓延 | 默认内部实现，延迟到 wf-server 需要时再抽出 |
