# Execution 模块迁移方案

## 一、迁移范围概览

### 1.1 TypeScript 源模块统计

| 域 | 源文件数 | 测试文件数 | 关键子目录 |
|----|---------|-----------|-----------|
| Workflow Execution | 84 | 57 | coordinators, executors, handlers, factories, barriers, state-managers |
| Agent Execution | 26 | 17 | coordinators, executors, handlers, factories |
| Shared Execution | 6 | 4 | execution-pool, execution-queue, hierarchy-integrity-service |
| Tool Services | ~30 | ~12 | services/tools, services/executors, mcp, auto-approval |
| 支撑模块 (entity/state/registry) | ~77 | — | — |
| 类型定义 | 19 | — | workflow-execution, agent-execution, execution |
| **合计** | **~272** | **~110** | — |

### 1.2 已有 Rust 基础设施

| Crate | 已有能力 | 对 Executor 的价值 |
|-------|---------|-------------------|
| `wf-types` | 全部执行类型定义（WorkflowExecution, NodeExecution, ForkJoinContext, FailurePolicy, 20种 RuntimeNode 等） | 类型基础，无需重复定义 |
| `wf-core` | EventBus (broadcast), Registry (DashMap), NodeStateMachine, WorkflowStateMachine | 事件传播、状态转换、运行时注册表 |
| `wf-checkpoint` | CheckpointCoordinator, WorkflowCheckpointCoordinator, Delta/Restore | 执行中自动 checkpoint、失败恢复 |
| `wf-storage` | WorkflowExecutionStorageAdapter + 三后端 | 执行记录持久化 |
| `wf-common` | Error, Result, Id, Time | 基础工具 |
| `wf-runtime` | Bootstrap, Lifecycle, StorageManager | 运行时集成宿主 |

### 1.3 缺失的 Rust 实现

| 缺失模块 | 对应 TS 模块 | 目标 Crate | 复杂度 |
|---------|-------------|-----------|--------|
| NodeHandler trait + 21 种实现 | `node-handlers/*.ts` | `wf-executor` | 高 |
| GraphBuilder / GraphTraversal | `WorkflowGraphStructure` 使用逻辑 | `wf-executor` | 中 |
| WorkflowCoordinator | `WorkflowExecutionCoordinator` | `wf-executor` | 高 |
| NodeCoordinator | `NodeExecutionCoordinator` | `wf-executor` | 高 |
| WorkflowLifecycleCoordinator | `WorkflowLifecycleCoordinator` | `wf-executor` | 中 |
| ForkJoinHandler | `fork-handler.ts` + `join-handler.ts` + `SyncBarrier` | `wf-executor` | 高 |
| VariableResolver | `VariableCoordinator` | `wf-executor` | 中 |
| ConditionEvaluator | `routeHandler` 中的条件判断 | `wf-executor` | 中 |
| ExecutionPool / ExecutionQueue | `execution-pool.ts` + `execution-queue.ts` | `wf-executor` | 中 |
| AgentLoopCoordinator | `AgentLoopCoordinator` | `wf-executor` | 高 |
| AgentIterationCoordinator | `AgentIterationCoordinator` | `wf-executor` | 高 |
| StateManager 实现 | `WorkflowExecutionState` 等 | `wf-executor` | 中 |
| TimeoutManager | `TimeoutManager` | `wf-executor` | 低 |
| ErrorChainManager | `ErrorChainManager` | `wf-executor` | 低 |
| IToolExecutor + ToolRegistry | `tool-registry.ts` + `IToolExecutor` | `wf-tools` | 中 |
| ToolCallExecutor | `tool-call-executor.ts` | `wf-tools` | 中 |
| MCP Client + Transports | `mcp-client.ts` + transport层 | `wf-tools` | 中 |
| ToolApprovalCoordinator | `tool-approval-coordinator.ts` | `wf-tools` | 低 |
| BaseExecutor + 参数验证 | `base.ts` + `ParameterValidator` | `wf-tools` | 低 |
| Builtin tools (call_agent, execute_workflow) | `builtin/` 中依赖执行能力的工具 | `wf-executor` | 中 |

---

## 二、循环依赖分析与解耦设计

### 2.1 TS 如何避免循环依赖

TS 通过三层机制实现 Execution ↔ Tools 解耦：

```
┌─────────────────────────────────────────────────────────────────┐
│                        wf-executor                               │
│  AgentIterationCoordinator ──→ ToolExecutionCoordinator          │
│                                      │                           │
│                                      ▼                           │
│  BuiltinToolHandler ──────────→ ToolCallExecutor                 │
│  (call_agent, execute_workflow)     │                            │
│                                      ▼                           │
│                              ToolRegistry ◄──── 仅依赖接口        │
│                                  │                               │
└──────────────────────────────────┼───────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                         wf-tools                                  │
│  ToolRegistry ──→ IToolExecutor (trait)                          │
│       │                  │                                        │
│       │                  ├── StatelessExecutor                    │
│       │                  ├── RestExecutor                         │
│       │                  ├── McpExecutor ──→ McpClient            │
│       │                  └── BuiltinExecutor ──→ 回调 wf-executor │
│       │                                                          │
│       └── ToolApprovalCoordinator                                │
│                                                                  │
│  * wf-tools 不 import 任何 execution 模块                         │
│  * 内置工具通过回调注册表获得执行能力                                │
└─────────────────────────────────────────────────────────────────┘
```

**关键机制**:

1. **ToolRegistry 位于 wf-tools，但接受外部注册** — 不依赖 execution 模块
2. **BuiltinExecutor 通过回调执行** — `wf-tools` 定义 `BuiltinToolCallback` trait，`wf-executor` 实现并注册
3. **类型级引用使用 `import type`** — 编译期擦除，运行时无依赖
4. **DI 容器运行时解析** — `call_agent` handler 通过 `container.get(Identifiers.AgentLoopCoordinator)` 获取执行能力

### 2.2 Rust 解耦设计

```
wf-types  ←  wf-common
    ↓           ↓
wf-storage ← wf-config
    ↓           ↓
    └────→ wf-core ←────┬────→ wf-llm (Phase 4)
                          │
                          ├────→ wf-tools ←──── wf-executor
                          │         │                  │
                          │         │                  ├── 实现 IToolExecutor 回调
                          │         │                  ├── 注册 BuiltinExecutor
                          │         │                  └── 使用 ToolRegistry
                          │         │
                          │         └── 不依赖 wf-executor
                          │
                          └────→ wf-executor → wf-checkpoint
```

**核心原则**: `wf-tools` 不依赖 `wf-executor`。`wf-executor` 依赖 `wf-tools` 并实现其回调接口。

### 2.3 回调注册表模式

```rust
// ===== wf-tools/src/callback.rs =====
// 定义回调 trait，wf-executor 实现

#[async_trait]
pub trait ExecutionCallback: Send + Sync {
    /// 执行一个 Agent Loop（用于 call_agent 内置工具）
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput>;

    /// 触发一个 Workflow（用于 execute_workflow 内置工具）
    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput>;

    /// 查询执行状态（用于 query_workflow_status 内置工具）
    async fn query_execution_status(
        &self,
        execution_id: &str,
    ) -> ToolResult<ExecutionStatus>;

    /// 取消执行（用于 cancel_workflow 内置工具）
    async fn cancel_execution(
        &self,
        execution_id: &str,
    ) -> ToolResult<()>;
}

/// 全局回调实例（启动时设置，仅一次）
static CALLBACK: OnceCell<Arc<dyn ExecutionCallback>> = OnceCell::new();

pub fn register_execution_callback(callback: Arc<dyn ExecutionCallback>) -> ToolResult<()> {
    CALLBACK.set(callback).map_err(|_| ToolError::AlreadyRegistered)
}

pub fn get_execution_callback() -> Option<Arc<dyn ExecutionCallback>> {
    CALLBACK.get().cloned()
}
```

```rust
// ===== wf-executor/src/builtin_tools.rs =====
// 实现回调，注册到 wf-tools

pub struct ExecutorBuiltinCallback {
    workflow_coordinator_factory: Arc<dyn WorkflowCoordinatorFactory>,
    agent_loop_coordinator_factory: Arc<dyn AgentLoopCoordinatorFactory>,
    execution_registry: Arc<dyn ExecutionRegistry>,
}

#[async_trait]
impl ExecutionCallback for ExecutorBuiltinCallback {
    async fn execute_agent_loop(&self, config, input) -> ToolResult<AgentLoopOutput> {
        let mut coord = self.agent_loop_coordinator_factory.create(config).await?;
        coord.execute(input).await.map_err(Into::into)
    }
    // ... 其他实现
}

// 启动时注册
pub fn register_builtin_tools() -> ExecutorResult<()> {
    let callback = Arc::new(ExecutorBuiltinCallback { ... });
    wf_tools::register_execution_callback(callback)
        .map_err(ExecutorError::ToolRegistrationFailed)?;
    Ok(())
}
```

---

## 三、crate 结构设计

### 3.1 `wf-tools` 文件布局

```
crates/wf-tools/
├── Cargo.toml
├── src/
│   ├── lib.rs                       # include!("wf_tools.rs")
│   ├── wf_tools.rs                  # 根模块
│   ├── error.rs                     # ToolError 枚举
│   ├── callback.rs                  # ExecutionCallback trait + 全局注册
│   ├── registry/
│   │   ├── mod.rs → registry.rs
│   │   └── tool_registry.rs         # ToolRegistry (IToolExecutor 查找)
│   ├── executor/
│   │   ├── mod.rs → executor.rs
│   │   ├── trait.rs                 # IToolExecutor trait
│   │   ├── base.rs                  # BaseExecutor (参数验证 + 重试 + 超时)
│   │   ├── stateless.rs             # StatelessExecutor
│   │   ├── stateful.rs              # StatefulExecutor
│   │   ├── rest.rs                  # RestExecutor
│   │   ├── builtin.rs               # BuiltinExecutor (通过回调调用 wf-executor)
│   │   └── mcp.rs                   # McpExecutor
│   ├── approval/
│   │   ├── mod.rs → approval.rs
│   │   └── coordinator.rs           # ToolApprovalCoordinator
│   ├── mcp/
│   │   ├── mod.rs → mcp.rs
│   │   ├── client.rs                # McpClient
│   │   ├── transport/
│   │   │   ├── mod.rs → transport.rs
│   │   │   ├── trait.rs             # IMcpTransport
│   │   │   ├── stdio.rs             # StdioTransport
│   │   │   ├── sse.rs               # SseTransport
│   │   │   └── streamable_http.rs   # StreamableHttpTransport
│   │   └── connection.rs            # McpConnectionManager + McpServerRegistry
│   └── tool_call.rs                 # ToolCallExecutor (桥接协调器 → 注册表)
```

**`wf-tools` Cargo.toml 依赖**:
```toml
[dependencies]
wf-types = { path = "../wf-types" }
wf-common = { path = "../wf-common" }
wf-storage = { path = "../wf-storage" }

async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }  # HTTP client for REST/MCP
serde_json = "1"
thiserror = "1"
tracing = "0.1"
once_cell = "1"
```

**关键**: `wf-tools` **不依赖** `wf-executor`、`wf-core`、`wf-checkpoint`。仅依赖基础 crate。

### 3.2 `wf-executor` 文件布局

```
crates/wf-executor/
├── Cargo.toml
├── src/
│   ├── lib.rs                       # include!("wf_executor.rs")
│   ├── wf_executor.rs               # 根模块
│   ├── error.rs                     # ExecutorError 枚举
│   ├── context.rs                   # ExecutorContext, NodeExecutionContext
│   ├── coordinator/
│   │   ├── mod.rs → coordinator.rs
│   │   ├── workflow.rs              # WorkflowCoordinator
│   │   ├── node.rs                  # NodeCoordinator
│   │   ├── lifecycle.rs             # WorkflowLifecycleCoordinator
│   │   ├── agent_loop.rs            # AgentLoopCoordinator
│   │   └── agent_iteration.rs       # AgentIterationCoordinator
│   ├── handler/
│   │   ├── mod.rs → handler.rs
│   │   ├── node_handler.rs          # NodeHandler trait
│   │   ├── registry.rs              # HandlerRegistry
│   │   ├── start_end.rs             # Start/End handlers
│   │   ├── fork_join.rs             # Fork/Join handlers
│   │   ├── route.rs                 # Route handler
│   │   ├── llm.rs                   # LLM handler (Phase 4 实现)
│   │   ├── agent_loop.rs            # Agent loop handler
│   │   ├── subgraph.rs              # Subgraph handler
│   │   ├── script.rs                # Script handler (Phase 4 实现)
│   │   ├── user_interaction.rs      # User interaction handler
│   │   ├── variable.rs              # Variable handler
│   │   ├── loop.rs                  # Loop start/end handlers
│   │   ├── context_processor.rs     # Context processor handler
│   │   ├── tool_visibility.rs       # Tool visibility handler
│   │   ├── sync.rs                  # Sync handler
│   │   ├── embed.rs                 # Embed boundary handlers
│   │   └── trigger_origin.rs        # Trigger origin handlers
│   ├── builtin_tools.rs             # 实现 ExecutionCallback + 注册内置工具
│   ├── graph/
│   │   ├── mod.rs → graph.rs
│   │   ├── traversal.rs             # 图遍历 + 就绪节点检测
│   │   └── builder.rs               # 从 WorkflowGraphStructure 构建执行图
│   ├── state/
│   │   ├── mod.rs → state.rs
│   │   ├── workflow_execution.rs    # WorkflowExecutionState 实现
│   │   ├── fork_join.rs             # ForkJoinState 实现
│   │   └── error_chain.rs           # ErrorChainManager 实现
│   ├── variable/
│   │   ├── mod.rs → variable.rs
│   │   ├── resolver.rs              # 变量解析 + 插值
│   │   └── store.rs                 # VariableStore (DashMap)
│   ├── barrier/
│   │   ├── mod.rs → barrier.rs
│   │   └── sync.rs                  # SyncBarrier (Fork/Join 同步)
│   ├── condition/
│   │   ├── mod.rs → condition.rs
│   │   └── evaluator.rs             # 条件表达式求值
│   ├── retry/
│   │   ├── mod.rs → retry.rs
│   │   ├── budget.rs                # RetryBudget
│   │   └── policy.rs                # 重试策略执行
│   ├── timeout/
│   │   ├── mod.rs → timeout.rs
│   │   └── manager.rs               # TimeoutManager
│   └── pool/
│       ├── mod.rs → pool.rs
│       ├── execution_pool.rs        # ExecutionPool
│       └── execution_queue.rs       # ExecutionQueue
```

**`wf-executor` Cargo.toml 依赖**:
```toml
[dependencies]
wf-types = { path = "../wf-types" }
wf-common = { path = "../wf-common" }
wf-core = { path = "../wf-core" }
wf-checkpoint = { path = "../wf-checkpoint" }
wf-storage = { path = "../wf-storage" }
wf-tools = { path = "../wf-tools" }

async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["rt"] }
futures = "0.3"
serde_json = "1"
thiserror = "1"
tracing = "0.1"
evalexpr = "11"                       # 条件表达式求值
```

---

## 四、核心 Trait 设计

### 4.1 `NodeHandler` — 节点执行接口

```rust
#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> StaticNodeType;
    async fn execute(&self, ctx: &mut NodeExecutionContext) -> ExecutorResult<NodeExecutionResult>;
}
```

**TS 对应**: `node-handlers/index.ts` 中的 `Record<string, NodeHandlerFn>` 映射

**Rust 调整**:
- TS 使用 `Record<string, NodeHandlerFn>` 动态查找，Rust 使用 `HashMap<StaticNodeType, Arc<dyn NodeHandler>>` 注册
- 优先 `match` 静态分发，plugin 场景使用 `Arc<dyn NodeHandler>` 动态分发
- 每个 handler 为独立 struct，实现 `NodeHandler` trait

### 4.2 `IToolExecutor` — 工具执行接口（位于 wf-tools）

```rust
#[async_trait]
pub trait IToolExecutor: Send + Sync {
    async fn execute(
        &self,
        tool: &Tool,
        parameters: &serde_json::Value,
        options: &ToolExecutionOptions,
        execution_id: &Id,
        context: &HashMap<String, serde_json::Value>,
    ) -> ToolResult<ToolExecutionResult>;

    fn executor_type(&self) -> &str;

    async fn cleanup(&self) -> ToolResult<()> { Ok(()) }
}
```

### 4.3 `ExecutionCallback` — 解耦回调（位于 wf-tools，由 wf-executor 实现）

```rust
#[async_trait]
pub trait ExecutionCallback: Send + Sync {
    async fn execute_agent_loop(&self, config: AgentLoopConfig, input: AgentLoopInput) -> ToolResult<AgentLoopOutput>;
    async fn execute_workflow(&self, workflow_id: &str, input: WorkflowInput) -> ToolResult<WorkflowOutput>;
    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus>;
    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()>;
}
```

### 4.4 `ExecutionCoordinator` — 协调器抽象

```rust
#[async_trait]
pub trait ExecutionCoordinator: Send + Sync {
    async fn execute(&mut self) -> ExecutorResult<()>;
    async fn pause(&mut self) -> ExecutorResult<()>;
    async fn resume(&mut self) -> ExecutorResult<()>;
    async fn cancel(&mut self) -> ExecutorResult<()>;
}
```

---

## 五、节点处理器映射表

| TS Handler | Rust 实现 | 所在 Crate | 依赖 |
|-----------|----------|-----------|------|
| `startHandler` / `endHandler` | `StartNodeHandler` / `EndNodeHandler` | `wf-executor` | 无 |
| `llmHandler` | `LlmNodeHandler` | `wf-executor` | `wf-llm` (Phase 4) |
| `agentLoopHandler` | `AgentLoopNodeHandler` | `wf-executor` | 内部 AgentLoopCoordinator |
| `subgraphHandler` | `SubgraphNodeHandler` | `wf-executor` | 内部递归 |
| `forkHandler` | `ForkNodeHandler` | `wf-executor` | `wf-checkpoint` |
| `joinHandler` | `JoinNodeHandler` | `wf-executor` | `wf-checkpoint` |
| `routeHandler` | `RouteNodeHandler` | `wf-executor` | `ConditionEvaluator` |
| `loopStartHandler` / `loopEndHandler` | `LoopStartHandler` / `LoopEndHandler` | `wf-executor` | 内部状态 |
| `scriptHandler` | `ScriptNodeHandler` | `wf-executor` | `wf-sandbox` (Phase 4) |
| `userInteractionHandler` | `UserInteractionNodeHandler` | `wf-executor` | `EventBus` |
| `variableHandler` | `VariableNodeHandler` | `wf-executor` | `VariableResolver` |
| `contextProcessorHandler` | `ContextProcessorNodeHandler` | `wf-executor` | 内部 |
| `toolVisibilityHandler` | `ToolVisibilityNodeHandler` | `wf-executor` | 内部 |
| `syncHandler` | `SyncNodeHandler` | `wf-executor` | `SyncBarrier` |
| `embedStartHandler` / `embedEndHandler` | `EmbedBoundaryHandler` | `wf-executor` | 内部 |
| `interactiveScriptHandler` | `InteractiveScriptNodeHandler` | `wf-executor` | `wf-sandbox` |
| `continueFromTriggerHandler` / `startFromTriggerHandler` | `TriggerOriginHandler` | `wf-executor` | 内部 |
| `call_agent` (builtin tool) | `execute_agent_loop` callback | `wf-executor` | 内部 AgentLoopCoordinator |
| `execute_workflow` (builtin tool) | `execute_workflow` callback | `wf-executor` | 内部 WorkflowCoordinator |
| `query_workflow_status` (builtin tool) | `query_execution_status` callback | `wf-executor` | 内部 ExecutionRegistry |
| `cancel_workflow` (builtin tool) | `cancel_execution` callback | `wf-executor` | 内部 CancellationToken |

---

## 六、Rust 特性调整分析

### 6.1 动态分发 → 静态分发优先

**TS 模式**: `Record<string, NodeHandlerFn>` 运行时字符串查找

**Rust 方案**:
- 启动时构建 `HashMap<StaticNodeType, Arc<dyn NodeHandler>>`
- 热点路径（START/END/ROUTE）可用 `match` 内联
- Plugin 系统预留 `Arc<dyn NodeHandler>` 扩展点
- 遵循项目约定："Minimize `dyn`, prefer concrete types"

### 6.2 DI Container → 显式依赖注入

**TS 模式**: `GlobalContext.container.get(Identifiers.WorkflowRegistry)` 服务定位

**Rust 方案**:
- 所有协调器通过构造函数接收 `Arc<dyn Trait>` 依赖
- 使用 `ExecutorContext` 结构体集中持有共享引用：

```rust
pub struct ExecutorContext {
    event_bus: Arc<EventBus>,
    checkpoint_coordinator: Arc<dyn CheckpointCoordinator>,
    workflow_adapter: Arc<dyn WorkflowStorageAdapter>,
    execution_adapter: Arc<dyn WorkflowExecutionStorageAdapter>,
    node_handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    tool_registry: Arc<ToolRegistry>,
    cancellation_token: CancellationToken,
}
```

- 消除全局可变状态，所有依赖显式传递
- 回调注册表（`ExecutionCallback`）是唯一的"全局"点，通过 `OnceCell` 实现，启动时设置后不可变

### 6.3 Promise.allSettled → futures::join_all + Result 聚合

**TS 模式**: `Promise.allSettled(branches)` 并行执行所有 fork 分支

**Rust 方案**:
- 使用 `futures::future::join_all` 或 `tokio::spawn` + `JoinSet`
- 每个分支返回 `Result<BranchResult, ExecutorError>`
- 失败策略在 `JoinResult` 上实现：

```rust
pub enum FailureStrategy {
    FailFast,
    ContinueOnError,
    FailOnThreshold { threshold: f64 },
}

impl FailureStrategy {
    fn evaluate(&self, results: &[BranchResult]) -> ForkOutcome { ... }
}
```

### 6.4 AsyncGenerator → tokio_stream::Stream

**TS 模式**: `AsyncGenerator<AgentLoopStreamEvent>` yield 流式事件

**Rust 方案**:
- 实现 `Stream<Item = AgentLoopStreamEvent>` trait
- 使用 `tokio_stream::wrappers::ReceiverStream` 桥接 channel

### 6.5 AbortController → CancellationToken

**TS 模式**: `AbortSignal` / `AbortController` 级联取消

**Rust 方案**:
- `tokio_util::sync::CancellationToken` 提供树形取消
- 每个执行实例持有 `CancellationToken`，子执行 clone 子 token
- `tokio::select!` 中监听 `cancelled()` future

### 6.6 StateManager 快照 → Serialize/Deserialize

**TS 模式**: `StateManager<TSnapshot>` 接口

**Rust 方案**:
- 利用 `serde` 直接序列化/反结构化状态结构体
- `CheckpointCoordinator` 已提供 `WorkflowExecutionStateSnapshot`

### 6.7 深度克隆 → Clone + serde

**TS 模式**: `structuredClone(variables)`

**Rust 方案**:
- 变量结构体 derive `Clone`
- 跨执行边界使用 `serde_json::to_value` + `from_value` 做深拷贝

### 6.8 超时管理 → tokio::time

**TS 模式**: `TimeoutManager` 基于 `setTimeout`/`setInterval`

**Rust 方案**:
- `tokio::time::sleep` + `tokio::time::timeout`
- `tokio::select!` 组合超时分支

### 6.9 错误链 → thiserror 嵌套

**TS 模式**: `ErrorChainManager` 手动维护父子错误关系

**Rust 方案**:
- `thiserror` 定义 `ExecutorError` 枚举
- `#[from]` 自动转换底层错误（CoreError, CheckpointError, StorageError, ToolError）
- `backtrace` 字段提供调用链

### 6.10 Record<string, unknown> → serde_json::Value

**TS 模式**: `handlerContext: Record<string, unknown>`

**Rust 方案**:
- 节点输出使用 `serde_json::Value`
- 强类型上下文使用具名 struct（`NodeExecutionContext`）

---

## 七、协调器层次结构

```
┌─────────────────────────────────────────────────────────────┐
│                  WorkflowLifecycleCoordinator                 │
│  (创建实体 → 注册 → 执行 → 清理)                                │
│  TS: workflow-lifecycle-coordinator.ts (620 行)               │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                  WorkflowCoordinator                          │
│  (节点循环 + 导航 + 重试策略)                                    │
│  TS: workflow-execution-coordinator.ts (574 行)               │
└──────────┬──────────────────────────────┬───────────────────┘
           │                              │
┌──────────▼──────────┐   ┌───────────────▼──────────────────┐
│ NodeCoordinator     │   │ AgentLoopCoordinator              │
│ (单节点: hooks,      │   │ (Agent 生命周期 + 迭代循环)         │
│  checkpoint, retry) │   │ TS: agent-loop-coordinator.ts     │
│ TS: node-execution-  │   │     (1452+ 行)                    │
│ coordinator.ts       │   └───────────────┬──────────────────┘
│ (905 行)             │                   │
└─────────────────────┘   ┌───────────────▼──────────────────┐
                          │ AgentIterationCoordinator         │
                          │ (单次迭代: hooks→LLM→tools)       │
                          │ TS: agent-iteration-coordinator.ts│
                          │     (757 行)                      │
                          └──────────────────────────────────┘
```

---

## 八、分步实施计划

### Phase 3-A: wf-tools 基础（1 周）

**目标**: 工具执行基础设施

| 任务 | 产出 | 依赖 |
|------|------|------|
| 创建 `wf-tools` crate | Cargo.toml + 模块骨架 | — |
| 定义 `ToolError` | 错误枚举 | wf-common |
| 定义 `IToolExecutor` trait | 工具执行接口 | — |
| 实现 `BaseExecutor` | 参数验证 + 重试 + 超时 | — |
| 实现 `ToolRegistry` | 执行器注册表 | IToolExecutor |
| 实现 `StatelessExecutor` / `RestExecutor` | 无状态/REST 执行器 | reqwest |
| 实现 `ToolCallExecutor` | 桥接协调器 → 注册表 | ToolRegistry |
| 实现 `ToolApprovalCoordinator` | 审批协调 | — |
| 定义 `ExecutionCallback` trait | 解耦回调接口 | — |

**验收**: `cargo test -p wf-tools` 通过，可注册和执行 stateless/rest 工具。

### Phase 3-B: wf-tools MCP 客户端（0.5 周）

**目标**: MCP 协议支持

| 任务 | 产出 | 依赖 |
|------|------|------|
| 实现 `IMcpTransport` + 三种传输 | stdio/SSE/StreamableHTTP | tokio |
| 实现 `McpClient` | JSON-RPC 2.0 客户端 | IMcpTransport |
| 实现 `McpConnectionManager` | 连接池管理 | McpClient |
| 实现 `McpExecutor` | MCP 工具执行器 | McpClient |

**验收**: 可通过 stdio 连接 MCP 服务器并执行工具。

### Phase 3-C: wf-executor 骨架（1 周）

**目标**: 执行引擎基础 + 简单节点

| 任务 | 产出 | 依赖 |
|------|------|------|
| 创建 `wf-executor` crate | Cargo.toml + 模块骨架 | wf-tools |
| 定义 `ExecutorError` | 错误枚举 + `#[from]` | 全部上游 |
| 定义 `NodeHandler` trait | 节点执行接口 | — |
| 实现 `StartNodeHandler` / `EndNodeHandler` | 边界节点 | — |
| 实现 `HandlerRegistry` | 处理器注册表 | — |
| 定义 `ExecutorContext` / `NodeExecutionContext` | 上下文结构体 | — |
| 实现 `WorkflowCoordinator` 骨架 | 节点循环框架 | wf-core |
| 实现 `builtin_tools.rs` | 注册 ExecutionCallback | wf-tools callback |

**验收**: START→END 两节点 workflow 可执行，内置工具注册成功。

### Phase 3-D: 图遍历 + 变量 + 条件（1 周）

**目标**: 图执行能力 + 数据流

| 任务 | 产出 | 依赖 |
|------|------|------|
| 实现 `GraphTraversal` | 拓扑排序 + 就绪节点检测 | wf-types |
| 实现 `VariableStore` + `VariableResolver` | 变量读写 + 插值 | — |
| 实现 `VariableNodeHandler` | 变量操作节点 | VariableStore |
| 实现 `ConditionEvaluator` | 条件表达式求值 | evalexpr |
| 实现 `RouteNodeHandler` | 条件路由 | ConditionEvaluator |
| 实现 `ContextProcessorNodeHandler` | 上下文处理 | VariableResolver |
| 实现 `ToolVisibilityNodeHandler` | 工具可见性 | — |

**验收**: 含变量、条件路由的 workflow 可执行。

### Phase 3-E: Fork/Join + 并行（0.5 周）

**目标**: 并行执行能力

| 任务 | 产出 | 依赖 |
|------|------|------|
| 实现 `SyncBarrier` | 分支同步原语 | tokio::sync::Notify |
| 实现 `ForkNodeHandler` | 并行分支创建 + join_all | SyncBarrier |
| 实现 `JoinNodeHandler` | 分支聚合 + 失败策略 | SyncBarrier |
| 实现 `ForkJoinState` | Fork/Join 状态跟踪 | — |
| 实现 `WorkflowExecutionBuilder` | 子执行实体创建 | wf-storage |
| 实现 `SubgraphNodeHandler` | 子图执行 | WorkflowCoordinator 递归 |

**验收**: Fork/Join workflow 正确并行执行，失败策略生效。

### Phase 3-F: 协调器完整 + 生命周期（1 周）

**目标**: 完整 workflow 生命周期

| 任务 | 产出 | 依赖 |
|------|------|------|
| 完善 `WorkflowCoordinator` | 重试 + 错误处理 + 事件发射 | wf-core |
| 实现 `NodeCoordinator` | hooks + checkpoint + retry | wf-checkpoint |
| 实现 `WorkflowLifecycleCoordinator` | 创建→注册→执行→暂停→恢复→销毁 | 全部上述 |
| 实现 `WorkflowStateTransitor` | 原子状态转换 + 级联取消 | wf-core |
| 实现 `WorkflowExecutionState` | 状态快照 + 恢复 | wf-checkpoint |
| 实现 `ErrorChainManager` | 错误链记录 + 分析 | — |
| 实现 `TimeoutManager` | 超时管理 | tokio::time |
| 实现 `RetryBudget` | 重试预算 | — |

**验收**: 完整 workflow 生命周期通过集成测试。

### Phase 3-G: Agent 执行（1 周）

**目标**: Agent loop 执行能力

| 任务 | 产出 | 依赖 |
|------|------|------|
| 实现 `AgentLoopCoordinator` | Agent 生命周期管理 | — |
| 实现 `AgentIterationCoordinator` | 单次迭代 (hooks→LLM→tools) | wf-llm 接口 (Phase 4) |
| 实现 `AgentLoopNodeHandler` | workflow 中调用 agent | AgentLoopCoordinator |
| 实现 `AgentLoopExecutor` | Agent 执行入口 | — |
| 实现 `ExecutionPool` / `ExecutionQueue` | 执行池 + 队列 | tokio::sync::Semaphore |

**验收**: Agent loop 可执行单次迭代，支持暂停/恢复。

### Phase 3-H: Checkpoint 集成 + 恢复（0.5 周）

**目标**: 执行中自动 checkpoint + 失败恢复

| 任务 | 产出 | 依赖 |
|------|------|------|
| 集成 `CheckpointCoordinator` 到 `NodeCoordinator` | 节点前后自动 checkpoint | wf-checkpoint |
| 实现执行恢复流程 | 从 checkpoint 重建执行状态 | wf-checkpoint |
| 实现中断恢复 | 取消后从 checkpoint 恢复 | wf-checkpoint |

**验收**: 执行中断后可从最近 checkpoint 恢复并继续。

### Phase 3-I: 剩余节点 + 集成测试（0.5 周）

**目标**: 全部 21 种节点 + E2E 测试

| 任务 | 产出 | 依赖 |
|------|------|------|
| 实现 `LoopStartHandler` / `LoopEndHandler` | 循环节点 | — |
| 实现 `SyncNodeHandler` | 同步节点 | SyncBarrier |
| 实现 `EmbedBoundaryHandler` | 嵌入图边界 | — |
| 实现 `TriggerOriginHandler` | 触发源节点 | — |
| 实现 `UserInteractionNodeHandler` | 用户交互等待 | EventBus |
| E2E 测试 | 5+ 场景覆盖 | 全部 |

**验收**: 全部 21 种节点有对应 handler，E2E 测试通过。

---

## 九、关键设计决策

### 9.1 状态机包装策略

`wf-core` 的 `NodeStateMachine` 和 `WorkflowStateMachine` 是同步的，不含内部锁。Executor 需要跨 `.await` 点持有状态。

**方案**: `Arc<RwLock<WorkflowStateMachine>>` 包装，状态转换时写锁，事件发射时读锁。

### 9.2 事件发射时机

TS 在节点生命周期各阶段发射事件。Rust 实现应在相同节点发射相同事件类型，确保 `wf-core::EventBus` 订阅者兼容。

### 9.3 变量存储模型

**方案**: `Arc<DashMap<String, serde_json::Value>>` — 细粒度锁，并发读。DashMap 已在 wf-core 中使用。

### 9.4 条件表达式求值

**方案**: 使用 `evalexpr` crate — 支持变量引用、比较、逻辑运算，可注入变量上下文。

### 9.5 插件中间管

**方案**: 定义 `Middleware` trait，`Vec<Arc<dyn Middleware>>` 在 `NodeCoordinator` 中顺序调用。

```rust
#[async_trait]
pub trait NodeMiddleware: Send + Sync {
    async fn before_execute(&self, ctx: &NodeExecutionContext) -> ExecutorResult<()>;
    async fn after_execute(&self, ctx: &NodeExecutionContext, result: &NodeExecutionResult) -> ExecutorResult<()>;
    async fn on_error(&self, ctx: &NodeExecutionContext, error: &ExecutorError) -> ExecutorResult<()>;
}
```

### 9.6 回调注册表 vs 构造函数注入

**问题**: `wf-tools` 的 `BuiltinExecutor` 需要调用 `wf-executor` 的功能，但不能有编译期依赖。

**方案**: 使用 `OnceCell<Arc<dyn ExecutionCallback>>` 全局注册表。`wf-executor` 启动时注册实现，`wf-tools` 运行时通过它调用。

**替代方案**: 构造函数注入 — 每个 `BuiltinExecutor` 实例持有回调引用。但 `BuiltinExecutor` 由 `ToolRegistry` 统一创建，无法在构造时传入。

**推荐**: 全局注册表 — 简单、符合 TS 的 DI 容器模式、运行时开销可忽略（OnceCell 一次性写入，后续只读）。

---

## 十、与原方案（Phase 3）的对应关系

| 原方案条目 | 本方案对应 | 调整 |
|-----------|-----------|------|
| 3.1 工具执行器 | Phase 3-A + 3-B (wf-tools) | 独立 crate，不混入 wf-executor |
| 3.1 审批引擎 | Phase 3-A (wf-tools) | 同上 |
| 3.2 WorkflowCoordinator | Phase 3-C + 3-F | 分骨架和完整实现两步 |
| 3.2 NodeCoordinator | Phase 3-F | 依赖 checkpoint 集成 |
| 3.2 GraphBuilder | Phase 3-D | 独立实现 |
| 3.2 StateManager | Phase 3-F | 复用 wf-checkpoint 快照 |
| 3.2 15 种节点 | Phase 3-D~3-I | 扩展为 21 种（含 agent 节点） |
| 3.3 Checkpoint 集成 | Phase 3-H | 独立阶段 |

**工作量调整**: 原方案 3 周 → 本方案 5.5 周（含 wf-tools 1.5 周 + wf-executor 4 周）。

---

## 十一、风险与缓解

| 风险 | 影响 | 概率 | 缓解 |
|------|------|------|------|
| 21 种节点 handler 实现量大 | 进度延迟 | 高 | 分批次实现，优先 8 种核心节点 |
| 异步状态机竞态 | 数据竞争 | 中 | `Arc<RwLock<>>` + tokio::select! 模式 |
| Checkpoint 集成复杂 | 执行中断 | 中 | 复用 wf-checkpoint 已验证的 coordinator |
| Agent 流式执行 | 进度延迟 | 中 | Stream trait 实现；先支持同步模式 |
| 回调注册表线程安全 | 运行时 panic | 低 | OnceCell 保证单次写入；启动阶段注册 |
| MCP Rust 生态不足 | 功能缺失 | 中 | 自行实现 JSON-RPC 2.0；优先 stdio |
