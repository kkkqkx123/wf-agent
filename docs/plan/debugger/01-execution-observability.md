# 执行可观测模型与 Mock 回放

本文件定义 debugger 要观察的「执行事实」从哪些现有结构来，以及 mock 交互历史的格式与回放引擎设计。

## 一、可观测点盘点（事实来源）

### 1.1 节点 / 边 / 路由

- 节点类型闭集：`StaticNodeType`（`crates/foundation/wf-types/src/node/static.rs:13`），含 `Route`/`Fork`/`Join`/`LoopStart`/`LoopEnd`/`Llm`/`AgentLoop`/`Variable` 等。
- 边：`Edge { source_node_id, target_node_id, r#type: EdgeType(Default|Conditional), condition: Option<String>, label, weight, metadata }`（`crates/foundation/wf-types/src/workflow/edge.rs:21`）。
- 路由语义：`RouteHandler` 遍历 `conditions`（`expression` → `target_node_id`），**首个表达式为真即 `break`**，否则落到 `default_target_node_id`（`crates/engine/wf-workflow/src/handler/route.rs:43-66`）。这就是「边路由」的可解释核心，也是分支穷尽的枚举对象。
- 图遍历 API：`GraphTraversal` 提供 `get_outgoing_edges`/`get_incoming_edges`/`find_ready_nodes`/`compute_reachable`（`crates/engine/wf-workflow/src/graph.rs:91/102/121/62`）。
- 静态分析：`analyze_graph`/`analyze_reachability`/`detect_cycles`/`topological_sort`（`crates/engine/wf-workflow/src/lib.rs:32-36`）。

### 1.2 每步输出（step output）

`NodeExecutionRecord { node_id, node_name, node_type, start_time, end_time, success, error, input: Option<Value>, result: Option<Value>, branch_id: Option<String> }`（`crates/engine/wf-workflow/src/state.rs:9-29`）是「每步输出」的天然载体。debugger 在每步前后记录该结构即可展示输入/输出与所属分支。

### 1.3 变量 diff

- 变量存储：`VariableStore = Arc<DashMap<String, Value>>`（`crates/engine/wf-workflow/src/variable.rs:5`）。
- 求值/插值：`evaluate_expression` 与 `VariableResolver`（`crates/engine/wf-workflow/src/variable.rs:631/544`）。
- 变量 diff 做法：对每一步前后的 `VariableStore` 各做一次快照（clone 为 `HashMap<String,Value>`），按 key 集合做增/删/改对比，值用工作区已有的 `similar = "3"`（`Cargo.toml`）做 JSON 文本 diff。

### 1.4 消息 diff

- 消息上下文存于变量表 `__msg_ctx__` 前缀下：`get_context`/`append_context`/`register_context`/`get_context_history`（`crates/engine/wf-workflow/src/message_context.rs:74/100/126/158`）。
- 消息结构：`wf_types::message::Message { id, role, content, ... }`（`crates/engine/wf-workflow/src/message_context.rs:247` 等测试用法）。
- 消息 diff 做法：取每一步前后某 `context_id` 的 `get_context`（活跃视图）或 `get_context_history`（全量），按 `message.id` 对齐，标注新增 / 改写 / 删除；agent 循环同理对比 `AgentLoopEntity::conversation`（`crates/engine/wf-agent/src/entity.rs:24`）。

### 1.5 交互历史（interaction）

`InteractionRegistry`（`crates/engine/wf-workflow/src/interaction.rs:20`）以 interaction id 注册/完成异步人机交互。mock 交互历史应保留交互请求与响应，便于 replay 时无需真实应答。

### 1.6 出错时的「实际值」

统一在 trace 中记录每步的 `error` 与 `result`；断言失败或步骤报错时，debugger 高亮打印**实际值**（节点的实际 `result`/`error`、变量的实际值、hook 的实际 `veto reason`、trigger 的实际落选原因）。

## 二、Mock 交互历史格式（草稿）

一份交互历史 = 有序步骤 + 全局图/agent 定义引用 + 可选断言。精简结构示意（非完整实现）：

```
Trace {
  kind: Workflow | Agent,
  graph_ref: <graph 定义或内联节点/边>,
  initial_variables: Map<String, Value>,
  steps: [
    Step {
      index, node_id, node_type,
      input: Value,
      result: Value,
      success: bool,
      error: Option<String>,
      branch_id: Option<String>,
      variable_snapshot_before: Map<String,Value>,
      variable_snapshot_after:  Map<String,Value>,
      message_context_before: Map<context_id, [Message]>,
      message_context_after:  Map<context_id, [Message]>,
      hooks_fired: [ HookFire ],
      triggers_seen: [ TriggerEvent ],
    }, ...
  ],
  assertions: [ Assertion ],   // 可选
}
```

- `variable_snapshot_*` / `message_context_*` 可由真实运行的 checkpoint 或单元测试导出，再由 debugger 计算 diff；
- `hooks_fired` / `triggers_seen` 让 debugger 在不重跑引擎的前提下，直接对已有记录做 hook/trigger 解释（见 `03-hook-trigger-debug.md`）。

## 三、回放引擎（replay.rs）

两种回放路径共用同一套「观察 → 格式化 → 断言」管线：

### 3.1 导入式回放（replay-from-trace）

- 直接读取 `Trace`，逐 `Step` 调用 `observe` 计算变量/消息 diff，调用 `format` 输出，调用 `assert` 校验；
- 适用于「已发生的一次执行」的事后分析。

### 3.2 静态综合回放（replay-from-graph，分支穷尽模式）

- 仅输入 graph + 初始变量，**不实际调用节点 handler**；
- 用 `GraphTraversal` 做可达性/环检测（`graph.rs` + `analysis.rs`）；
- 对每个 `Route`/`Fork`/`Loop` 决策点，枚举其所有出边条件 + 默认分支，逐条用 `ConditionEvaluator` 对当前变量快照求值，标注：
  - 哪些分支在当前快照下**命中**；
  - 默认分支是否**可达**（无显式条件命中时）；
  - 是否存在**永远不可达的死边**（健壮性检查）；
- 产物是一棵「分支决策树」，供用户核对路由逻辑是否如预期。详见 `02-workflow-agent-step-debug.md` 的「分支穷尽」一节。

### 3.3 实时式回放（可选增强）

- 通过 mock `HookHandlerRegistry` / mock `TriggerActionRunner` 真正驱动 `fire()` 与 `matcher/arbiter/governor`，但用 no-op / 录制型实现替换真实副作用；
- 适合「给定一份输入变量，看真实路由/条件函数会选出哪条边」的在线推演。

## 四、与现有结构的映射小结

| 调试对象 | 现有来源 | debugger 用法 |
|----------|----------|---------------|
| 每步输出 | `NodeExecutionRecord`（`state.rs:9`） | 直接展示 input/result/branch_id |
| 变量 diff | `VariableStore`（`variable.rs:5`） | 前后快照对比 + `similar` diff |
| 消息 diff | `__msg_ctx__` 上下文（`message_context.rs:18`） | 按 id 对齐的增删改 |
| 路由决策 | `RouteHandler`（`route.rs:43`） | 复用语义 / 枚举分支 |
| 出错实际值 | `error`/`result` 字段 | 高亮实际值 |

下一份文档 `02-workflow-agent-step-debug.md` 展开「逐步输出 / 消息 diff / 变量 diff / 分支穷尽」的具体展示与交互方式。
