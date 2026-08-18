# wf-agent 工作流引擎分析报告

> 分析范围：`crates/wf-workflow`（Rust 版）
> 说明：TS 版为初版初步设计，不纳入本次分析

---

## 一、整体架构

```
WorkflowExecutor
  → WorkflowLifecycleCoordinator
    → WorkflowCoordinator::execute_inner()  (主循环)
      → NodeCoordinator::execute_node()     (Hook/事件/执行)
        → 具体 NodeHandler::execute()        (按 StaticNodeType 分发)
```

核心执行流程：
1. `WorkflowExecutor` 接收工作流定义，创建 `WorkflowCoordinator`
2. `WorkflowLifecycleCoordinator` 管理工作流生命周期状态
3. `WorkflowCoordinator::execute_inner()` 是主循环，驱动节点导航和执行
4. `NodeCoordinator::execute_node()` 处理节点级别的 Hook、Checkpoint、事件
5. 具体 `NodeHandler` 按节点类型执行实际业务逻辑

---

## 二、节点类型及处理逻辑

### 2.1 控制流节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **START** | 通过 `completed_marker` 变量做幂等守卫，透传输入 | `start_end.rs` |
| **END** | 同上，纯状态标记节点 | `start_end.rs` |
| **ROUTE** | 遍历 `conditions` 数组 → `ConditionEvaluator::evaluate` 逐条评估 → 首条命中则跳转，否则走 `default_target_node_id` | `route.rs` |

### 2.2 循环节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **LOOP_START** | 通过 `__loop_{id}_counter` 变量计数，超过 `max_iterations` 终止 | `loop_handler.rs` |
| **LOOP_END** | 评估 `break_condition` → 若需继续则返回 `loop_start_node_id` 跳转 | `loop_handler.rs` |

### 2.3 并行与同步节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **FORK** | BFS 提取分支子图 → `JoinSet` 并行/串行执行 → `FailureStrategy` 评估 → 聚合输出 | `fork_join.rs` |
| **JOIN** | 从 Fork 输出中 `collect_branch_outputs` → 按 `wait_for_all/wait_for_any/wait_for_n` 策略 `merge_outputs` | `fork_join.rs` |
| **SYNC** | 解析 `variable_exchanges` → 跨分支变量同步 → 可选 `wait_for_completion` | `sync.rs` |

### 2.4 子图/内嵌图节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **SUBGRAPH** | 从 `registry::lookup_graph` 查找已注册子图 → 创建子 `WorkflowCoordinator` 执行 → `apply_variable_outputs` 导出变量 | `subgraph.rs` |
| **EMBED_GRAPH** | 从内联 `graph_definition` 解析子图 → 创建子 `WorkflowCoordinator` 执行 → 与 SUBGRAPH 几乎相同 | `embed.rs` |

> 注：SUBGRAPH 与 EMBED_GRAPH 的设计区分是本节重点分析对象，详见[第四节](#四-subgraph-与-embed_graph-设计区分分析)。

### 2.5 AI 节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **LLM** | 完整 LLM 调用：`build_messages` → `resolve_tools` → 多轮 tool loop → 双轨 token 追踪 → 流式/非流式 → 上下文压缩信号 | `llm.rs` |
| **AGENT_LOOP** | `AgentLoopCoordinator` 驱动：工具可见性/渐进式披露/技能注入/Checkpoint → 流式事件透传 | `agent_loop.rs` |

### 2.6 数据处理节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **VARIABLE** | 解析 `VariableResolver::resolve` 求值表达式 → 通过 `ctx.set_variable` 写入，禁止修改 `__` 前缀的内部变量 | `variable.rs` |
| **CONTEXT_PROCESSOR** | 三种操作：`Aggregate`(聚合)、`Transform`(转换)、`BatchUpdate`(批量更新) | `context_processor.rs` |
| **SCRIPT** | `SandboxRuntime::execute_named` 沙箱执行 → 输出映射 + JSON 解析回退 | `script.rs` |
| **INTERACTIVE_SCRIPT** | 在 SCRIPT 基础上注入 `__interaction_input__` 变量 | `interactive_script.rs` |

### 2.7 交互与工具节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **USER_INTERACTION** | 事件驱动：`register_interaction` 创建 oneshot channel → 发布 `FollowupQuestionRequested` 事件 → `tokio::time::timeout` 等待 → 执行 UPDATE_VARIABLES/ADD_MESSAGE | `user_interaction.rs` |
| **TOOL_VISIBILITY** | 写 `__tool_blocked_*`/`__tool_activated_*` 标记变量 → 消息上下文追加 System 通知 | `tool_visibility.rs` |

### 2.8 触发器节点

| 节点类型 | 处理逻辑 | 文件 |
|---------|---------|------|
| **START_FROM_MESSAGE** | 幂等守卫 → `map_message_inputs` 映射输入 → 可选 TriggerAction 执行 | `trigger.rs` |
| **CONTINUE_FROM_MESSAGE** | 幂等守卫 → `export_variable_outputs` + `export_message_outputs` → 可选 TriggerAction | `trigger.rs` |

---

## 三、NodeHandler 分发机制

### 3.1 注册与查找

NodeHandler 通过 `get_node_handler()` 函数按 `StaticNodeType` 枚举分发的：

```rust
pub fn get_node_handler(node_type: StaticNodeType) -> Option<Box<dyn NodeHandler>> {
    match node_type {
        StaticNodeType::Start => Some(Box::new(StartHandler)),
        StaticNodeType::End => Some(Box::new(EndHandler)),
        StaticNodeType::Llm => Some(Box::new(LlmHandler)),
        StaticNodeType::AgentLoop => Some(Box::new(AgentLoopHandler)),
        StaticNodeType::Route => Some(Box::new(RouteHandler)),
        StaticNodeType::Fork => Some(Box::new(ForkHandler)),
        StaticNodeType::Join => Some(Box::new(JoinHandler)),
        StaticNodeType::Sync => Some(Box::new(SyncHandler)),
        StaticNodeType::LoopStart => Some(Box::new(LoopStartHandler)),
        StaticNodeType::LoopEnd => Some(Box::new(LoopEndHandler)),
        StaticNodeType::Subgraph => Some(Box::new(SubgraphHandler)),
        StaticNodeType::EmbedGraph => Some(Box::new(EmbedHandler)),
        StaticNodeType::Script => Some(Box::new(ScriptHandler)),
        StaticNodeType::InteractiveScript => Some(Box::new(InteractiveScriptHandler)),
        StaticNodeType::Variable => Some(Box::new(VariableHandler)),
        StaticNodeType::ContextProcessor => Some(Box::new(ContextProcessorHandler)),
        StaticNodeType::UserInteraction => Some(Box::new(UserInteractionHandler)),
        StaticNodeType::ToolVisibility => Some(Box::new(ToolVisibilityHandler)),
        StaticNodeType::StartFromMessage => Some(Box::new(StartFromMessageHandler)),
        StaticNodeType::ContinueFromMessage => Some(Box::new(ContinueFromMessageHandler)),
        _ => None,
    }
}
```

**注意**：`EMBED_START` 和 `EMBED_END` 在 `StaticNodeType` 枚举中不存在，对应的 `RuntimeEmbedStartNode`/`RuntimeEmbedEndNode` 类型定义在 `crates/wf-types/src/node_execution/runtime.rs` 中，但没有对应的 handler 实现，也未在分发映射中注册。这意味着它们当前**完全不参与运行时执行**。

---

## 四、SUBGRAPH 与 EMBED_GRAPH 设计区分分析

### 4.1 TS 版的设计意图

TS 版（初版设计）对 SUBGRAPH 和 EMBED_GRAPH 有明确的职责区分：

#### SUBGRAPH（方案 C：独立子执行实体）

- **预处理阶段**：不展开。`processSubgraphs()` 中遇到 SUBGRAPH 节点直接跳过，保留为独立节点。
- **运行时阶段**：通过 `WorkflowExecutionBuilder.createChildExecution()` 创建**独立的子执行实体**，拥有独立的 `VariableManager`、`ExecutionState` 等。
- **变量映射**：支持显式的 `variableInputs`/`variableOutputs` 声明式映射，通过 `importVariables`/`exportVariables` 跨边界传递。
- **消息上下文**：支持 `messageInputs`/`messageOutputs` 双向传递，通过 `enterSubgraph`/`exitSubgraph` 管理消息上下文栈。
- **失败策略**：支持 `fail`/`retry`/`continue` 三种策略，含指数退避重试。
- **数据隔离**：变量通过 `deep-clone` 方式隔离，父子实体完全独立。

#### EMBED_GRAPH（方案 C：内联图展开）

- **预处理阶段**：**完全展开**。`processSubgraphs()` 中通过 `mergeGraph()` 将内联图展开到主图中，节点 ID 添加命名空间前缀防止冲突。
- **展开转换**：`START` → `EMBED_START`，`END` → `EMBED_END`（边界标记节点）。
- **严格约束**（由 `validateEmbedGraphConstraints()` 强制检查）：
  - 规则 1：不能定义变量（`variables` 数组必须为空）
  - 规则 2：不能有触发器（`triggers` 数组必须为空）
  - 规则 3：不能包含 `VARIABLE` 类型的节点
  - 规则 4：不能有 `variableInputs`/`variableOutputs` 变量映射
- **运行时**：EMBED_START/EMBED_END 是纯穿透节点，**不执行任何逻辑**、不隔离数据、不创建新作用域。
- **核心意图**：**纯控制流复用**——只复用节点编排逻辑，不涉及数据隔离和变量传递。

### 4.2 Rust 版当前实现

#### SUBGRAPH 实现（`subgraph.rs`）

```rust
// 1. 从 registry 中通过 subgraph_id 查找已注册子图
let subgraph = registry::lookup_graph(id)?;

// 2. 创建独立的 ExecutorContext + WorkflowCoordinator
let exec_ctx = ExecutorContext::new(execution_id, sub_workflow_id, ...);
let mut coordinator = WorkflowCoordinator::new(exec_ctx, subgraph, handlers)?;

// 3. 变量映射：apply_variable_inputs / apply_variable_outputs
apply_variable_inputs(config, &parent_vars, &child_vars)?;
let output = coordinator.execute().await?;
apply_variable_outputs(config, &child_vars, &parent_vars);

// 4. 创建新的 execution_id，独立于父实体
```

#### EMBED_GRAPH 实现（`embed.rs`）

```rust
// 1. 从内联 graph_definition 中解析子图
let subgraph: WorkflowGraphStructure = serde_json::from_value(graph_value)?;

// 2. 创建独立的 ExecutorContext + WorkflowCoordinator
let exec_ctx = ExecutorContext::new(execution_id, sub_workflow_id, ...);
let mut coordinator = WorkflowCoordinator::new(exec_ctx, subgraph, handlers)?;

// 3. 变量映射：apply_variable_inputs / apply_variable_outputs
apply_variable_inputs(config, &parent_vars, &child_vars)?;
let output = coordinator.execute().await?;
apply_variable_outputs(config, &child_vars, &parent_vars);
```

#### 预处理模块（`preprocess.rs`）

```rust
// 提供 flatten_graph() 方法，对 EMBED_GRAPH 节点做内联展开
// 节点添加命名空间前缀：<embed_node_id>:<original_node_id>
// 边重连：入边指向子图入口，出边从子图出口引出
// 但展开结果仅用于 `PreprocessedGraph.flattened` 分析视图
// 运行时执行仍然走 EmbedHandler，不执行展开后的图
```

### 4.3 两个实现的差异对比

| 维度 | TS 版设计意图 | Rust 版当前实现 | 问题 |
|------|--------------|----------------|------|
| **SUBGRAPH 预处理** | 不展开，保留独立节点 | 不展开，保留独立节点 | 一致 |
| **EMBED_GRAPH 预处理** | `mergeGraph()` 展开到主图，节点重命名 | `flatten_graph()` 展开到 `flattened` 视图，但仅用于分析 | **不一致** |
| **EMBED_GRAPH 运行时** | 不需要运行时 handler（节点已展开） | 通过 `EmbedHandler` 创建子 `WorkflowCoordinator` 执行 | **严重不一致** |
| **EMBED_GRAPH 变量映射** | 禁止 `variableInputs`/`variableOutputs` | 通过 `input_mapping`/`output_mapping` 支持变量映射 | **违反设计约束** |
| **EMBED_GRAPH execution_id** | 共享父 execution_id（展开后无独立实体） | 使用父 execution_id"克隆" | **语义模糊** |
| **EMBED_START/EMBED_END** | 运行时穿透节点 | 定义存在但无 handler 实现 | **未使用** |
| **EMBED_GRAPH 约束检查** | 预处理时严格检查 3 条规则 | 无约束检查 | **缺失** |
| **数据隔离** | 纯控制流复用，不涉及数据隔离 | 通过 `apply_variable_inputs` 创建了数据耦合 | **违背设计意图** |

### 4.4 核心问题

**问题 1：运行时与预处理脱节**

Rust 版的 `preprocess.rs` 已实现 `flatten_graph()` 方法，能够正确地将 EMBED_GRAPH 内联展开（命名空间化节点、重连边），但这一展开结果仅存储在 `PreprocessedGraph.flattened` 中用于**分析目的**，从不用于**运行时执行**。运行时执行器仍然使用 `EmbedHandler` 以子工作流方式执行。

这意味着：
- 分析视图看到的是展开后的图（无环、可达性正确）
- 运行时视图看到的是展开前的图（含 EMBED_GRAPH 节点）
- 两套视图不一致，可能导致分析结果与运行行为不匹配

**问题 2：EMBED_GRAPH 运行时实现与 SUBGRAPH 几乎相同**

对比 `embed.rs` 和 `subgraph.rs`，两者核心逻辑一致：
- 都创建独立的 `WorkflowCoordinator`
- 都使用 `apply_variable_inputs`/`apply_variable_outputs` 做变量映射
- 都使用 `resolve_parent_handlers` 向下转型
- 都发射 `SubgraphStarted`/`SubgraphCompleted` 事件

唯一区别：
- 子图来源：SUBGRAPH 从 registry 查找，EMBED_GRAPH 从内联配置解析
- `execution_id`：SUBGRAPH 新建，EMBED_GRAPH 复用父 ID
- 指标：SUBGRAPH 有 subgraph_metrics 跟踪，EMBED_GRAPH 没有

**问题 3：EMBED_GRAPH 的变量映射违反设计约束**

TS 版的 `validateEmbedGraphConstraints()` 明确禁止 EMBED_GRAPH 有任何 `variableInputs`/`variableOutputs`，因为 EMBED_GRAPH 的设计意图是纯控制流展开，不应该涉及数据传递。但 Rust 版的 `EmbedGraphNodeConfig` 包含 `input_mapping`/`output_mapping` 字段，且运行时通过 `apply_variable_inputs`/`apply_variable_outputs` 执行变量映射，这：

- 违背了 EMBED_GRAPH 的"纯控制流复用"设计意图
- 创建了运行时的数据耦合，与 TS 版意图完全相反
- 实际上让 EMBED_GRAPH 变成了"另一种 SUBGRAPH"

**问题 4：EMBED_START/EMBED_END 存在但不可用**

`RuntimeEmbedStartNode` 和 `RuntimeEmbedEndNode` 类型定义在 `crates/wf-types/src/node_execution/runtime.rs` 中，但：
- `StaticNodeType` 枚举中不包含 `EmbedStart`/`EmbedEnd`
- `get_node_handler()` 中没有对应的 handler
- 当前没有任何路径能创建或执行这些节点

### 4.5 设计对比总结

```
TS 版设计意图：
  SUBGRAPH    = 独立子执行实体 + 变量映射 + 消息上下文 + 数据隔离
  EMBED_GRAPH = 预处理阶段内联展开 + 纯控制流复用 + 禁止变量映射

Rust 版当前实现：
  SUBGRAPH    ≈ 独立子执行实体 + 变量映射（与 TS 版一致）
  EMBED_GRAPH ≈ SUBGRAPH 的内联配置版本（与 TS 版完全不同）
```

**建议的修正方向**：

1. **方案 A（对齐 TS 版设计）**：
   - 运行时使用 `flatten_graph()` 展开后的图执行，移除 `EmbedHandler`
   - 移除 `EmbedGraphNodeConfig` 中的 `input_mapping`/`output_mapping`
   - 添加 `EMBED_START`/`EMBED_END` 的 handler 实现（纯穿透节点）
   - 添加 EMBED_GRAPH 约束校验（无变量、无触发器、无 VARIABLE 节点）
   - 移除 `EMBED_START`/`EMBED_END` 运行时类型或实现它们

2. **方案 B（保留当前模式，但消除不一致）**：
   - 明确将 EMBED_GRAPH 定义为"轻量子图执行"（inline subgraph execution）
   - 与 SUBGRAPH 共享公共执行逻辑（提取 `SubgraphExecutor` 消除代码重复）
   - 移除 `flatten_graph()` 中未使用的展开逻辑
   - 统一 `execution_id` 策略
   - 仍然添加约束检查（如不允许嵌套 SUBGRAPH 等）

3. **方案 C（同时支持两种模式，配置驱动）**：
   - 通过配置字段（如 `expand_at_preprocess: bool`）决定 EMBED_GRAPH 的执行方式
   - `expand_at_preprocess = true`：走方案 A 的展开路径
   - `expand_at_preprocess = false`：走方案 B 的子图执行路径
   - 需要保证两种模式下的语义一致性

---

## 五、现有问题与设计缺陷

### 5.1 核心协调器职责过重（严重）

**`WorkflowCoordinator::execute_inner()`** 约 **465 行**，在一个方法中处理了以下全部职责：

- 中断检查（Stop/Pause）
- 最大执行时间检查
- 最大步数限制
- 导航计数循环检测
- 已完成的节点跳过
- 带超时的节点执行
- 错误处理（retry/continue/fallback 三种策略）
- 重试循环（以及重试中的节点上下文重建）
- 指标记录
- 错误记录链构建
- Checkpoint 集成
- 触发器效果处理

**问题**：圈复杂度极高，难以测试和维护，横切关注点混杂。

### 5.2 循环作用域隔离缺陷

```rust
let counter_var = format!("__loop_{}_counter", loop_id);
```

- 循环状态存储在全局变量空间中，**无作用域隔离**
- 嵌套循环场景下，内层循环 ID 可能与外层冲突
- `LOOP_START` 不具有 `break_condition` 检查，只有 `LOOP_END` 有，设计不对称
- `max_iterations` 默认值 100，无保护机制防止用户设置过大值

### 5.3 循环检测的脆弱性

```rust
self.navigation_count += 1;
let max_allowed = self.total_node_count * self.max_navigation_multiplier;
```

- `navigation_count > total_node_count * 5` 是**启发式近似**，非精确检测
- 对合法的大工作流或深层嵌套循环不友好
- `max_navigation_multiplier = 5` 硬编码默认值
- 不区分正常控制流和异常循环

### 5.4 FORK 实现中的竞态条件与设计缺陷

- **`SyncBarrier` 在 ForkHandler 中创建但 JoinHandler 不使用**：JoinHandler 从 `ctx.input` 读取分支结果，而非通过 Barrier 同步。Barrier 实际仅用于 `SYNC` 节点的 `wait_for_completion`。
- **`extract_branch_subgraph` 将 `fork_node_id` 加入每个分支子图**，子图包含父节点是设计污染。
- **`find_join_node` 依赖 `fork_path_ids` 字符串匹配**，不同 Fork 的路径 ID 可能冲突。
- **并行分支的 `RetryBudget` 共享在同一 `Arc<RetryBudget>` 上**，分支间并行消费存在竞态条件。

### 5.5 Handler 向下转型的脆弱模式

`fork_join.rs`、`subgraph.rs`、`embed.rs`、`trigger.rs` 中重复出现：

```rust
fn resolve_handlers(ctx: &NodeExecutionContext) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    match &ctx.handler_registry {
        Some(any) => match any.clone().downcast::<HashMap<StaticNodeType, Box<dyn NodeHandler>>>() {
            Ok(handlers) => handlers,
            Err(_) => Arc::new(HashMap::new()), // 静默降级！
        },
        None => Arc::new(HashMap::new()),
    }
}
```

- 转型失败时**静默返回空 HashMap**，下游出现 `HandlerNotFound` 难以排查
- 类型别名变化时所有转型静默失败
- 相同模式在 4 个文件中重复，违反 DRY 原则

### 5.6 SUBGRAPH 和 EMBED_GRAPH 代码重复（严重）

- `SubgraphHandler` 和 `EmbedHandler` 执行逻辑几乎完全一致，仅子图来源不同（注册表 vs 内联）
- 约 150 行代码重复，包括 `ExecutorContext` 构建、`WorkflowCoordinator` 创建、事件发射、`variable_mapping` 等
- 应抽象共享的 `SubgraphExecutor` 或 `execute_subgraph` 函数

### 5.7 LLM 节点的 `dead_loop_detection: None`

```rust
LlmRequest {
    // ...
    dead_loop_detection: None,  // TODO
}
```

- 死循环检测未实现，硬编码为 `None`
- `max_tool_calls_per_request` 默认值 5，LLM 持续输出 tool calls 时截断而非报错

### 5.8 变量系统的类型安全缺失

- 变量使用 `Arc<DashMap<String, Value>>`，无 schema 约束
- 内部变量（`__` 前缀）保护仅在 `VariableHandler` 的 `is_readonly_var` 中检查，其他 handler 可直接通过 `ctx.set_variable` 写入 `__` 变量
- 并发场景下无 ACID 保证（Fork 分支同时写入同一变量时，最后写入者胜出）

### 5.9 `compute_node_input` 的不一致性

```rust
fn compute_node_input(&self, node_id: &str) -> Value {
    if inputs.len() == 1 {
        inputs.values().next().cloned().unwrap_or(Value::Null)
    } else {
        Value::Object(inputs)
    }
}
```

- 单条入边时返回裸值，多条入边时包装为 `Object`
- 节点无法区分"收到了一个对象"和"收到了多个输入合并的对象"
- 同样模式出现在 `compute_final_output` 中

### 5.10 事件发布的无错误处理

- 几乎所有 `emit_*` 函数使用 `let _ = bus.publish(event)` 静默忽略错误
- 多个 handler 使用 `if let Some(bus) = &event_bus { ... }`，无 EventBus 时完全静默
- 事件发布失败不影响工作流执行，但可能导致外部系统状态不一致

### 5.11 USER_INTERACTION 的资源泄漏风险

```rust
let (interaction_id, rx) = register_interaction();
let wait_result = tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await;
```

- `register_interaction()` 创建 oneshot channel，超时后 `rx` 丢弃但 `tx` 端可能仍被引用
- `InteractionRegistry` 中已注册的 `interaction_id` 超时后不会被清理

### 5.12 错误记录链的复杂性

```rust
record.parent_error_id = Some(prev.id.clone());
let mut chain = prev.error_chain.clone();
chain.push(record.id.clone());
record.error_chain = chain;
record.root_cause_id = prev.root_cause_id.clone();
```

- 手动维护错误链，逻辑复杂易出错
- `parent_error_id` 设置的是"上一条记录"而非"真正的父错误"
- 重试场景中，重试失败链接到初次失败记录，但初次失败记录可能已被标记为"已处理"

### 5.13 配置解析的松散性

- `NodeRetryConfig::resolve` 中 `serde_json::from_value::<RetryPolicy>` 使用 `ok()` 忽略解析错误
- `node_checkpoint_config` 同样使用 `ok()` 静默降级
- 配置错误时静默使用默认值，用户可能以为配置生效但实际未生效

### 5.14 同步点（SYNC）的实现空洞

- `SyncHandler` 使用 `SyncBarrier`，`wait_for_completion` 等待所有分支到达 Barrier
- 如果某个分支永远不到达 Barrier（如因错误提前退出），`wait_for_all` 会永久阻塞
- 无超时机制保护

### 5.15 测试覆盖缺口

- Loop（LOOP_START/LOOP_END）集成测试缺失
- SUBGRAPH/EMBED_GRAPH 端到端测试缺失
- SYNC 节点跨分支同步测试缺失
- 并发场景下的竞态条件测试缺失
- Checkpoint 恢复的集成测试缺失

---

## 六、总结

### 优先级矩阵

| 优先级 | 问题 | 影响 |
|--------|------|------|
| **P0** | EMBED_GRAPH 运行时与预处理脱节，实现与 SUBGRAPH 重复 | 设计意图完全偏离，EMBED_GRAPH 语义错误 |
| **P0** | `execute_inner` 方法过重（~465 行） | 可维护性、可测试性差 |
| **P0** | 循环作用域隔离缺失 | 嵌套循环场景错误 |
| **P1** | EMBED_GRAPH 变量映射违反设计约束 | 数据污染风险 |
| **P1** | EMBED_START/EMBED_END 定义存在但不可用 | 无用代码 |
| **P1** | Handler 向下转型静默降级 | 运行时错误难以排查 |
| **P1** | SUBGRAPH/EMBED_GRAPH 代码重复 | 维护成本高，易不一致 |
| **P1** | 输入/输出聚合逻辑不一致 | 节点行为难以预测 |
| **P1** | 变量系统无类型安全 | 并发访问无保障 |
| **P2** | FORK 的 Barrier 与 JOIN 解耦 | 设计冗余 |
| **P2** | 事件发布错误处理缺失 | 外部系统状态不一致 |
| **P2** | USER_INTERACTION 资源泄漏 | 累积资源占用 |
| **P2** | 配置解析静默降级 | 用户配置无效而不知 |
| **P3** | SYNC 无超时保护 | 分支异常时永久阻塞 |
| **P3** | 测试覆盖缺口 | 回归风险高 |

### 核心建议

**SUBGRAPH 与 EMBED_GRAPH 的设计区分是正确且有价值的**，但 Rust 版当前实现存在以下关键问题：

1. **EMBED_GRAPH 运行时实现需要从根本上修正**——要么走内联展开路径（方案 A），要么明确为"轻量子图执行"（方案 B），但不能维持当前"半展开"状态
2. **无论选择哪种方案，SUBGRAPH 和 EMBED_GRAPH 的共享代码都应该提取到公共的执行器函数中**，消除约 150 行的重复代码
3. **预处理与运行时的一致性需要保证**——如果 `flatten_graph()` 存在，运行时就应该使用展开后的图执行，而不是仅用于分析
4. **EMBED_GRAPH 的约束检查需要实现**（无变量、无触发器、无 VARIABLE 节点），这是保证语义正确性的前提