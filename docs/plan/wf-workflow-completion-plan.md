# wf-workflow 补齐方案与 Rust 适配设计调整

## 1. 现状概述

Rust `wf-workflow` 核心执行循环已完成：19/20 节点处理器、DAG 遍历、Fork/Join 并行、变量解析、重试逻辑、基本检查点、事件发射。但对照 TS SDK，存在**图预处理/验证层缺失**、**周边基础设施不完整**、**嵌入/触发/钩子等功能为 stub** 三大类差距。

---

## 2. 差距分析与修改方案

### 2.1 立即补齐（P0）

| 差距 | 现状 | 方案 | 影响范围 | 预估行数 |
|------|------|------|---------|---------|
| **ContinueFromTrigger 无 handler** | `parse_node_type` 认识该类型，但 `register_defaults()` 未注册 | 新增 `ContinueFromTriggerHandler`，行为同 `TriggerPassthroughHandler`（透传） | `handler/start_end.rs` + `handler.rs` 注册 | ~15 行 |
| **EmbedHandler 为 stub** | `execute` 直接返回 `ctx.input.clone()`，无实际展开逻辑 | 需图预处理阶段展开 EMBED_GRAPH（见 2.3），在此之前保留 stub 但加 TODO | `handler/embed.rs` | 暂不修改 |
| **Hooks 为 stub** | `WorkflowHookHandler::execute_hooks` 始终返回空 Vec | 基于 `BaseHookDefinition` 的条件 + payload 模板实现实际 hook 执行 | `hook/handler.rs` | ~100 行 |
| **SubgraphHandler 空 handlers** | 子图用 `Arc::new(HashMap::new())`，遇到非默认节点会 `HandlerNotFound` | 将父级 handlers 传入子图，或允许 builder 注入子图 handler 工厂 | `handler/subgraph.rs` + `coordinator/workflow.rs` | ~30 行 |
| **ForkHandler 不执行真实子图** | `tokio::spawn` 直接返回 `BranchResult::success`，不执行分支中后续节点 | 每个分支应执行子 `WorkflowCoordinator`（类似于 SubgraphHandler） | `handler/fork_join.rs` | ~150 行 |

### 2.2 核心流程完善（P1）

| 差距 | 方案 | 影响范围 | 行数 |
|------|------|---------|------|
| **无限循环检测** | `_navigationCount > total_node_count * 5` 时返回错误 | `coordinator/workflow.rs` | ~10 行 |
| **节点超时** | 从 config 读取 `timeout`/`node_timeout`，包装 `tokio::time::timeout` | `coordinator/node.rs` | ~30 行 |
| **输出映射（outputMapping）** | handler 执行后按 `outputMapping` 规则写入 variables/workflow 输出 | `handler/output_mapping.rs`（已有）| ~50 行补齐 |
| **幂等 START/END** | 检查 `__completed_{nodeId}` 变量，已完成的直接返回 | `handler/start_end.rs` | ~25 行 |
| **错误链（ErrorChain）** | `WorkflowExecutionState` 已有 `error_records`，需补充 parent-child 链接 | `coordinator/node.rs` + `state.rs` | ~60 行 |

### 2.3 图预处理/验证层（P1，独立 crate 或合入 wf-core）

TS 的 `WorkflowGraphBuilder` + `GraphValidator` + `NodeValidator` 链在 Rust 中完全缺失。该层负责：

| 功能 | Rust 方案 | 备注 |
|------|----------|------|
| **WorkflowTemplate → WorkflowGraphStructure** | `GraphBuilder` 结构体：校验 start/end 存在、FORK/JOIN 配对、循环检测（拓扑排序） | 可基于 `petgraph` 或手写 DFS |
| **EMBED_GRAPH 展开** | 将 `EMBED_GRAPH` 节点替换为内联子图，原 START→EMBED_START、END→EMBED_END | 递归展开，检测循环引用 |
| **节点配置校验** | 每个节点类型独立的 `NodeConfigValidator` trait，返回 `ValidationError[]` | 可先做基础字段存在性检查 |
| **拓扑排序** | 验证 DAG 合法性 | 与循环检测合并 |

**选择方案**：
- `crates/wf-graph` 独立 crate，依赖 `wf-types`，被 `wf-workflow` 引用
- 初始实现聚焦于构建 + 基础校验，复杂分析（可达性、子图关系）延后

### 2.4 事件系统增强（P1）

| 缺失事件 | 当前已有 | 需补充 |
|---------|---------|--------|
| SUBGRAPH_STARTED/COMPLETED | ❌ | SubgraphHandler 中 emit |
| JOIN_CONDITION_MET/FAILED | ❌ | JoinHandler 中 emit |
| SYNC_STARTED/COMPLETED/FAILED | ❌ | SyncHandler 已有 NodeSyncStarted/Completed，补充 Failed |
| CHECKPOINT_CREATED/RESTORED/DELETED/FAILED | ❌ | checkpoint coordinator 中 emit |
| 工作流级别 FORK/JOIN 综合事件 | √ 部分 | 统一事件命名与 TS 对齐 |

### 2.5 测试补充（P1）

| 场景 | 文件 | 行数 |
|------|------|------|
| 简单线性工作流 | `tests/simple_workflow.rs` | ~80 |
| 条件路由 | `tests/route_workflow.rs` | ~80 |
| Loop 循环 | `tests/loop_workflow.rs` | ~80 |
| Fork/Join 完整执行 | `tests/fork_join_workflow.rs` | ~120 |
| 子图执行 | `tests/subgraph_workflow.rs` | ~80 |
| 中断恢复 | `tests/interruption_workflow.rs` | ~100 |
| 变量解析 | 扩充 `variable.rs` 中已有 3 个测试 | ~50 |

---

## 3. 需基于 Rust 特性调整的设计

### 3.1 图预处理：petgraph vs 自建

TS 使用手写 adjacency list + 遍历。

**Rust 方案**：推荐直接使用 `petgraph` crate。`petgraph::StableGraph` 提供内置的拓扑排序（`petgraph::algo::toposort`）、循环检测（`petgraph::algo::is_cyclic_directed`），可将当前手写的 `GraphTraversal` 封装在 petgraph 之上。

```rust
// 当前：基于 Vec<Edge> 手写查找
// 改为：petgraph 提供 O(1) neighbor 查询
use petgraph::graph::DiGraph;
type NodeIndex = petgraph::graph::NodeIndex;

struct WorkflowGraph {
    graph: DiGraph<WorkflowNode, WorkflowEdge>,
    start_node: Option<NodeIndex>,
    end_nodes: Vec<NodeIndex>,
}
```

### 3.2 Handler 分发：HashMap<dyn> → 枚举分派（可选优化）

当前用 `HashMap<StaticNodeType, Arc<dyn NodeHandler>>`，有动态分发开销且 handler 无法在编译期确保完备。

**Rust 方案**：在性能关键路径上可使用 `match` 枚举分派：

```rust
enum DispatchHandler {
    Start(StartHandler),
    End(EndHandler),
    Route(RouteHandler),
    // ...
}

impl DispatchHandler {
    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        match self {
            Self::Start(h) => h.execute(ctx).await,
            Self::End(h) => h.execute(ctx).await,
            // 编译期确保全覆盖
        }
    }
}
```

**保留 HashMap 的理由**：当前 `dyn NodeHandler` 模式足够且灵活（便于测试 mock），暂不优化。如果 bench 显示 HashMap 查询成为热点再迁移。

### 3.3 Fork 分支：JoinSet 替代 futures::join_all

TS 用 `Promise.allSettled`。当前 Rust 用 `futures::future::join_all` + `tokio::spawn`。

**Rust 方案**：使用 `tokio::task::JoinSet` 管理分支：

```rust
let mut join_set = JoinSet::new();
for branch in branches {
    join_set.spawn(run_branch(branch, ctx.clone()));
}
let mut results = Vec::new();
while let Some(result) = join_set.join_next().await {
    match result {
        Ok(Ok(branch_result)) => results.push(branch_result),
        Ok(Err(e)) => return Err(e),
        Err(join_err) => // handle spawn panic
    }
}
```

优势：可逐个处理完成的分支、支持 cancellation、不要求所有分支类型相同。

### 3.4 状态管理：DashMap 替代 TS 的 Map + Proxy

TS 的 `VariableManager` 使用 `Map` + `Proxy` 实现响应式变量访问。

**Rust 方案**：当前 `DashMap<String, Value>` + `VariableResolver::resolve()` 已足够。无需 Proxy 等价物——Rust 的安全并发模型使得显式 `get_variable(name)` / `set_variable(name, value)` 是正确且符合习惯的做法。

### 3.5 状态机：编译期状态转换 vs 运行时校验

TS 的 `WorkflowExecutionState` 使用字符串状态 + 运行时校验转换合法性。

**Rust 方案**：可利用类型系统编译期保证状态转换：

```rust
struct RunningState { current_node: String, /* ... */ }
struct PausedState { resume_at: String, /* ... */ }
struct CompletedState { output: Value, /* ... */ }

enum WorkflowExecutionState {
    Created,
    Running(RunningState),
    Paused(PausedState),
    Completed(CompletedState),
    Failed(FailedState),
    Cancelled,
}
```

当前 `WorkflowExecutionState` 使用 `status: WorkflowExecutionStatus` 枚举 + `RwLock` 内部可变，已足够且更简单。**建议保持现状**，编译期状态机对当前架构的侵入性过大。

### 3.6 注册中心：DashMap 泛型替代 TS 的类体系

TS 有 `WorkflowRegistry` / `WorkflowGraphRegistry` / `WorkflowExecutionRegistry` / `WorkflowRelationshipRegistry` 四个独立类。

**Rust 方案**：使用 `wf_core::Registry<T>` 泛型统一实现：

```rust
// wf-core 已有
pub struct Registry<T> { inner: DashMap<Id, T> }

type WorkflowRegistry = Registry<WorkflowTemplate>;
type WorkflowGraphRegistry = Registry<WorkflowGraphStructure>;
type WorkflowExecutionRegistry = Registry<WorkflowExecutionEntity>;
type WorkflowRelationshipRegistry = Registry<Relationship>;
```

不再需要为每个注册类型重复实现增删改查。

### 3.7 检查点：利用 serde + bincode 简化持久化

TS 的 checkpoint 需要手写深拷贝 (lodash `cloneDeep`)、手动计算 delta。

**Rust 方案**：序列化天然由 `serde` + `bincode` / `json` 解决，`serde_json::Value` 的 diff 计算可用 `serde_json::json_diff` 或 `similar` crate。Delta checkpoint 的实现比 TS 更自然——直接对 `serde_json::Value` 做 JSON diff。

当前 `wf-checkpoint` crate 已有完整的 delta 系统（78 tests），`wf-workflow` 的 checkpoint integration 只需对接它即可。

### 3.8 错误处理：thiserror + 链式错误

TS 的 `ErrorChainManager` 维护一个柔性错误链。

**Rust 方案**：利用 `thiserror` + `std::error::Error::source()` 链天然实现：

```rust
#[derive(thiserror::Error)]
pub enum WorkflowError {
    #[error("Node execution failed: {0}")]
    NodeExecutionFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    // source() 自动构成错误链
}
```

`ErrorChainManager` 的跨层传播能力在 Rust 中嵌套 `#[from]` + `source()` 即可覆盖大部分场景。分析功能（`analyzeErrorPattern`）需额外实现，但优先级低。

### 3.9 执行池：tokio 原生并发替代专用池

TS 有 `WorkflowExecutionPool` 管理执行器实例，限制并发度。

**Rust 方案**：直接使用 `tokio::sync::Semaphore` 限制并发执行数：

```rust
struct ExecutionPool {
    semaphore: Arc<Semaphore>,
}

impl ExecutionPool {
    async fn execute(&self, workflow: WorkflowExecutor) -> Result<Value> {
        let _permit = self.semaphore.acquire().await;
        workflow.execute_workflow(...).await
    }
}
```

不需要 TS 中动态伸缩、idle timeout 等复杂逻辑——tokio 任务本身就是轻量级。

---

## 4. 分期建议

| 阶段 | 内容 | 工作量 | 依赖 |
|------|------|--------|------|
| **Phase A** | P0 修复：ContinueFromTrigger handler + SubgraphHandler handlers 透传 + 无限循环检测 + 节点超时 | 2-3 天 | 无 |
| **Phase B** | ForkHandler 真实子图执行 + 输出映射补齐 | 2-3 天 | Phase A |
| **Phase C** | 图预处理/验证层（wf-graph 独立 crate 或合入 wf-core） | 3-5 天 | 无 |
| **Phase D** | Hooks 完整实现 + 事件补充 + 检查点事件 | 2 天 | Phase A |
| **Phase E** | 测试补充（全场景覆盖） | 3-4 天 | Phase A-D |
| **Phase F** | 错误链 + 幂等 START/END（可选优化） | 1 天 | Phase A |

**合计**：约 13-18 人天完成功能性补齐，测试额外 3-4 天。
