# wf-workflow 模块文档

> 范围：`crates/engine/wf-workflow`（engine 层，位于 `wf-agent` 之上，是 engine 层的最顶层 crate）。
> 定位：整理该 crate 当前已实现的功能，作为架构与维护参考。所有结论均附 `file:line` 依据（相对 `crates/engine/wf-workflow/src/`，取自当前 HEAD）。

## 模块定位

wf-workflow 实现了**图工作流的执行引擎**：给定一张 `WorkflowGraphStructure` 图与输入，引擎完成"静态校验 → 图预处理 → 主循环导航 → 节点分发执行 → 变量写回 → 终态落库"的完整执行链路，并提供变量系统、错误分支路由、触发器、checkpoint/恢复、执行预算、可观测性等支撑设施。agent 循环（`wf-agent`）以 AGENT_LOOP 节点的形式嵌入图执行。

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-architecture.md](01-architecture.md) | 执行入口、三层协调器（生命周期 / 主循环 / 单节点）、导航与死循环防护、超时与中断 |
| [02-node-handlers.md](02-node-handlers.md) | 全部内置节点处理器：控制流、循环、并行、LLM、Agent、脚本、交互、RAG 等 |
| [03-variables-and-context.md](03-variables-and-context.md) | 变量系统：`${...}` 表达式层、类型转换、命名消息数组、变量注入与映射 |
| [04-validation-and-error-branches.md](04-validation-and-error-branches.md) | 图校验规则集、`ValidatedGraph` 类型保证、引用闭包、错误路由表与隔离错误分支作用域 |
| [05-triggers.md](05-triggers.md) | 事件驱动触发器：扇入订阅、候选匹配、竞争裁决、开火门控与动作分发 |
| [06-checkpoint-persistence.md](06-checkpoint-persistence.md) | 检查点策略与协调器、执行记录落库、预算跨挂起/恢复、交互挂起、注册中心与可观测性 |

## 能力总览

| 能力 | 关键类型 | 入口文件 |
|------|----------|----------|
| 对外执行门面 | `WorkflowExecutor` / `WorkflowRunRequest` | `src/executor.rs:17`、`src/executor.rs:38` |
| 应用侧注册/回调平面 | `WorkflowExecutionCallback` | `src/execution_callback.rs:39` |
| 生命周期协调 | `WorkflowLifecycleCoordinator` | `src/coordinator/lifecycle.rs:25` |
| 主循环协调 | `WorkflowCoordinator` | `src/coordinator/workflow.rs:29` |
| 单节点调度 | `NodeCoordinator` | `src/coordinator/node.rs:53` |
| 活体执行实体 | `WorkflowExecutionEntity` | `src/entity.rs:127` |
| 执行状态机 | `WorkflowExecutionState` | `src/state.rs:70` |
| 图遍历与错误路由表 | `GraphTraversal` / `ErrorRouteTable` | `src/graph.rs:8`、`src/error_branch.rs:104` |
| 图校验 | `GraphValidator` / `ValidatedGraph` | `src/validation.rs:72`、`src/validation.rs:46` |
| 引用闭包校验 | `validate_reference_closure` | `src/reference_closure.rs:120` |
| 节点处理器注册表 | `HandlerRegistry` | `src/handler.rs:51` |
| 变量求值 | `ExprEvaluator` / `VariableResolver` | `src/variable.rs:50`、`src/variable.rs:621` |
| 命名消息数组 | `register_context` / `get_context_history` | `src/message_context.rs:209`、`src/message_context.rs:246` |
| 错误分支作用域 | `ErrorBranchScope` | `src/error_branch.rs:37` |
| 触发器监听 | `TriggerEventListener` | `src/trigger/listener.rs:59` |
| 开火门控 | `TriggerGovernor` | `src/trigger/governor.rs:29` |
| 检查点集成 | `WorkflowCheckpointIntegration` / `NodeCheckpointStrategy` | `src/checkpoint/coordinator.rs:30`、`src/checkpoint/strategy.rs:51` |
| 执行记录构建 | `build_workflow_execution` | `src/persistence.rs:26` |
| 执行预算接续 | `persist_totals` / `continuation_options` | `src/execution_budgets.rs:25`、`src/execution_budgets.rs:170` |
| 压缩回写注册表 | `ExecutionContextRegistry` | `src/execution_context.rs:147` |
| 图/脚本注册中心 | `WorkflowRegistry` | `src/registry.rs:128` |
| Hook 发射器 | `WorkflowHookEmitter` | `src/hook.rs:14` |
| 数据降级事件 | `emit_data_degradation` | `src/degradation.rs:21` |

## 集成测试覆盖

`tests/` 目录按主题组织：agent 循环嵌入与压缩（`agent_loop_e2e.rs`、`agent_compression_e2e.rs`）、上下文处理与压缩（`context_processor_e2e.rs`、`context_compression_e2e.rs`）、fork/join 与同步（`fork_join_sync_e2e.rs`、`fork_registry_e2e.rs`）、错误分支（`error_branch_e2e.rs`）、循环（`loop_e2e.rs`）、子图（`subgraph_e2e.rs`）、checkpoint（`checkpoint_e2e.rs`、`subexecution_checkpoint_e2e.rs`）、持久化重建（`persistence_rebuild.rs`）、校验（`validation_e2e.rs`）、变量（`variable_e2e.rs`）、路由与决策（`route_e2e.rs`、`decision_config_e2e.rs`）、工具可见性（`tool_visibility_e2e.rs`）、降级（`barrier_degradation.rs`）、LLM mock（`llm_mock_e2e.rs`），以及注册表负向用例（`handler_registry_negative.rs`、`event_publish_negative.rs`）。
