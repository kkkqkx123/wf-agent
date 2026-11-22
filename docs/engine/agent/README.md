# wf-agent 模块文档

> 范围：`crates/engine/wf-agent`（engine 层，位于 `wf-execution-shared` 之上、`wf-workflow` 之下）。
> 定位：整理该 crate 当前已实现的功能，作为架构与维护参考。所有结论均附 `file:line` 依据，取自当前 HEAD。

## 模块定位

wf-agent 实现了**自主 LLM agent 循环的执行引擎**：给定一个实体（entity）配置与初始输入，引擎驱动"请求组装 → LLM 调用 → 工具执行 → 结果落地"的迭代循环，并提供完整的生命周期管理、审批、检查点/恢复、会话压缩、事件流、触发式子执行等支撑设施。

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-architecture.md](01-architecture.md) | 三层协调器架构（生命周期 / 执行 / 迭代）与主循环控制流 |
| [02-state-persistence.md](02-state-persistence.md) | `AgentLoopState` 状态机、执行记录落库、注册中心与容量/超时管理 |
| [03-tools.md](03-tools.md) | 工具路由、快照冻结、可见性、审批、discoverable 代理执行与 Decision 决策模式 |
| [04-checkpoint.md](04-checkpoint.md) | 检查点策略、协调器、快照构建与三种恢复模式 |
| [05-compression.md](05-compression.md) | 会话压缩：后台消费者、版本化写回、背压与强制压缩 |
| [06-entrances.md](06-entrances.md) | 对外入口 `AgentLoopExecutor`、门禁、触发式执行、子代理、hook、校验与错误分析 |

## 能力总览

| 能力 | 关键类型 | 入口文件 |
|------|----------|----------|
| 生命周期协调 | `AgentLoopCoordinator` | `src/coordinator/lifecycle.rs` |
| 迭代外循环（暂停/停止/重试） | `AgentExecutionCoordinator` | `src/coordinator/execution.rs` |
| 单次迭代骨架 | `AgentIterationCoordinator` | `src/coordinator/iteration.rs` |
| 运行时状态与快照 | `AgentLoopState` / `AgentLoopStateSnapshot` | `src/state.rs` |
| 执行记录落库 | `build_agent_execution` | `src/persistence.rs` |
| 对外执行门面 | `AgentLoopExecutor` | `src/executor.rs` |
| 实体注册/并发/深度闸门 | `AgentLoopRegistry` / `AgentCapacityGate` | `src/registry.rs`、`src/capacity.rs` |
| 工具暴露与路由 | `ToolRouter` / `ToolSnapshot` | `src/tool_router.rs`、`src/tool_snapshot.rs` |
| 工具执行与审批 | `ToolExecutionCoordinator` / `ToolApprovalGate` | `src/coordinator/tool.rs` |
| 工具可见性拦截 | `VariableBackedVisibilityStore` | `src/visibility.rs` |
| 检查点 | `AgentCheckpointStrategy` / `AgentCheckpointIntegration` | `src/checkpoint/` |
| 会话压缩 | `spawn_conversation_compression_consumer` | `src/conversation_compression.rs` |
| 事件流 | `AgentEventStream` / `AgentStreamEvent` | `src/stream.rs` |
| 触发式子执行 | `TriggeredAgentExecutionManager` | `src/trigger.rs` |
| 校验与错误分析 | `AgentLoopValidator` / `analyze_error` | `src/validation.rs`、`src/error_analysis.rs` |

## 集成测试覆盖

`tests/` 目录按主题组织：核心循环（`agent_loop.rs`）、生命周期终态与暂停恢复（`lifecycle_states.rs`）、审批（`approval.rs`）、可见性（`visibility.rs`）、工具边界（`tool_boundaries.rs`）、工具发现（`tool_discovery.rs`）、流式（`stream.rs` / `stream_lifecycle.rs`）、hook（`hook.rs`）、检查点（`checkpoint.rs`）、超时（`timeout.rs`）、token（`token.rs`）、持久化（`persistence.rs`）、触发式执行（`trigger.rs`）、校验（`validation.rs`）。
