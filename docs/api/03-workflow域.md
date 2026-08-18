# wf-api workflow 域

对应 `src/workflow.rs`（11 个子模块），约 8,400 行。覆盖：定义生命周期、执行控制、检查点、审批、触发器命令，以及大量**执行数据分析**（执行状态、执行图、图查询、节点级迭代分析、检查点链）。

## 1. workflow.rs（1380 行）— 定义与执行记录生命周期

- 校验：两阶段——`wf-config` 定义校验 + `wf-workflow::GraphValidator` 图校验（fork/join 配对、循环、start/end、环、可达性）。
- CRUD：save/get/delete（删除级联执行记录、检查点、版本快照），保持内存执行注册表双写一致。
- `clone_workflow`：克隆带 `cloned_from`/`cloned_at` 溯源，失败补偿（删除克隆）。
- `rollback_workflow`：恢复版本快照，自动保留回滚前定义为新版本。
- 版本化更新：`create_versioned_update` / `auto_increment_version`（semver `Patch/Minor/Major`）。
- 导入导出：`export_workflow(s)` / `import_workflow(s)`（含 JSON 变体）。
- 搜索：`search_workflows`、按 name/tags/category/author 查询（内存过滤）。
- 执行记录 CRUD：`save/get/delete/list/update_execution_status`（薄存储包装）。

## 2. workflow_execution.rs（1495 行）— 执行控制 API（核心）

驱动 `wf-workflow` 的 `WorkflowCoordinator` 对 live `WorkflowExecutionEntity` 执行：

- `resolve_graph`：定义 → `definition_to_graph`（节点配置 + 执行配置合并进运行时 `inner`）→ 图校验。
- `execute`：跑至完成，默认墙钟超时 5 分钟（`DEFAULT_EXECUTION_TIMEOUT_MS`），超时映射 `ApiError::Timeout`。
- `stream`：返回 `(execution_id, ExecutionEventStream)`，spawn 独立 driver 任务（注册进 `ctx.execution_tasks`，断开/关停可 abort）。
- `pause`/`resume`：节点间中断；resume 从已完成节点输出重新播种，恢复捕获的输入/选项（存于保留变量 `__execution_options`）。
- `create_checkpoint`：live 实体快照经 `wf-checkpoint` 的 `WorkflowCheckpointCoordinator`（`CheckpointTrigger::Manual`）持久化。
- `restore_checkpoint`：从快照**物化全新 live 实体**（变量、节点结果、当前节点、重放节点执行审计历史、恢复捕获选项并清零步/时间预算），替换旧句柄；`restore_and_resume` 立即续跑。
- `cancel`/`status`、`execution_summaries` 投影。

## 3. execution_state.rs（1745 行，最大）— 执行状态查询

- `WorkflowStateAccessor`：live 实体适配共享 `ExecutionStateAccessor`/`StatePoint`（`infra::state_tracker`）。
- 工作流侧：状态/变量（`source: live/persisted/unknown`）、状态迁移历史、执行上下文、调用栈、内存使用、变量快照（按时间范围）、上下文演化、节点迁移、关键上下文快照、节点输入上下文、状态迁移分析。
- Agent 侧：`agent_execution_get_state` / `agent_execution_variables` / `agent_execution_iteration_history`。
- 全部 `*View` 结构体可序列化。

## 4. execution_graph.rs（957 行）— 执行图/决策点分析

- 纯函数算法（可单测、无 I/O）与异步数据获取分离：`enumerate_paths`（有界 DFS）、`analyze_decision_points`（≥2 条条件边分支节点）、`reachable_nodes`（BFS）。
- `analyze`：完整路径分析（关键路径、节点类型分布、已执行/未执行节点）。
- `get_slow_nodes`：最慢 1% 分位节点。
- `analyze_efficiency`：已执行步数 vs 最短结构路径。
- `get_alternative_paths`：决策点未走条件分支。
- `get_path_probability_analysis`：启发式概率模型（条件边均分 `1/k`，真实分支概率未记录）。
- `record_execution_graph` / `clear_execution_data`：执行图持久化到 `WorkflowExecution` 记录（重启存活）。

## 5. graph_query.rs（435 行）— 图结构查询直通层

`get_graph`（定义 → 图）、summary/nodes/nodes_by_type/edges/neighbors；`graph_analysis`/`detect_cycles`/`topological_sort`/`reachability` 直通 `wf-workflow::analysis`；执行侧图查询委托 `execution_graph::resolve_graph`。

## 6. workflow_iteration.rs（862 行）— 节点级迭代分析

- 内部 `RawNodeAttempt` 归一化层：live 历史与持久化 `node_results` 统一形状；`collapse_node` 合并尝试（最新胜出）。
- 能力：`get_execution_node_analyses`（每节点一条扩展记录）、`get_tool_dependency_chain`（按节点类型推断 TOOL/SCRIPT/HTTP 依赖）、`get_llm_reasoning_path`（LLM 类型节点推理重建）、`get_execution_path`（带 `is_optimal`：全部完成且无重试）、`get_optimization_opportunities`（启发式：时长 >5s 中、重试 >2 高、工具依赖 >5 中）、节点统计/按类型/失败节点。

## 7. iteration.rs（241 行）— Agent Loop 迭代聚合

`analyze(ctx, agent_loop_id)`：live 迭代历史优先、持久化 `iteration_history` 兜底；按工具聚合（次数/总时长/平均/失败），推导最慢/最频繁工具，生成可读优化提示（失败调用、慢工具 >5s、>50% 纯 LLM 迭代、无工具）。

## 8. approval.rs（526 行）— 工具审批协调器（TS `ToolApprovalCoordinator`）

- `check_and_request_approval`：策略评估（`wf_tools::approval::ToolApprovalCoordinator::process_batch`）：自动批准 / 策略拒绝 / 打开人工环路。
- `request_user_approval`：持久化 `user_interaction`（tool_approval 类型）→ 注册 `wf-workflow::InteractionWait` 通道（先注册再发布，防响应竞态）→ 发布 `ToolApprovalRequested` → 通知 `UserInteractionHandler` → 带超时等待。
- `execute_tool_with_approval`：审批 + 工具执行组合。
- `ApprovalStatus`：`AutoApproved/Approved/Rejected/TimedOut`。

## 9. checkpoint.rs / file_checkpoint.rs — 检查点 CRUD 与链分析

- `checkpoint.rs`（421）：存储薄包装 + 链分析——`get_checkpoint_chain`（时间序排列、计算链转移与时间范围）、`list_checkpoints_by_time_range`（按工作流 join 执行再按时间窗过滤）。
- `file_checkpoint.rs`（204）：文件检查点 CRUD，`delete_file_checkpoints_by_entity` 支持 `keep_latest`（按 created_at 降序保留最新 N 个）。

## 10. execution_trigger.rs（133 行）— 执行级触发器命令

`enable`/`disable`/`is_enabled`（委托 `entity::trigger` 原子 compare-and-set）+ `trigger_execution_history`（按执行过滤，最新优先）。

## 11. 跨 crate 关系

| 关注点 | 所在 crate |
|--------|-----------|
| 持久化 | `wf-storage` 适配器（经 `StorageContext`）——真理来源 |
| 执行引擎 | `wf-workflow`（Coordinator、GraphValidator、analysis、InteractionWait） |
| 检查点引擎 | `wf-checkpoint`（仅 workflow_execution.rs 使用） |
| 审批策略 | `wf-tools::approval` |
| Live vs 持久化 | `ApiContext` 中两类 live 注册表（workflow_executions / agent_loops） |

本质是**读/控制 API 层**：生命周期与执行控制委托引擎，而大部分代码（execution_state、execution_graph、graph_query、workflow_iteration、iteration、checkpoint 链）是对执行数据的分析重建。