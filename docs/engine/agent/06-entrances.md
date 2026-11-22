# 对外入口、触发式执行与支撑设施

> 范围：`crates/engine/wf-agent/src/executor.rs` 及 `executor/` 子树、`entity.rs`、`agent_request.rs`、`validation.rs`、`hook.rs`、`callback.rs`、`trigger.rs`、`subagent.rs`、`error.rs`、`error_analysis.rs`、`constants.rs`。

## 1. 执行门面：`AgentLoopExecutor`

统一入口：同步执行、后台派发、状态查询与取消都经它进入（`src/executor.rs:23-40`）。内部持有 LLM 网关、工具注册表、执行注册表 `AgentLoopRegistry`、迭代数上下限、子代理深度上限等（`executor.rs:26-40`）。

- **构造与注入**：builder 方法依次注入迭代数、迭代硬上限、子代理深度（仅可收紧不可超过结构上限，`executor.rs:78-82`）、并发上限（`executor.rs:85-88`）、共享注册表（供 wf-api/wf-runtime 观察同一批执行，`executor.rs:94-101`）、事件总线、信号总线（stop/pause/resume 控制信号，`executor.rs:117-123`）、hook 处理器注册表（`executor.rs:127-130`）、压缩参数与 settle 超时（`executor.rs:134-146`）。
- **coordinator 组装** `coordinator()`（`executor.rs:148-177`）：把父链接、事件/信号总线、hook 注册表、压缩限制等全部装配进 `AgentLoopCoordinator`。

### 各子模块职责

| 模块 | 职责 |
|---|---|
| `sync_run.rs` | 同步执行入口 `execute` / `execute_with_parent`：先过配置门与输入门、深度门、文件作用域继承，再预分配 child id、向父流发 started 事件，运行结束后 `ChildRunReclaim::finish` 解除父子链接并向父流发 ended 事件（`sync_run.rs:10-61`） |
| `spawn.rs` | 后台派发 `spawn_agent_loop` / `spawn_with_parent`：预生成 execution id 并注册占位实体，使句柄在任务启动前即可查询/取消（`spawn.rs:25-31, 51-86`）；父取消令牌触发时子执行被停止（`spawn.rs:184-193`） |
| `control.rs` | 状态查询与取消：校验调用者与目标在同一执行子树（`require_same_subtree`）后返回状态字符串；`control_cancel` 同样先校验子树，再 `entity.stop()` 并 abort 后台任务（`control.rs:25-88`） |
| `gates.rs` | 执行前门禁：`gate_config` 拒绝 error 级配置问题（`gates.rs:40-62`）；输入/上下文 schema 校验（`gates.rs:9-36`）；`check_sub_agent_depth` 统一深度门（`gates.rs:69-79`）；`inherit_file_scope` 继承文件作用域（`gates.rs:81-105`） |
| `reclaim.rs` | `ChildRunReclaim` 回收守卫：派发前 arm、run 结算时 `finish`；若驱动 future 在结算前被 drop（工具超时/任务 abort），Drop 路径回收注册表槽位与容量许可，避免幽灵执行占用资源（`reclaim.rs:8-84`） |
| `callback_adapter.rs` | 为执行器实现 `ExecutionCallback` trait，使 agent-as-tool 路径可调用同步/spawn/查询/取消；错误映射保留 Cancelled/Timeout/Validation 语义（`callback_adapter.rs:17-103`） |

## 2. 实体与工厂

- `AgentLoopEntity`（`src/entity.rs:46-109`）是单次 agent loop 的运行时实体：实现 `ExecutionEntity` trait 的状态查询/pause/resume/stop/abort（`entity.rs:510-598`）；携带层级管理器（父子注册、深度、根 id，`entity.rs:154-164`）；管理工具快照与可见性池（`entity.rs:216-266`）；持有容量许可与子代理事件通道（`entity.rs:602-639`）；暂停超时管理（`entity.rs:643-698`）；运行时输入追加（steering/followup 双模式 `AgentAppendMode`，`entity.rs:19-35, 705-758`）。
- `AgentLoopFactory::create(id)` 是 `AgentLoopEntity::new` 的薄封装（`src/factory.rs:6-8`）。

## 3. 请求构造：`agent_request.rs`

`build_agent_request` 从实体构建 `LlmRequest`：读取对话的**投影视图**（压缩后 summary+尾部）而非全量历史（`agent_request.rs:42-49`）；显式"纯聊天"（可用工具池为空且无初始/可发现工具）时直接不带 tools 渲染，而不是把空池解析成全部工具（`agent_request.rs:51-74`）。随后按工具池解析工具 schema 并注入可发现工具元数据块。

## 4. 校验：`AgentLoopValidator`

产出结构化 `ValidationIssue`（field + message + severity，`src/validation.rs:16-38`）：

- **配置校验**：agent_id/model 非空（`validation.rs:71-83`）；`max_iterations >= 1` 且不超过硬上限（`validation.rs:85-101`）；`max_execution_time=0`、`token_limit=0` 属警告（`validation.rs:103-119`）；profile 感知版本校验 model 是否在已知 profile 集合（`validation.rs:150-153`）；tool-call 协议兼容性（`validation.rs:171-174`）。
- **输入校验**：`validate_input` 要求消息文本非空或带图片（`validation.rs:272-275`）；`validate_context_with_schema[_with_policy]` 检查上下文键类型/必填/默认，strict 模式拒绝未知键（`validation.rs:217-233`）。
- **fail-fast 变体**：`validate_or_fail[_with_cap]` / `validate_or_fail_input[_with_schema]` 在 error 级问题上直接返回 Err（`validation.rs:330-355, 298-325`）；`validate_tool_list_syntax` 在无注册表时拦截空白/重复/矛盾工具桶（`validation.rs:488-551`）。

## 5. Hook 体系：`hook.rs`

`AgentHookEmitter` 提供三个发射入口（`src/hook.rs`）：

- `fire_agent_point`（`hook.rs:21-45`）：为实体配置的某 hook 类型构建 `HookContext`（含状态、迭代数、abort 信号）并 fire，返回 `FireSummary` 供 gate 点响应否决。
- `fire_point`（`hook.rs:50-65`）：对调用方自建 context 发射（如并行工具调用路径）。
- `fire_agent_point_with_checkpoint`（`hook.rs:73-92`）：fire 后若存在 opt-in 定义命中，则创建 hook 请求的 checkpoint，绕过 per-trigger 节奏、受总开关约束，失败仅告警。

## 6. 内建回调：`callback.rs`

`register_builtin_tools(gateway, registry)` 创建共享 `AgentLoopExecutor` 并调用 `register_execution_callback` 把它注册为全局执行回调，使工具侧（agent-as-tool）能驱动 agent loop（`callback.rs:7-18`）。

## 7. 触发式执行：`trigger.rs`

- `TriggeredAgentExecutionConfig`（`trigger.rs:41-63`）：父实体、结果写入变量名、是否等待完成（仅决定 runner 是否阻塞，父子从不同步——写回落入父对话，父下一轮 LLM 请求读到）、超时、触发点会话锚点、输入模式与写回方式。
- `snapshot_conversation_for_child`（`trigger.rs:81-96`）：位置锚点做 `PrefixToAnchor` 切前缀，其余组合回退全量快照。
- `TriggeredAgentExecutionManager`（`trigger.rs:112-123`）：`submit_triggered_execution` 同步返回子输出或后台提交描述符；子执行在父实体上注册、结果写回父变量快照，子失败不影响父（`trigger.rs:109-170, 446-449`）。

## 8. 子代理报告：`subagent.rs`

向父流报告子代理生命周期：`display_name`（agent id 或任务提示词首行，60 字符，`subagent.rs:21-32`）、`preview`（160 字符，`subagent.rs:35-42`）、`child_depth`；`emit_started` / `emit_ended` 通过 `ParentLink.subagent_tx` try_send 事件，尽力而为，通道满则丢弃而不阻塞子执行（`subagent.rs:50-82`）。

## 9. 错误与错误分析

- `AgentError` 共 11 类（`src/error.rs:4-60`）：Validation（确定性可复现拒绝）、ExecutionTimeout、Cancelled、ConcurrencySaturated（瞬时饱和）、HierarchyLimitReached（深度超限）、ErrorPatternTripped（重复错误熔断）、ContextBudgetExhausted（压缩后仍上下文超限）等。
- `error_analysis.rs` 把错误映射为结构化 `ErrorAnalysis`（kind/error_type/retryable/recovery_action，`error_analysis.rs:15-21`）：`analyze_error` 覆盖全部 AgentError 分支；`tool_error_analysis` / `llm_error_analysis` / `shared_error_analysis` 分别处理三类来源（`error_analysis.rs:66, 173, 282`）；HTTP 状态映射独立区分限流/上游故障与普通超时（`error_analysis.rs:39-64`）。
- 辅助设施：`decide_retry`（跨迭代重试决策，无重试策略时回落到 `RETRY_LIMIT_FALLBACK`，`error_analysis.rs:453-`）、`find_root_cause` / `get_error_chain`（`error_analysis.rs:491-`）、`analyze_error_pattern` / `get_recommended_recovery_action`（`error_analysis.rs:543-, 577-`）。

## 10. 常量：`constants.rs`

| 常量 | 值 | 语义 |
|------|----|------|
| `AGENT_MAX_ITERATIONS_CAP` | 1000 | max_iterations 硬上限，防止失控循环（`constants.rs:4`） |
| `DEFAULT_MAX_ITERATIONS` | 10 | 未配置时的默认值（`constants.rs:7`） |
| `RETRY_LIMIT_FALLBACK` | 3 | 无重试策略时的重试预算与熔断阈值回落值（`constants.rs:13`） |
