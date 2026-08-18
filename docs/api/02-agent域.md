# wf-api agent 域与 entity 域

对应 `src/agent.rs`（10 个子模块）与 `src/entity.rs`（8 个子模块），约 9,200 行。TS 对应物：`AgentLoopRegistryAPI`、`AgentLoopCheckpointResourceAPI`、`AgentErrorAnalysisAPI`、`AgentExecutionRegistryAPI`、`AgentDecisionGraphAPI`、`AgentLoopMessageResourceAPI`、`AgentUserInteractionResourceAPI`、`AgentVariableResourceAPI`、`MessageResourceAPI`、`SkillRegistryAPI` 等。注：TS `AgentTriggerResourceAPI` 的 per-loop 触发器语义已废弃（Rust 中触发器为全局事件驱动资源，见 `entity/trigger.rs`）。

## 1. agent 模块

### 1.1 agent.rs（116 行）— 定义/执行记录 CRUD 直通层

对 `AgentProfileStorageMetadata`、`AgentLoopStorageMetadata`、`AgentExecution` 三个实体的 save/get/delete/list 直通（接受 `&StorageContext` 而非 `&ApiContext`），另提供 `update_agent_loop_status` 与 `list_executions_by_definition`。

### 1.2 agent_execution.rs（486 行）— Agent Loop 执行 API（核心）

- `run`：通过 `wf-agent` 的 `AgentLoopCoordinator` 启动循环；每次运行生成新 `agent_loop_id`；墙钟超时（配置 `max_execution_time`，否则 `max_iterations × 30s`，默认 90s）；结束持久化会话消息与 `AgentExecution` 记录。
- `stream`：返回 `ExecutionEventStream` 流式执行。
- `pause` / `resume` / `cancel` / `status`：经由实体中断与状态机。
- `registry`：暴露共享 `AgentLoopRegistry`。
- `AgentStateAccessor`：把 live entity 适配为共享 `StatePoint` 契约。

### 1.3 agent_loop_registry.rs（1162 行）— 最大查询注册表

- 三源摘要：live 注册表 → `agent_execution` 记录 → `agent_loop` 元数据。
- 能力：`summaries`/`summary`、按状态列表与快捷函数、`update_status`（live 状态机路由，否则改写持久化状态）、`statistics`、`cleanup_completed`（清理已终止 live 实体）、`iteration_history`、`execution_timeline`（14 种时间线条目类型）、`variable_history`、`context_evolution`、`execution_statistics`、`execution_path`。
- 共享聚合 helper `aggregate_execution_statistics`（pub(crate)，被 agent_execution_registry 复用）。

### 1.4 agent_execution_registry.rs（273 行）— 执行记录查询

`AgentExecutionFilter`（status/agent_id/parent_execution_id）、`AgentExecutionSummary`；running/paused/completed/failed 快捷查询；统计委托给 loop registry。

### 1.5 agent_graph.rs（1369 行）— Agent 决策图分析（TS `AgentDecisionGraphAPI`）

- 从迭代历史（live 或持久化）归一化为内部 `IterationSnapshot`，构造"start → 决策节点 → 工具节点 → end"图。
- 能力：`analyze`（决策节点 + 工具序列 + 已探索/未探索分支 + 路径效率）、`all_paths`（有界 DFS，`MAX_ENUMERATED_PATHS = 1000`）、`execution_path` 系列、`alternative_decisions` 系列、`decision_sequence`/`patterns`、`critical_path`（最长路径）、`path_probability_analysis`（边概率乘积 + 归一化熵作为多样性）。
- 启发式：复杂度 = 步数；一致性 = 1 − 置信度标准差；多样性 = 归一化熵。

### 1.6 agent_checkpoint.rs（447 行）— Agent Loop 检查点生命周期

- `create`：从 live 实体构建 `AgentStateSnapshot`，bincode 序列化（`wf_checkpoint::CheckpointSerializer`），并与最新检查点**链式关联**（previous/base/chain_root/chain_position）。
- `restore`：校验归属 → 解码 → 通过 `StateManager::restore_from_snapshot` 重放。
- `list`（最新优先）、`chain`（按 chain_root 分组）、`delete_for`、`statistics`（`AgentCheckpointStatistics`）。

### 1.7 agent_error_analysis.rs（447 行）— 循环执行错误分析

live 错误链优先、持久化 `AgentExecution.error` 文本兜底。能力：错误记录、`get_error_chain`（从指定错误 id 切片）、`analyze_root_cause`、统计（by_type/by_severity/recoverable/root_cause）、高级分析（首末时间、recurring）、恢复建议、相似错误聚类（归一化消息，top 20）。

### 1.8 其余

- `agent_message.rs`（310）：循环消息查询——recent/search/stats/conversation_history/count，去重 `normalize_history`。
- `agent_performance.rs`（261）：性能剖析——`AgentPerformanceProfile`（总时长、平均迭代时长、每迭代工具调用数、top-3 瓶颈）、`IterationComparison`（最快/最慢、变异系数）。
- `agent_user_interaction.rs`（281）：交互查询与**处理器槽**——`UserInteractionHandler` trait（on_interaction / on_tool_approval_requested / on_followup_question_requested），ApiContext 内单一 handler 槽，跨 agent loop / workflow / 审批协调器共享（观察者模式）。
- `agent_variable.rs`（226）：变量读写——live 快照优先合并持久化，upsert/delete/search/export/statistics。

## 2. entity 模块（低层存储实体 CRUD）

| 文件 | 行数 | 功能 |
|------|------|------|
| `message.rs` | 436 | 完整消息资源 API：CRUD、分页、搜索、`normalize_history` 去重、统计（`MessageStats` 按角色/执行/类型）、`estimate_tokens` 启发式 token 估算（拉丁/CJK） |
| `resource.rs` | 274 | **通用资源 API trait**：`ResourceApi<TEntity, TFilter>` 对任意 `BaseStorageAdapter` 自动实现（blanket impl），统一错误映射，含批量 save/load/delete |
| `skill.rs` | 420 | 技能注册表管理：启停、缓存控制、目录扫描（`SKILL.md`）、prompt 组装、**三级渐进披露**（L1 元数据 → L2 内容 → L3 资源）；无 SkillLoader 时优雅降级 |
| `task.rs` | 189 | 任务实体生命周期：CRUD、统计、按年龄清理、幂等取消、按 execution/instance 查询 |
| `trigger.rs` | 350 | 触发器全局资源：CRUD/搜索/统计，`register_trigger`（重复 id 409）、`export_triggers`、**原子 enable/disable**（adapter 的 compare-and-set，防丢失更新）、`trigger_fire_statistics`（含触发执行次数） |
| `trigger_execution.rs` | 220 | 触发执行记录：按触发器名/执行/工作流查询、`execution_history`（按执行 id，最新优先）、成功/失败统计、按年龄清理 |
| `user_interaction.rs` | 349 | 共享交互域 API：持久化 CRUD、`respond_interaction`（持久化响应 → 状态翻转 → 解析 live `USER_INTERACTION` 节点的等待，拒绝重复响应 `Conflict`）、handler 接线 |

## 3. 关键设计要点

1. **Live-entity 优先、持久化兜底**：几乎所有查询先读 `AgentLoopRegistry` live 实体，重启后降级到 `AgentExecution`/`AgentLoopStorageMetadata` 记录。
2. **函数式 API**：自由 `async fn(&ApiContext)`，镜像 TS 资源 API 命名（大量别名函数如 `get_recent_messages`）。
3. **Handler 槽（观察者）**：单个 `Arc<dyn UserInteractionHandler>` 跨域共享。
4. **检查点链式增量**：previous/base/chain_root/position 维护增量历史。