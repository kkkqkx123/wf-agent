# wf-workflow 图校验与错误分支

> 范围：`src/validation.rs` 及 `src/validation/` 子树、`src/node_validation.rs`、`src/protocol_consistency.rs`、`src/reference_closure.rs`、`src/analysis.rs`、`src/graph.rs`、`src/error_branch.rs`、`src/error_analysis.rs`。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

## 1. 图校验

### 1.1 编排与类型保证（`validation.rs`）

- 规则模块分工（模块文档 `validation.rs:1-14`）：`basic`（节点/边结构合法性、端点引用、边界数量）、`node_types`（边界拓扑、孤立节点、SYNC/EMBED/SUBGRAPH 约束）、`fork_join`（配对与分支目标）、`loop_pairs`（循环配对）、`route`（路由目标与表达式）、`error_routes`（ERROR 边声明与环）、`connectivity`（环与可达性）。
- `ValidatedGraph`（`validation.rs:46`）是标记类型：只能经 `GraphValidator::validate` / `validate_with_profiles` 构造，使"结构已校验"成为**类型层面的承诺**（访问器 `inner()`/`into_inner()`，`validation.rs:50-55`）。
- `validate_with_profiles`（`validation.rs:88-118`）按序运行完整规则管线：节点/边基础 → START/END 拓扑 → 引用 → 节点类型拓扑/孤立 → fork/join 配对 → loop 配对 → sync/embed/subgraph → 触发子图 → route → 错误分支 → fork 子节点 → 环 → 可达性 → 节点配置 → 协议一致性；全通过才返回 `ValidatedGraph`。
- 正式注册变体 `validate_with_reference_context`（`validation.rs:123-155`）：叠加引用闭包校验；告警随结果返回，不阻断注册。
- `format_validation_report`（`validation.rs:64-70`）：逐条 `- [field] message` 报告，供 CLI/TUI 注册输出。

### 1.2 规则模块（`validation/`）

| 模块 | 检查 | 位置 |
|------|------|------|
| `basic` | 节点 id 唯一、边结构、START/END 数量、边端点引用 | `basic.rs:11` / `:44` / `:63` / `:144` |
| `node_types` | START 无入边、END 无出边、孤立节点（ERROR 边算连通）、SYNC/EMBED/SUBGRAPH 约束 | `node_types.rs:14` / `:60` / `:87` / `:194` / `:256` |
| `fork_join` | FORK/JOIN 配对、路径 id、JOIN 从 FORK 可达、分支 `child_node_id` 解析 | `fork_join.rs:17` / `:211` |
| `loop_pairs` | 每个 loopId 恰一对 LOOP_START/LOOP_END、交叉引用、边界边所在侧 | `loop_pairs.rs:14` |
| `route` | 路由表达式语法、至少一个条件或默认目标、默认目标不重复 | `route.rs:15` |
| `error_routes` | ERROR 边声明合法性 + 错误路由环检测（跨 fork 路由仅告警） | `error_routes.rs:25` |
| `connectivity` | 触发子图、环检测、可达性（每节点从 START 可达且可达某 END） | `connectivity.rs:12` / `:67` / `:85` |

### 1.3 节点配置与协议一致性

- `node_validation.rs:9-21`：字段级业务规则的薄适配层——单一事实源在共享的 `wf-config` 节点校验器（wf-api 保存路径复用），此处桥接为 `ValidationError`。
- `protocol_consistency.rs:57-110`：所有声明 `tool_call_protocol` 的 LLM/AGENT_LOOP 节点必须使用被识别的协议值，且**全图声明的格式必须一致**；profile 感知变体追加 `profile_id` 存在性与节点格式 ↔ 档案格式兼容检查（`protocol_consistency.rs:104-139`）。

### 1.4 引用闭包（`reference_closure.rs`）

- `MAX_REFERENCE_DEPTH = 16`（`reference_closure.rs:10`）限制子图递归深度。
- `ReferenceContext`（`reference_closure.rs:16-26`）：注册中心快照集合（profile、工具与禁用工具、脚本、工作流/图、触发器）。
- `ReferenceClosureReport`（`reference_closure.rs:93-107`）：错误/告警二分——**硬性引用缺失阻断正式注册；弱风险允许带报告注册**（`reference_closure.rs:89-91`）。
- 校验覆盖：profile 存在性与格式、工具名（含内联 agent 工具列表）、脚本引用、子图/嵌入存在性、触发器引用、agent-loop 形状，以及深度受限的子图递归检查 `validate_subgraph_recursion`（`reference_closure.rs:620`）。

### 1.5 图静态分析（`analysis.rs`）

- `is_structural_edge`（`analysis.rs:13-15`）：LOOP_END→LOOP_START 续环边与 ERROR 边是合法控制流，从环检测/拓扑排序中排除。
- `detect_cycles`（`analysis.rs:50-121`，迭代 DFS）、`topological_sort`（`analysis.rs:133-183`，Kahn 算法，失败时报告环上节点）、`analyze_reachability`（`analysis.rs:230-264`，计算不可达与死端节点集）。
- `analyze_graph`（`analysis.rs:277-293`）：环 + 拓扑 + 可达性 + 节点/边总量与类型分布的组合分析，供校验与执行前检查复用。

## 2. 图遍历与错误路由表（`graph.rs`）

- `GraphTraversal`（`graph.rs:8-13`）：持图结构 + 构造期一次性编译的 `ErrorRouteTable`，终态失败热路径退化为查表。
- `new`（`graph.rs:24-74`）快速失败项：节点集非空、START 节点存在、边端点引用存在；随后编译错误路由表，并检查**从 START 出发的可达性**——仅经 ERROR 跳转可达的专用错误处理节点也算可达（`graph.rs:55-58`）。
- 导航辅助一律排除 ERROR 边：`get_outgoing_edges` / `get_incoming_edges` / `find_ready_nodes`（`graph.rs:88-135`，ERROR 边不算依赖）；闭包 `reachable_from` 则横跨全部边含 ERROR（`graph.rs:138-140`）。

## 3. 错误分支

### 3.1 错误边与路由（`error_branch.rs`）

- `is_error_edge`（`error_branch.rs:17-19`）：ERROR 边是校验与可达性中的一等图边，但**绝不参与正常导航**。
- 路由表 `ErrorRouteTable`（`error_branch.rs:104-112`）：按失败节点键控的路由（按边声明顺序）+ 工作流级默认 + 挂定点集合；`build`（`error_branch.rs:119-144`）不可失败，无配置的 ERROR 边充当兜底路由；`resolve`（`error_branch.rs:159-182`）先匹配节点级路由再取工作流默认，**取消永不落入无名路由**（`error_branch.rs:175-177`）；`suspend` 显式声明优先于目标节点的挂定点标记（`error_branch.rs:150-152`，`suspend: false` 即显式退出挂定）。
- 失败控制 `ErrorFailureAction`（`error_branch.rs:22-30`）：`Interrupt`（无路由命中，保持 fail-fast）/ `Continue`（已跳转）/ `Suspended`（进入挂定点，执行停车）。

### 3.2 隔离变量作用域（`ErrorBranchScope`，`error_branch.rs:37-41`）

- 主路径变量表在分支运行期间**冻结**；分支在"入口快照克隆 + 只读错误命名空间"的覆盖层上执行，分支写只落覆盖层。
- 显式合并点以 last-writer-wins 带审计回刷主表；作用域退出时回收错误命名空间；仅 `error.` / `error:` 命名空间被排除在合并之外（`is_machinery_key`，`error_branch.rs:47-49`）。
- 分支与触发节点共享预算链：最坏下游链超 `max_steps` 仅告警（`check_branch_budget`，`error_branch.rs:191-210`；`worst_chain_len` 记忆化计算，`error_branch.rs:214-239`）。

### 3.3 错误分类与分析（`error_analysis.rs`）

- `classify_error`（`error_branch.rs:65-74`）：只对终态失败分类（瞬态重试在处理器内耗尽）；已带类别标记的保持原样避免有损往返；不做消息文本子串匹配。
- `analyze_workflow_error`（`error_analysis.rs:28-132`）：穷举 `WorkflowError` → `ErrorAnalysis`（kind、error_type、retryable、recovery_action）。要点策略：`ExecutionPaused` 是控制流而非故障；`NodeFailure` 永不可重试——重试在处理器内消耗；`OperationError`/`LoopError` → 人工介入。
- 嵌套 agent 失败投影（`error_analysis.rs:12-23`）：agent 循环失败映射为类型化 `NodeFailure`，使同步/流式 agent 路径都按类别路由。
- 模式聚合：`analyze_workflow_error_pattern`（`error_analysis.rs:157-196`）跨记录汇总受影响节点、最常见错误类型、可恢复性与恢复动作直方图，给出工作流粒度的恢复建议；`workflow_error_record`（`error_analysis.rs:138-141`）按次持久化 `ErrorRecord`。

### 3.4 与执行主循环的衔接

错误分支的运行时流转（进入/挂起/合并/丢弃、`route_node_failure` 分发）见 [01-architecture.md](01-architecture.md) 第 5 节（`coordinator/workflow/error_branch_flow.rs`）。
