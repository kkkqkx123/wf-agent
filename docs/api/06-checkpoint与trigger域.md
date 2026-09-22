# wf-api checkpoint 与 trigger 域

对应 `src/checkpoint/`（4 文件 + 根门面，约 1,350 行）与 `src/trigger/`（5 文件 + 根，约 1,170 行）。两者都是从旧结构升格的顶层域：checkpoint 能力原散在 workflow 模块（`workflow/checkpoint.rs`、`file_approval.rs`、`file_provenance.rs`），trigger 能力原在 entity/builder/template 三处（旧的 `entity/trigger.rs` per-loop 触发器实体模型已整体删除，被全局 TriggerTemplate 模型取代）。

## 1. checkpoint 模块 — 共享检查点面

模块定位（根注释）：通用 checkpoint CRUD、文件级检查点、来源查询、审批流，**agent 与 workflow 两条执行路径共用**。机制全部在 `wf-checkpoint` facade 之下（内部 checkpoint-base/checkpoint-state/checkpoint-file + layertwine 存储引擎），wf-api 只做视图整形、错误映射与门禁。

### 1.1 record.rs（572 行）— 统一记录 CRUD 与链分析

`wf_types::Checkpoint` 记录存储的单一门面：`save/get/delete/list_checkpoints`、`list_checkpoints_by_entity(_entities)`、`get_latest_checkpoint`、`delete_checkpoints_by_entity`、实体元数据 get/set、`list_checkpoints_by_time_range`（workflow 专用：按 workflow_id 列执行 id 集 → `list_by_entities_with_metadata` → 时间窗过滤）。

- **写入门禁 `validate_chain_record`**：id/entity_id 非空；`Full` 免链路字段；`Delta` 必须同时带 `previous/base/chain_root_checkpoint_id` + `chain_position`，缺一拒绝写入（"不存坏链"）——只接受 coordinator 构造的链一致节点。
- **链分析 `build_chain`**：按 timestamp 排序、`windows(2)` 生成 `CheckpointTransitionView{from,to,elapsed,type:"FULL"|"DELTA",trigger_description}`（description 取 `custom_fields["description"]`）；`CheckpointChainAnalysisView` 含 total_elapsed 与 time_range。`get_checkpoint_chain` 硬编码 workflow 域；域解析版是 `chain_for_execution`。
- **域判别**：同一 checkpoint adapter 以记录 `entity_type` 列分区（`"agent_loop"` / `"checkpoint"`）。`entity_type_for_execution` 经 `resolve_execution_with_override` 判域；`ensure_checkpoint_domain(ctx, cid, expected)` 做反向断言，不匹配错误消息直接指引正确端点（"…belongs to {actual}, not {expected}; use the {actual} checkpoint endpoint"）。
- `gc_for_execution(execution_id, before)`：None 删全部、Some 删早于时间戳者，计数返回；先解析域。
- 消费方：wf-server workflow/checkpoints.rs（链端点对 `ExecutionNotFound` 降级返回 `empty_chain` 200，域错仍报错）；agent/executions.rs（restore/resume 双重域校验，未知 cid 的 NotFound 改映射为 400 Validation）；CLI checkpoint 命令。

### 1.2 file.rs（180 行）— 文件级 workspace 门面

调用方显式传 `&FileCheckpointManager`（不藏于 ctx）。能力：`create_file_checkpoint`/`restore_workspace_from_checkpoint`（返回 restored 计数，默认 `FileCheckpointOptions`）、`scan_workspace`（`ScanConfig` + 自定义 ignore patterns，返回 `WorkspaceScanResult{files,dirs,empty_dirs}`）、`list_file_changes`、`diff_actors`/`diff_against_staged`、`list_partitions`、`get_actor_workspace`、`list_conflicts`、`filter_changes_by_kind`（watcher 记录纯过滤）。分工：layertwine 负责 content-addressed 存储/三方合并/GC 机制，checkpoint-file 的 manager 负责 actor/approval/staged/feature 分区编排，本层只做视图（`FileCheckpointSummary`）与 `ApiError::execution_with_source` 转换。

### 1.3 approval.rs（427 行）— workspace 变更审批流（含 LLM 单次评审）

多 agent 并发写 workspace 时"改完不直接生效，进审批层等人/模型裁决"。全部经私有 `manager(ctx)`：未装配 file_checkpoint_manager 时统一报 "file checkpointing is not enabled; set file_checkpoint.enabled=true"。

- `list_pending_approvals` → `Vec<PendingApproval>`（actor 形如 `agent:{loop_id}`、snapshot_id hex、changes 按文件分块；持久化在 manager 的 Sqlite，跨执行存活——"跑完再审"）。
- `approve_changes(agent_instance_id, feature_name, paths)`：feature 空串取 manager 默认名；`paths` 非空 → **文件级部分审批**（未列文件留在审批层）；返回 `MergeOutcome{merged, snapshot_id, conflicts: Vec<ConflictView>, conflict_files, message}`，冲突块（0-indexed 行 + base/ours/theirs 三方内容）渲染委托 layertwine 的 `<<<<<<< ours` marker。
- `reject_changes(id, reason)`：审批分区回滚到 baseline（reason 仅日志/诊断）。
- `review_pending_approval`（LLM 评审）：把可见工具面锁死为唯一裁决工具 `REVIEW_VERDICT_TOOL = "approve_changes"`，经 `llm::llm::generate_with_tools_once` **单轮恰好一个 tool call**；失败原因作为 User 消息回灌**有界重试恰好一次**；无 pending / 工具未注册 → 直接 `Unresolved`（不发模型请求）。`ReviewOutcome { Decided(MergeOutcome) | Unresolved(String) }`，契约注释明文：**无法裁决必须保持 pending 交人工，绝不自动 merge/reject**。

### 1.4 provenance.rs（144 行）— 来源查询与编辑会话

`list_partitions`（actor/approval/integrated/staged）、`list_changes_by_actor`（路径子串 + 闭区间时间过滤）、`list_changes_by_path`（跨分区）、`get_actor_workspace`、`diff_actors`/`diff_against_staged`、`file_timeline`（含 rename/move 历史，需 storage 已配置）、`rename_file`（记 move 链接、按 delete-old + edit-new 应用）、**编辑会话组**：`begin_edit_group(label)`/`list_sessions`/`rollback_session`/`undo_edit`/`redo_edit`（`EditSession` 由 manager 持久化，最新优先）、`run_gc(keep_recent_heads)`（`GcRetention` 在内置保护集【branch heads + ancestors + git anchors】之上额外保留的分区头数，返回 `GcStats`）。`EditSession/GcRetention/GcStats` 实为 facade 从 layertwine 直接再导出。

### 1.5 与 agent/workflow 域的关系

- **执行状态 checkpoint** 的创建/恢复在引擎侧与域模块：`agent/agent_checkpoint.rs`（手动 + live 集成）、`workflow/workflow_execution.rs`（create_checkpoint/restore_checkpoint + 引擎自动策略）；协调器/链式类型/delta 回放全在 `wf-checkpoint` 的 coordinator。
- record/file/approval/provenance 是**跨执行、跨 workspace 的记录与文件面**，audit、entity/variable history、CLI 是它们的读方。

## 2. trigger 模块 — TriggerTemplate 域

运行时事实模型：全局 `TriggerTemplate` 注册表（wf-resource registry）+ `wf-workflow::TriggerEventListener` 订阅 EventBus 做通用事件匹配发射；触发执行落 `TriggerExecution` 记录。本模块是该模型的存储/构造/校验面。

### 2.1 template.rs（430 行）— 存储 CRUD + 双写注册（原 template/agent_trigger_template.rs）

`AgentTriggerTemplateFilter{trigger_type, category, tags, enabled, name}`；`query`（存储侧过滤 + tags 内存 any 命中）、`query_by_type/category/tags`、`search`（name/description 子串）、`summaries`、`get/delete`、`export_template`（**按 name**，复用 `template::export_by_name`）/`import_template`（走 save，故同样双写）。

- **`save` = 双写**：组装类型化 `TriggerTemplate`（condition/action 非法 JSON 静默变 None）→ **持久化前作用域预检** `check_trigger_scopes`（竞争报告点名本模板 → Validation 拒绝且 **storage/registry 都不动**；报告只涉预存模板 → 仅 warn）→ `storage.trigger_template.save` + `registries.upsert_trigger_template`（覆盖语义）。HTTP 面双写缺口已修复。
- `delete` → `reference::delete_with_reference_check(Trigger)`（扫 Route/Start/End 节点 trigger 键，id+name 双候选）；**实际删除只删 storage、不清 registry——已知漂移**。
- `trigger_type_of(condition)`：全 crate 唯一分类源——None→`"schedule"`、condition 含非空 `eventType`/`event_type` →`"event"`、否则 `"condition"`。文档注释警示：这是存储形态桶不是来源声明，"schedule" 桶装着永不被事件监听器命中的无条件行（**不存在 scheduler**）。
- `infer_action_type`：`pause_workflow_execution`→pause、`stop/cancel_workflow_execution`→stop、`create_checkpoint`→checkpoint、其余→custom。

### 2.2 builder.rs（284 行）— TriggerTemplateBuilder（原 builder/template.rs 拆出）

消费式链式 builder；`build()` 经 `wf_config::processor::trigger::validate_trigger_template`；`build_with_validation` 附加域校验器结果。**`register(ctx)` 双持久化**：`validate_trigger_registration`（单模板形态 + 合并 scope 竞争）→ 生成 `TriggerTemplateStorageMetadata`（自动 `generate_id`，`trigger_type = trigger_type_of(condition)`）→ storage.save → **严格注册** `registries.register_trigger_template`（重名 → `Conflict`）。与 save 的差异：builder 严格注册 vs save 覆盖注册；builder 把 `create_checkpoint`/`checkpoint_description_template` 写进类型化模板，save 从 metadata 组装时置 None（metadata 类型无这些字段）。语义注释：priority 仅在 best-win 派发模式生效且同 scope 必须互异；dispatch_mode 缺省 unique；`allow_multi_effect` 需配显式 `effect_order`。

### 2.3 execution.rs（201 行）— 触发执行历史（原 entity/trigger_execution.rs）

纯持久面（全部 `&StorageContext`）：save/get/delete/list、by_trigger_name/by_execution/by_workflow、`get_trigger_execution_stats`（HashMap 按 success/failed 计数）、`execution_history(execution_id, trigger_name)`（Reverse(triggered_at) 最新优先）、`cleanup_old_trigger_executions(older_than)`。

### 2.4 active.rs（84 行）— composition 边界激活判定

`is_active` = `enabled.unwrap_or(true)`（**缺省即启用**，与 hook 约定一致）；`resolve_trigger(registries, name)`；`active_trigger_templates` = registry 全量 filter → `sort_winners_deterministically` 定序。当前主要供 API 消费方（wf-runtime 监听器直接读 registry）。

### 2.5 validation.rs（161 行）— `TriggerValidator`

基于共享 `ValidationContext`：`validate`（结构 → action 引用闭合 → scope 竞争，命中报 `"competition"` 类别 error）；`validate_set`（整集注册路径优先，报告 incoming_names 空 → warning、非空 → error）。action 引用闭合按变体：Subworkflow/Workflow → `triggered_workflow_id/workflow_id ∈ ctx.workflow_ids`；Script → `script_name ∈ ctx.script_names`；Agent 两类 → `agent_id ∈ ctx.agent_ids`（trim 非空）+ 可选 model 走 profile 校验。

### 2.6 已删除的旧模型（背景）

旧"每执行/每 loop 触发器"整链已移除且无残留兼容：`TriggerStorageMetadata` 实体与其 CRUD/统计/原子启停（`entity/trigger.rs`）、`AgentConfig.triggers`、`WorkflowDefinition.triggers`、`WorkflowBuilder::add_trigger`、`workflow/execution_trigger.rs`（执行级 enable/disable 命令）。现役启停是模板记录的 `enabled` 布尔（save/register 双写进 registry，监听器按 active 过滤）。

## 3. 关键设计要点

1. **单一 checkpoint 历史**：手动/引擎/hook 触发的状态 checkpoint 共享 coordinator、store、链式 Full/Delta 逻辑；audit/variable-history 都从同一存储读取。
2. **写入门禁**：Delta 缺链字段即拒；trigger 作用域竞争在持久化前拒绝、两存储都不动。
3. **审批安全姿态**：评审失败一律保持 pending；LLM 评审有界（单轮一工具 + 一轮回灌）且工具面锁死。
4. **变更面显式域、只读面可回落**（与 audit 文档呼应）。
5. **注册表驱动运行时**：trigger 执行只认 registry，所有写路径的漂移（delete 不清 registry、node 模板 HTTP 不注册）都会表现为"重启后行为不同"，是已知风险点。
