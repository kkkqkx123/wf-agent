# 事件系统分析：TS vs RS

## TS 事件系统的作用

TS 的事件系统（`EventRegistry` / `ExecutionEventEmitter`）是**执行引擎的核心同步原语**，而非可观测性的附属品。

| 用途 | 机制 | 是否阻塞 |
|------|------|---------|
| Fork/Join 同步 | `waitForBranchCompletion()` 阻塞等待 `WORKFLOW_EXECUTION_COMPLETED` 事件 | 阻塞 |
| 暂停/恢复/取消 | `waitForWorkflowExecutionPaused()` 等待 `PAUSED` 事件 | 阻塞 |
| 工具审批 | `waitForToolApproval()` 等待 `TOOL_APPROVAL_RESPONDED` 事件 | 阻塞 |
| 流式传输 | `StreamingExecutionBase` 订阅事件类型并 yield 给客户端 | 异步生成器 |
| 可观测性 | Metrics、日志、历史查询 | 非阻塞 |

JS 缺少 Rust 的并发原语，所以 TS 用事件驱动来模拟阻塞等待和状态通知。Fork/Join、暂停/恢复、取消、工具审批这些流程如果移除事件系统会直接崩溃。

## RS 是否需要迁移到事件驱动架构？

**不需要。核心执行层面，Rust 已有更合适的替代方案，不应迁移到事件驱动架构。**

### 对比

| 功能 | TS（事件驱动） | RS（当前实现） | 评价 |
|------|--------------|---------------|------|
| Fork/Join 同步 | `waitForEvent(WORKFLOW_EXECUTION_COMPLETED)` | 未实现（P1 待完成） | — |
| 暂停/恢复 | `waitForEvent(PAUSED)` | `tokio::sync::watch` | RS 更优：watch 保持最新值，不会漏信号 |
| 取消 | `waitForEvent(CANCELLED)` | `CancellationToken` | RS 更优：专用原语，协作式取消 |
| 通知唤醒 | `waitForEvent(RESUMED)` | `Notify` | RS 更优：轻量、不分配 |
| 工具审批 | `waitForEvent(APPROVAL_RESPONDED)` | `oneshot` 通道 | RS 更优：一次性响应天然匹配 oneshot |
| 流式传输 | 订阅事件 | 未实现 | 如需流式 API，可加事件消费者 |

### 核心判断

TS 的事件驱动是被 JS 语言限制逼迫的——JS 没有 `watch`、`CancellationToken`、`Notify`，只能用 EventEmitter + Promise 来拼凑同步。Rust 有更好的工具，强行迁移到事件驱动架构是开倒车。

### 结论

- Fork/Join 的同步应该用 `watch` 或 `Notify`，而不是事件广播
- 暂停/恢复/取消已正确使用 `watch` + `CancellationToken`，保持不动
- EventBus 可以保留为**可选的可观测性通道**（用于未来流式 API、事件历史），但不应该成为核心执行流程的依赖
- 当前 EventBus 零订阅者的状态是合理的——先把核心逻辑跑通，再把事件作为对外暴露的接口（streaming API）加上去
