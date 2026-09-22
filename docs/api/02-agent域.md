# wf-api agent 域与 entity 域

对应 `src/agent.rs`（16 个子模块，约 7,400 行）与 `src/entity.rs`（7 个子模块，约 2,700 行）。TS 对应物：`AgentLoopRegistryAPI`、`AgentLoopCheckpointResourceAPI`、`AgentErrorAnalysisAPI`、`AgentExecutionRegistryAPI`、`AgentDecisionGraphAPI`、`AgentLoopMessageResourceAPI`、`AgentUserInteractionResourceAPI`、`AgentVariableResourceAPI`、`MessageResourceAPI`、`SkillRegistryAPI` 等。

> 触发器、模板、builder 已从独立模块下沉到各域：agent 的 builder 在 `agent/builder.rs`，composition 在 `agent/composition.rs`，触发器在顶层 `trigger/`，见对应文档。

## 1. agent 模块

### 1.1 agent.rs（190 行）— 定义/执行记录 CRUD + 正式化入口

三段 16 个函数，CRUD 部分接受 `&StorageContext`（非 `&ApiContext`）：

- profile CRUD：`save/get/delete/list_agent_profile`（get 不存在 → `not_found("agent_profile", id)`）。
- loop 定义 CRUD：`save/get/delete/list_agent_loop` + `update_agent_loop_status`（走适配器 `update_status(id, &str)`，**状态是自由字符串**）。
- 执行记录 CRUD：`save/get/delete/list_agent_execution` + `list_executions_by_definition`。
- **正式化两函数**（新增语义，接 `&ApiContext`）：`validate_agent_definition`（走 `AgentValidator` 做 shape 校验 + 额外检查 `system_prompt_template_id` 存在于 `ctx.registries.templates`，返回 warnings 列表）、`save_agent_template`（先 validate → 构造 `AgentTemplate` → **先 `storage.agent_template.save` 再 `registries.upsert_agent_template`**，注册冲突 → `Conflict`，"持久化先于注册以保证重启存活"）。

### 1.2 agent_execution.rs（796 行，测试约 360 行）— Agent Loop 执行 API（核心）

超时策略：`DEFAULT_AGENT_TIMEOUT_MS = 90_000`（30s × 默认 3 迭代）、`AGENT_TIMEOUT_PER_ITERATION_MS = 30_000`；`agent_timeout_ms(config)`：`max_execution_time` 优先 → 否则 `max_iterations × 30s` → 否则 90s（`.max(1)`），经 `with_timeout` 映射 `ApiError::Timeout`。

核心类型 `RunAgentLoopParams`：`config`、`input`、`agent_loop_id: Option<Id>`（caller-preset 运行 id，允许首事件前关联控制句柄）、`approval_options`、`approval_handler`（引擎策略先行，仅 `Ask` 到达 handler）。

- `run`：先 `gate_agent_config`（引擎 `AgentLoopValidator::validate_or_fail`，error 拒、warning 放行）→ 组装 `AgentLoopCoordinator`（含 host 默认 approval handler，caller 优先）→ 每次运行生成新 `agent_loop_id`（`config.agent_id` 只是定义标签）→ 超时执行。成功 `persist_conversation`；失败把 live entity `state.fail()` 并落终态 `AgentExecution`（补偿超时使 start 记录停在 Running）。
- `stream`：**不包超时**；先解析 id 再建 coordinator（host approval handler 按同一 execution id 建交互），`from_agent_stream` 适配。
- `pause` / `cancel`：live entity `entity.pause()`（迭代间生效）/ `entity.stop()`。
- `resume`：先拒绝终态（completed/failed/cancelled → Validation，提示改用 restore）。
- `resume_from_checkpoint(..., in_place)`：**所有权前置检查**（`AgentCheckpointStateManager::list_by_entity` 确认 checkpoint 属于该 loop）。`in_place=false`（默认，branch）：新执行 id 经 `parent_execution_id` 关联源，源链不 mutate；`in_place=true`：沿用源 id，要求源 terminal/paused（live run 拒），caller-preset 冲突拒。与手动 checkpoint 共用同一 store/chain。
- `status`：返回**类型化** `wf_types::ExecutionStatus`（不再返回 Debug 字符串）。

`AgentStateAccessor`：live entity → `StatePoint` 适配（`memory_usage` = 各变量 JSON 字节和，快照失败降级 Running+空态）。

### 1.3 agent_loop_registry.rs（1150 行）— 只读三源查询注册表

**不创建 loop**。三源摘要 `all_summaries`：BTreeMap 按 id 去重，优先级 live（`entity.model()` 作 profile_id）→ `agent_execution` 记录（`context.profile_id`）→ `agent_loop` 元数据（`tool_call_count` 恒 0、`end_time`/`execution_time` 恒 None、status 经 `parse_status` 字符串解析）。

能力约 17 个：`summaries/summary`、`list_by_status`、running/paused/completed/failed 快捷、`update_status`（live 按目标状态**路由生命周期方法**：Running→resume、Paused→pause、Cancelled/Stopped→stop，不可表达的状态回落改写持久记录）、`statistics`、`cleanup_completed`（清 4 种终态）、`iteration_history(_summary)`、`execution_timeline`、`variable_history`（**live-only**，每变量至多 1 条 `from:None→当前值`）、`context_evolution`、`execution_statistics`、`execution_path`。

`ExecutionTimelineEntryType` **14 个变体**（Execution Start/End/Completed/Failed/Cancelled/Stopped/Timeout、Iteration Start/End、Error、Interruption Pause/Resume/Stop/Timeout）。`aggregate_execution_statistics`（pub(crate)，被 execution registry 复用）：Running 无 end 按 `now-start` 计时长，`round2`，Cancelled 与 Stopped 合并计入 cancelled。`IterationDetail.duration`：进行中为 `-1`。

### 1.4 agent_execution_registry.rs（283 行）— 执行记录查询（两源）

`AgentExecutionFilter{status, agent_id(定义 id), parent_execution_id}`；running/paused/completed/failed/has/count/`execution_statistics`/`status_statistics`。**`parent_execution_id` 只有 live entity 提供**（持久记录该字段恒 None）；结果按 `start_time` 逆序；`count` 用 HashSet 对 live+persisted 按 id 去重（不重复计数）。统计委托 loop registry 的共享聚合。

### 1.5 agent_graph.rs（1326 行）— Agent 决策图分析（纯查询）

从迭代历史归一化构造 `start → decision → action/error → end` 图。约 22 函数 + 13 视图。启发式：决策标签 `decision = "tool:{首个工具名}"`（无工具则 `llm`）；`path_efficiency = 工具调用数 / max(迭代数,1)`；unexplored 工具 = 可用工具集 − 已调用集；**所有记录的边 `was_taken=true, probability=1.0, weight=1.0`**（真实分支概率未记录）；`graph_density = edges/(n(n-1))`；`consistency_score = max(1 − 置信度标准差, 0)`（缺失 confidence 按 0.5）；`path_diversity` = 归一化概率 Shannon 熵 / log2(路径数)。路径枚举复用 `workflow::execution_graph::dfs_paths`（`MAX_ENUMERATED_PATHS = 1000`）。

### 1.6 agent_checkpoint.rs（443 行）— 手动 checkpoint 走统一 coordinator

**与引擎快照共享 store 与 chain 逻辑**：`create` 的 live 分支用引擎 `AgentCheckpointIntegration::create_checkpoint(CheckpointTiming::Manual)`；非 live 分支走 `AgentCheckpointCoordinator` 流水线（`manual_allowed` → prepare → build → validate → persist → save_file_snapshot）。链式字段由 coordinator 决定：`checkpoint_type`（Full→Delta）、`chain_position`、`previous/base/chain_root_checkpoint_id`、`blob_size`。`restore`：`coordinator.restore`（delta 链自动解析）→ 校验 `agent_loop_id` 归属 → live 时 `state.restore_from_snapshot` 重放（error_records 重置、timeout_count 清零），非 live 仅 warn（**restore 幂等，不因缺 loop 失败**）。`list` 时间逆序、同毫秒以 id 降序 tie-break；`chain` 按 chain_root 分组、chain_position 升序；`statistics`（by_type full/delta、active={Active,Completed}、avg_blob_size）；`agent_loop_id=None` 走全局查询按 `entityType=agent_loop` 过滤。全部经 `wf-checkpoint` facade，不触碰内部 crate。

### 1.7 agent_error_analysis.rs（396 行）— 循环执行错误分析

live `state.error_records()` 优先、持久 `AgentExecution.error` 文本用 `error_common::minimal_record` 兜底。能力：错误记录、`get_error_chain`（`from_error_id` 切片起点，None 全链）、`analyze_root_cause`（`root_cause_id==id || parent 无` 首条）、`get_error_statistics`、`get_advanced_error_analysis`（首末时间、recurring=`len>1`）、`get_recovery_proposal`（可恢复无 action → 回落 `"retry"`）、`get_similar_errors`（全部持久 Failed 按 normalize_message 聚类，逆序 truncate 20）。severity 启发式：`is_recoverable → Warning 否则 Critical`。共享逻辑走 `analysis::error_common`。

### 1.8 其余 agent 子模块

- `agent_message.rs`（314）：recent（默认 20）/search/stats/conversation_history（`max_messages` 时 drain 最旧保留最新切片）/count/global_stats/别名。**`dedupe_and_delete` 会物理删除存储冗余副本**（故意不叫 `normalize_history`——entity 层的 `normalize_history` 是只读去重）。
- `agent_performance.rs`（258）：`analyze_performance`（bottlenecks = 时长逆序 top-3）、`execution_timeline`（转发 loop registry）、`iteration_comparison`（`range_ms=max-min`、`variation`=变异系数 std/mean，round2）。
- `agent_user_interaction.rs`（211）：只做 loop 作用域查询，**`UserInteractionHandler` trait 与事件记录定义在 `entity/user_interaction.rs` 并在此 `pub use` 再导出**（避免 entity → agent 反向依赖）；`respond` 先验 `interaction.execution_id == agent_loop_id` 所有权。
- `agent_variable.rs`（224）：`get_execution_variables`（entity::variable::export 打底，live `variable_snapshots()` 覆盖同名键）、get/has/statistics/search/export/set/delete（scope 固定 `default`）。
- `agent_draft.rs`（117）：镜像 workflow draft 三态，复用 `workflow::draft::LifecycleStatus`；`promote_draft` → `save_agent_template` → 删 draft。
- `agent_config.rs`（33）：常量 `DEFAULT_AGENT = "@standard/main"`、`DEFAULT_MODEL = "default"`、`DEFAULT_MAX_ITERATIONS = 50`；`build_agent_loop_config` 构造"未解析意图"（max_iterations 故意留 None，由 composition 最终填）。
- `validation.rs`（97）：`AgentValidator`（四步：shape → profile 引用 → 工具四桶列表 → **tool_call_protocol 与 profile format 兼容性**，用引擎 `AgentLoopValidator::validate_tool_call_protocol` 保证 API 时与运行时规则一致）。
- `builder.rs`（900）、`composition.rs`（636）见 [07-llm、模板与builder.md](./07-llm、模板与builder.md)。

## 2. entity 模块（低层存储实体 CRUD）

### 2.1 execution.rs（319 行）— 跨域执行 id 歧义解析

`ExecutionDomain { AgentLoop, Workflow }`；`checkpoint_entity_type`：AgentLoop→`"agent_loop"`、Workflow→`"checkpoint"`；`parse` 接受多种别名（agent/agent-loop/agentloop、workflows）。`resolve_execution` 三层成本序探测 + 短路：双 live registry（内存）→ 持久 execution 记录（一层给出结论即跳过第 3 层）→ ghost id（仅 checkpoint 分区可见，按 entity_type 过滤防串域）。`execution_verdict` 9 臂真值表：双 miss → `ExecutionNotFound`；双 hit 无 override → `Conflict`（含 "specify --domain explicitly"）；单 hit 与 override 不符 → `Validation`（指明实际域）。`ensure_execution_domain`：端点域断言（路由本身即显式覆盖）。

### 2.2 其余 entity 子模块

| 文件 | 行数 | 功能 |
|------|------|------|
| `message.rs` | 605 | 消息 CRUD + **游标分页**（`page_by_execution`/`page_by_agent_loop` 取 limit+1 探测 `has_more`，newest-first，before_timestamp 缺省 `i64::MAX`）；`normalize_history` 只读去重；`latest_session_anchor`（供 CLI `--resume`）；`estimate_tokens` 启发式（词数 + (字符数 − 词数×5)/4，近似拉丁/CJK）；`pub(crate)` 共享 `role_name`/`message_text` |
| `variable.rs` | 544 | 复合键 `(execution_id, scope, name)` + 确定性 id（`"{exec}::{scope}::{name}"`，全局槽 `__global__`，空 scope 归一 `default`），`set` 幂等 upsert（保留 created_at）、`define` create-only（重复 `AlreadyExists`）；`history(name)` **四源并集**带 `VariableSource` 标签（Storage/Live/Persisted/Checkpoint，checkpoint 源走 coordinator restore 解析 delta 链、best-effort）；`variables_at_node`（node_id ∪ default ∪ global） |
| `resource.rs` | 285 | **通用 `ResourceApi` blanket trait**：任何 `BaseStorageAdapter` 自动获得 get/list/save/delete/exists/clear/count_by_field/batch；**全部用 RPITIT**（`-> impl Future + Send`）而非 `async fn in trait`，使消费者无需 boxing 即可 `tokio::spawn` |
| `skill.rs` | 394 | 技能注册表（`ctx.tool_registry.skill_loader()`）：**无 loader 时元数据查询降级空集合、显式操作报错**；启停、缓存控制（含单技能粒度 clear）、`scan_skills`（扫含 `SKILL.md` 的子目录）、`to_prompt` 组装、**三级渐进披露**（L1 元数据 → L2 content → L3 resources，Binary 有损转文本） |
| `task.rs` | 189 | 全接收 `&StorageContext`（不经 ApiContext）：CRUD、统计、`cleanup_tasks(older_than)`、幂等 `cancel_task`（读-改-写）、按 execution/instance 查询（全表 list 后内存过滤） |
| `user_interaction.rs` | 395 | 共享交互层：`UserInteractionHandler` trait（必选 `on_interaction`，`on_tool_approval_requested`/`on_followup_question_requested` 默认 no-op，让既有实现者无需改动）；handler 槽进程作用域挂 `ApiContext`，**同一 handler 服务 workflow `USER_INTERACTION` 节点、agent loop、审批协调器**；`respond_interaction` 闭环：已 responded → `Conflict`，否则**先持久化再唤醒 live 等待节点**（返回值忽略，无等待者时 false）；状态是自由字符串 `pending`/`responded` |

> 旧的 `entity/trigger.rs`、`entity/trigger_execution.rs` 已消失：trigger 实体模型（`TriggerStorageMetadata`）被 TriggerTemplate 模板模型整体取代，触发相关能力迁至顶层 `trigger/` 模块，详见 [06-checkpoint与trigger域.md](./06-checkpoint与trigger域.md)。

## 3. 关键设计要点

1. **Live-entity 优先、持久化兜底**：几乎所有查询先读 `ctx.agent_loops` live 实体，重启后降级到 `AgentExecution`/`AgentLoopStorageMetadata`。持久契约两套并存：`AgentLoopStorageMetadata.status` 是字符串、`AgentExecution.status` 是类型化 `ExecutionStatus`。
2. **函数式 API**：自由 `async fn(&ApiContext)`，低层直通用 `&StorageContext`。
3. **Handler 槽（观察者）**：单个 `Arc<dyn UserInteractionHandler>` 跨域共享，定义在 entity 层防依赖倒置。
4. **checkpoint 统一化**：手动与引擎 checkpoint 共享 coordinator、store、chain 逻辑，单一 checkpoint 历史；restore 自动解析 delta 链。
5. **composition 边界**：模板解析/选项合并集中在 composition，execution API 保持纯执行器。
