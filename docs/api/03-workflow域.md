# wf-api workflow 域

对应 `src/workflow.rs`（22 个子模块，约 10,700 行）。覆盖：定义生命周期（含草稿三态）、执行控制、builder、审批闭环、以及大量**执行数据分析**（状态、执行图、图查询、节点级迭代分析）。工作流侧检查点 CRUD 已迁出到顶层 `checkpoint` 模块（见文档 06），本模块只负责快照的构建与物化。

## 1. 定义生命周期

### 1.1 definition.rs（591 行）— CRUD 与级联

`save_workflow` = 委托 `save_workflow_with_impact`（丢报告）。完整管线：**① publish 级校验（shape+graph+reference closure，warnings 只 warn 不阻断）→ ② storage.save → ③ 注册进 registry（注册失败=整个 save 失败，防 storage 与索引静默分叉）→ ④ `ctx.clear_stale(id)`（清 Expired 标记）→ ⑤ `check_update_impact(SubWorkflow)` 返回下游影响报告**。

- `clone_workflow`：新 id（给定或 `generate_id`）+ 名称 `" (copy)"` + provenance 元数据 `cloned_from`/`cloned_at`；provenance 写失败则回滚（删 registry + 存储）。
- `rollback_workflow`：先把当前版本快照存为版本（label = 当前 version 或 `"pre-rollback-{version}"`），再 load 目标版本走 `save_workflow` 管线。
- `delete_workflow` 级联：registry remove → clear_stale → 删 draft → **经 `WorkflowCheckpointStateManager` 对每个执行逐个删检查点** → 删执行记录 → 按 `"{id}:v"` 前缀删版本复合键 → 删本体。
- `update_workflow_metadata`：description/category/tags → `storage.update_metadata` + `save_workflow`（重走完整管线）。

### 1.2 draft.rs（305 行）— Draft/Formal/Expired 三态

契约（模块头）：draft 可残缺/悬挂、**永不直接执行**；formal 必须过 shape+graph+引用闭合，是执行、默认列表、反向索引的唯一来源；Expired 是 update-impact 打的 stale 标记，正式重存即清除。

- `LifecycleStatus { Draft, Formal, Expired }`（snake_case 序列化）；`lifecycle_of`：draft 存储存在 → Draft；`ctx.is_stale` → Expired；否则 Formal。
- 校验分层：`validate_draft_parse`（仅 id/name 非空）→ `validate_draft_internal`（编辑器实时提示：shape+graph，`ValidationContext::empty()`，无 I/O）→ `validate_draft_complete`（发布前预览 = get_draft + publish 校验，错误包装 `"draft '{id}' cannot be promoted: …"`）。
- `promote_draft`：同名正式版本先快照（label `"pre-promote-{now_ms}"`）→ `save_workflow_with_impact` → 删 draft（失败保留 draft），返回影响报告；`promote_all_drafts` 逐个互不阻断。
- `hot_reload_to_draft`：文件/配置加载落到 draft 并附校验预览，不直接写正式注册表。

### 1.3 validation.rs（366 行）— 两级校验中心

- `validate_workflow`（& 自由函数）= shape（`wf_config::processor::workflow::validate_workflow_definition`）+ graph（`GraphValidator::validate`），draft-friendly 无外部引用。
- `validate_workflow_for_publish` = 两级之上 + **reference closure**（`validate_with_reference_context`）+ 工具列表 + hook handler 存在性 + prompt template 注册表检查（sync 路径因不能做 I/O 跳过该项）。
- `build_reference_context`：基于 `infra::validation::build_validation_context` 补 `workflow_graphs`（全部已存工作流转图）与 trigger_templates 详情，全内存无网络。`val_ctx_to_reference_context` 是唯一 ValidationContext→ReferenceContext 转换点。
- 已知冗余：本文件内还有一个私有路径 `save_workflow`（draft 级校验 + 直存），未被根再导出，正式路径一律用 definition.rs 版本。

### 1.4 search.rs / summary.rs / import_export.rs / version.rs / versioning.rs

- `search.rs`（394）：纯内存过滤（数据源 = 存储全量 list）。`WorkflowSearchOptions{keyword, tags, category, author, offset, limit}`；keyword 对 id/name/description/author/category/tags 六字段小写包含；tags **AND** 语义；`get_workflow_by_name`（精确首条）/by_tags/by_category/by_author 返回完整定义。
- `summary.rs`（42）：`WorkflowSummary`（node_count/edge_count 派生）投影 + `workflow_summaries`。
- `import_export.rs`（185）：导出单条（JSON Value/pretty）与批量（**以 id 为键的 JSON object**）；导入 id 策略（new_id 优先，仅当与源 id 不同且目标存在才 `AlreadyExists`，**同 id 覆盖导入不报错**），走完整 publish 管线；批量导入接受 object/array，单条失败静默跳过。
- `version.rs`（45）：按版本号的存/取/列；保存前跑两级校验（非 publish 级）。
- `versioning.rs`（255）：`VersionStrategy{Patch|Minor|Major}` + `WorkflowChanges`（字段级双层 Option，description 可显式清空）；`create_versioned_update`：keep_original 时先存当前版本（label `"pre-update"`）→ 逐字段覆盖 → 版本 = 显式值或 `auto_increment_version(当前或 "0.0.0", strategy)` → 走正式 `save_workflow`。`auto_increment_version`：按 `.` split + filter_map 解析（非数字段丢弃、缺失补 0，如 `"v2"`+Minor → `"0.1.0"`）。

### 1.5 execution.rs（65 行）— 执行记录存储薄包装

`save/get/delete/list_executions`、`update_execution_status`；`delete_execution_full`（未再导出）连带删 `agent_execution`/`agent_loop` 同 id 记录（错误吞掉）。

## 2. workflow_execution.rs（1754 行）— 执行控制（核心）

驱动 `wf-workflow::WorkflowCoordinator` 对 live `WorkflowExecutionEntity` 执行。

- 常量：`pub const DEFAULT_EXECUTION_TIMEOUT_MS = 300_000`（5 分钟）；私有保留变量键 `__execution_options`（序列化 `WorkflowExecutionOptions` JSON，resume/checkpoint 时从此读取并在快照中过滤掉）。
- `resolve_execution_graph(ctx, workflow_id)`：is_stale 仅 warn → load 定义 → **`template::composition::apply_templates_to_definition` 展开节点模板** → `definition_to_graph` → 引用闭包校验 + 工具列表校验 → 输出图。
- `execute`：resolve → hooks → spawn 实体注册进 `ctx.workflow_executions` → `resolve_options`（composition 合并，写 `__execution_options`）→ `with_timeout` 包 `run_workflow`；Err 时 `finalize_failed`（修补超时导致 coordinator 未写终态的 Running 记录）。
- `stream`：返回 `(execution_id, ExecutionEventStream)`；`spawn_execution_stream(Some(event_bus), id)` 先同步订阅；driver 任务 abort 句柄注册进 `ctx.execution_tasks`，**消费者断开即 abort**。
- `pause`：live entity pause；若 options.`max_pause_duration>0` spawn 一次性定时器，到点回读仍 Paused 才 stop（镜像 agent loop）。
- `resume`：Completed → 直接返回已存 output（不重驱）；其他终态 → Validation（"restore from checkpoint instead"）；否则重建图/ExecutorContext/中间件 → **`coordinator.resume_from(entity 快照)`**（node_results、变量、current_node、节点执行审计历史、message_contexts）→ `entity.resume()` → execute。
- `create_checkpoint`：仅 live；`build_checkpoint_snapshot` 含 input+options、层级（parent/ancestors/children）、error_records、operation_state、trigger_states（`ctx.trigger_state_registry.snapshot_for`）、节点审计、message_contexts（**fork_join_context 恒 None**，引擎只瞬态跟踪）→ `WorkflowCheckpointCoordinator.create_manual_checkpoint`。引擎内自动快照经 `WorkflowCheckpointIntegration`（**默认策略：每个完成节点后 + 无条件 start/end**）。
- `restore_checkpoint`：coordinator restore → `restored_identity`（优先 snapshot 的 `execution_config`，旧检查点回落持久记录 + default）→ **物化全新 live entity**（全量变量、node_results、current_node、**replay 节点审计历史**、hierarchy 祖先链）→ continuation options **剥离 max_steps 与 max_execution_time**（已消耗预算不重新限制续跑）→ 替换旧句柄（先 unregister）→ 返回 `RestoredCheckpoint`；`restore_and_resume` 立即续跑。
- `cancel`/`status`（engine timeout 读作 Failed）、`execution_summaries`（纯 persisted 投影）。
- `definition_to_graph`（pub）：**边界由节点类型推导，顺序无语义**——start = `Start` 或 `StartFromMessage`，end = 所有 `End`（无 End 时所有 `ContinueFromMessage` 兜底）；节点 inner = config 与 execution_config 合并（**execution_config 覆盖 config**，保住 checkpoint/timeout/retry 等运行字段）。

## 3. 执行分析

### 3.1 execution_state.rs（1725 行，最大）— 执行状态查询

数据源选择是每个查询的固定模式：live（`source:"live"`，全字段）→ persisted（`"persisted"`，**completed_nodes/节点历史恒空**——持久边界不保留）→ `"unknown"` 空视图（不报错）；agent 侧四级（live → agent_execution → agent_loop 元数据 → unknown）。`WorkflowStateAccessor` 适配共享 `StatePoint`。

能力：状态/变量、**状态迁移历史**（从 `event_bus.recent_events()` 过滤 lifecycle 事件重建，首 from="Created"，相邻去重）、执行上下文（pending 用 `reachable_nodes` BFS 可达集裁剪 → 不可达归 skipped；progress = completed/total）、调用栈、内存（启发式：序列化变量 + 128B/记录 + 64B/完成节点）、变量快照（按时间范围）、上下文演化（total_variable_changes 是**下界**）、节点迁移、关键上下文快照、节点输入上下文（transition 时间戳 ±1000ms 匹配变量快照）、状态迁移分析（top-10 transition 对、平均驻留）；agent 侧 `agent_execution_get_state/variables/iteration_history`。私有 `execution_graph()` 与 execution_graph.rs 的 resolve_graph 有一份重复实现（已知事实）。

### 3.2 execution_graph.rs（959 行）— 执行图/决策点分析

纯函数算法与异步获取分离。`MAX_ENUMERATED_PATHS = 1000`（全 crate 共享的路径枚举上限）；`dfs_paths`（pub(crate)，agent 图复用）：显式栈、路径内不重复访问、结果按长度降序 + 节点序列 tie-break。

- `analyze`：路径枚举、critical_path=**最长**结构路径、节点类型分布、已执行（live 按 start_time 排序过滤 success；persisted 按 node_results）/未执行节点、决策点。
- `analyze_decision_points`：条件出边 ≥2 才算决策点；taken 判定 = 执行序列中该节点之后第一个出现的后继对应的边。
- `get_slow_nodes(percentile)`：clamp 0..1，保留 `ceil(len×(1−p)).max(1)`（默认用法 0.8 = 最慢 20%）。
- `analyze_efficiency`：optimal=最短结构路径；ratio=executed/optimal；retry_count=失败尝试数。
- `get_path_probability_analysis`：**k 条条件出边均分 1/k，无条件边 1.0**；路径概率=连乘；`path_diversity` = 归一化熵/log2(路径数)。
- `record_execution_graph`/`clear_execution_data`：图回写/清空持久执行记录（重启存活）。
- `resolve_graph`（execution-id 版，pub(crate)）：persisted record.graph 优先，否则按 workflow_id 取定义转图——**与 workflow-id 版同名不同物**。

### 3.3 graph_query.rs（458 行）— 结构查询直通层

`get_graph`（走 publish 校验的 resolve）→ summary/nodes/nodes_by_type/edges/neighbors；`graph_analysis`/`detect_cycles`/`topological_sort`/`reachability` 直通 `wf-workflow::analysis`；`list_graph_workflows` 读 registry（非存储）；执行侧委托 `execution_graph::resolve_graph`。`GraphEdgeView.edge_type` 是 Debug 串（`format!("{:?}", type)`）。

### 3.4 workflow_iteration.rs（859 行）— 节点级迭代分析

内部 `collapse_node` 归一化：live 尝试历史与持久 `node_results` 统一形状、合并为每节点一条（retry_count=attempts−1，status 判定：最新尝试无 end_time 未成功→"running"，成功→"completed"，有 error→"failed"，其余→"cancelled"）。能力：`get_execution_node_analyses`、`get_tool_dependency_chain`（TOOL/HTTP/MCP/SCRIPT 类型节点 → 单条依赖）、`get_llm_reasoning_path`（LLM/REASONING 节点；content 探测键 `content,text,response,message,answer`）、`get_execution_path`（`is_optimal` = 全 completed 且无重试）、`get_optimization_opportunities`（**时长 >5s→medium、重试 >2→high、工具依赖 >5→medium**）、节点统计/按类型/失败节点。live 侧 input/output 恒 None，persisted 侧 node_type 恒 "unknown"。

### 3.5 iteration.rs（239 行）— Agent Loop 迭代聚合

`analyze(agent_loop_id)`：live 迭代历史优先、持久化兜底；按工具聚合（次数/总时长/平均/失败），推导最慢/最频繁工具；优化提示规则：失败调用、慢工具 avg>5s、纯 LLM 迭代 >50%、无工具。已知退化：`average_iteration_duration_ms` 直接复制 `average_tool_duration_ms`。

## 4. 审批与人工环路

### 4.1 approval.rs（575 行）— API 层审批闭环（TS `ToolApprovalCoordinator`）

`ApprovalFlow`（pub(crate)，Clone，只克隆 storage/event_bus/handler 槽三件）。`check_and_request_approval`：options 缺省 `ToolApprovalOptions::balanced_defaults` → 引擎 `wf_tools::approval::ToolApprovalCoordinator::evaluate_with_mcp_context`（presets/patterns/file/command/network 规则在引擎侧）→ Approve=`AutoApproved`（不建交互）、Deny=`Rejected`（**策略拒绝从不打开人工环路**）、Ask → `request_user_approval`。

`request_user_approval` 步骤：持久化 `user_interaction`（type `tool_approval`）→ **先注册 `InteractionWait` 通道再发布事件**（防 fast-response 赛跑；wait drop 即注销，取消不泄漏 sender）→ 发布 `ToolApprovalRequested` → 通知 `UserInteractionHandler` → 等待响应。**无超时语义**："未答复"绝不降级为自动决定；等待与执行同寿，通道被关 → Err "cancelled"；响应畸形 → Err。`execute_tool_with_approval` = 审批 +（批准时 **edited_parameters 覆盖原参数**）+ `llm::tool::execute`。`ApprovalStatus { AutoApproved, Approved, Rejected }`。

### 4.2 tool_approval_handler.rs（97 行）— host 侧审批 handler（新）

`InteractionApprovalHandler` 实现引擎 `ToolApprovalHandler` 契约，把 policy 的 Ask 决策桥接到 4.1 的持久化流程；失败结果映射为"被拒"（不 panic）。`host_tool_approval(ctx, execution_id)`：`ctx.tool_approval` 配置 None 或 disabled → **None（保留库默认自动批准）**；否则返回 options+handler，宿主按执行逐个 attach（agent coordinator `with_approval_options/handler`、workflow ExecutorContext `with_tool_approval`），**caller 自带 handler 优先**。

## 5. builder 与 composition

`builder.rs`（525，`WorkflowBuilder<Empty/Building>`：`build()` 跑两级校验、`save()` 走正式管线；`from_config_json/toml` 校验延迟到 build；`add_conditional_edge` condition 为 `${...}` 运行时表达式）与 `node_builder.rs`（434，`NodeBuilder<NoType/Typed>`：13 个类型化便捷构造器——fork 固定 Parallel、join 固定 WaitForAll、loop_start 固定 max_iterations=1、`with_checkpoint` 写入 execution_config 覆盖 workflow 级策略）、`execution_builder.rs`（527，**先订阅后 spawn**、`CallbackPack` 分发 NodeCompleted/NodeFailed/WorkflowExecutionFailed、`execute_with_result` 从持久记录投影、`execute_stream` 回调转发任务注册键 `"{execution_id}:callbacks"`）——详见 [07-llm、模板与builder.md](./07-llm、模板与builder.md) 的模式综述。

`composition.rs`（149 行）：`resolve_options` = caller `WorkflowExecutionOptions` 必胜 → 存储 `WorkflowConfig` 补默认 → 最终兜底 `enable_checkpoints: Some(true)`；`workflow_hooks_to_definitions`（自被删除的顶层 composition/hook.rs 并入）。

`variable.rs`（103 行）：`VariableStore` 与表达式求值（`wf-workflow::variable`）的薄封装。

## 6. 跨 crate 关系

| 关注点 | 所在 crate |
|--------|-----------|
| 持久化 | `wf-storage` 适配器（经 `StorageContext`）——真理来源 |
| 执行引擎 | `wf-workflow`（Coordinator、GraphValidator、analysis、InteractionWait） |
| 检查点引擎 | `wf-checkpoint` facade（workflow_execution.rs 创建/恢复、definition.rs 级联删除、entity/variable.rs 历史兜底、audit 快照兜底） |
| 审批策略 | `wf-tools::approval` |
| 依赖影响 | `infra::dependency`（保存后 check_update_impact / mark_stale） |
| Live vs 持久化 | `ApiContext` 两类 live 注册表（workflow_executions / agent_loops） |

本质是**读/控制 API 层**：生命周期与执行控制委托引擎，多数代码（execution_state、execution_graph、graph_query、workflow_iteration、iteration）是对执行数据的分析重建。
