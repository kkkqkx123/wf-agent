# file-checkpoint × layertwine 整体集成方案（总纲）

> 状态：**Phase 1-6 已完成**（代码层面全部落地，待 `cargo check` / `cargo test` / `cargo clippy` 全量验证）
> 定位：本文是 **总纲**，汇总既有设计文档，给出整体分阶段修改与集成方案。决策点与细化设计**直接引用已有文档**，本文不重复论证，只做编排与落点映射。
> 引用文档：
> - 《file-checkpoint-决策点与备选方案.md》——已定决策 D1-D6（下文称《决策点》）
> - 《file-checkpoint-待决策问题分析与设计.md》——待决策问题 1-8 的细化分析（下文称《待决策分析》）
> - 《file-checkpoint-layertwine-整合整改方案.md》——路径 B 整合方案 Stage 1-5（下文称《整合方案》）

## 一、目标态架构

```
┌─────────────────────────────────────────────────────────────┐
│ wf-api / 事件流        ←  投影（FileCheckpoint）+ 溯源查询 API │
│   ├─ 溯源 REST：partitions / changes / workspace / diff      │
│   └─ 事件流：CheckpointFileChanged（携带 DeltaSummary）      │
├─────────────────────────────────────────────────────────────┤
│ wf-checkpoint                                               │
│   FileCheckpointManager（权威入口）                           │
│   ├─ ActorId 编码 + actor_index 层级解析（actor_id.rs）       │
│   ├─ 来源接入：文件工具 / 脚本 diff 采集 / manual watcher      │
│   ├─ merge / approval 封装（merge_entity_changes）            │
│   └─ 投影与溯源查询模块（provenance.rs）                      │
├─────────────────────────────────────────────────────────────┤
│ layertwine（权威模型）                                       │
│   layered/（六层状态机：agent/approval/integrated/manual/staged）│
│   engine/（三方合并）  checkpoint/（Checkpoint/Branch）        │
│   Repository trait（SnapshotStore/DeltaStore/PartitionStore…）│
├─────────────────────────────────────────────────────────────┤
│ layertwine SqliteStorage（内容寻址、INSERT-ONLY、WAL）        │
└─────────────────────────────────────────────────────────────┘
```

**写路径**（三类来源 → 分区 delta）：
1. 文件工具（LLM 节点 / agent 循环）→ `apply_agent_edit` → agent/wf 分区（D1 归属）
2. 脚本节点 / 脚本工具 → 执行前后状态 diff → `apply_agent_edit`（决策点 2 / 《待决策分析》问题 2）
3. 人工/外部修改 → watcher + 哈希注册表 → `apply_manual_edit`（决策点 3 / 问题 3）

**读路径**：restore 走 `reconstruct_text`；溯源走分区 history + delta；审批/合并走 layered 流转。

**核心不变式**（源自 layertwine，全局遵守）：
- 不可变实体 INSERT-ONLY，全部变更 = 分区指针推进（`Partition.advance`）；回退仅指针（Iron Law 2）。
- `Snapshot::merge` 约定 `parents[0]` 必须是目标分区当前快照（layertwine core/snapshot 166-176）。
- 一个 layertwine DB 对应一个工作区（manual/staged 单例分区约束，见《待决策分析》3.1、3.2-6）。

---

## 二、决策引用索引

| 决策/问题 | 结论出处 | 落点状态 |
|-----------|----------|---------|
| D1 修改来源归属（agent 独立 / wf 共用 / subgraph / subworkflow） | 《决策点》决策点 1 | Phase 1 ✓（ActorId 编码）、Phase 6 ✓（子执行层级解析） |
| D2 脚本变更归 agent_edit | 《决策点》决策点 2 + 《待决策分析》问题 2（**前提修正**：以状态 diff 替代 VFS 回写） | Phase 3 ✓ |
| D3 manual 混合方案（watcher + 显式） | 《决策点》决策点 3 + 《待决策分析》问题 3（哈希注册表互斥） | Phase 3 ✓ |
| D4 approval 保留（暂停后恢复） | 《决策点》决策点 4 + 《待决策分析》问题 4（双层审批 + ApprovalPolicy） | Phase 5 ✓（auto/llm/manual/none 全接线） |
| D5 子执行命名前缀（fork-join 用 Branch） | 《决策点》决策点 5 + 《待决策分析》问题 1（格式 A 编码） | Phase 1 ✓、Phase 6 ✓（层级解析）；fork-join Branch 完整语义留 Phase 7 |
| D6 合并暂不考虑 → 远期 | 《决策点》D6 + 《待决策分析》问题 5 | 远期（Phase 7）；Phase 5 已提供合并入口与冲突 marker |
| 问题 6 分区/Branch 生命周期 | 《待决策分析》问题 6 | Phase 4 ✓（discard/GC 基础）、Phase 6 ✓（显式删除） |
| 问题 7 `FileCheckpointStorageMetadata` 删除 | 《待决策分析》问题 7（已定） | Phase 1 ✓ |
| 问题 8 溯源查询 API | 《待决策分析》问题 8 | Phase 6 ✓（含 REST 端点与事件流） |
| 待评审：合并冲突策略 | 《待决策分析》问题 5.2 | Phase 5 ✓（marker 默认 / fail / approval） |

---

## 三、分阶段方案（含完成状态）

> 阶段 4 与阶段 3 可部分并行；以下为**当前落地状态**，验收标准逐项对照。

### Phase 1：类型层收敛与操作者身份确立 — ✅ 已完成（提交 `b11c80d1`）

- `wf-checkpoint/src/actor_id.rs`：`ActorId` 编码/解析/层级推演模块，格式 `{kind}:{hierarchy}`（kind ∈ wf/agent/sub，嵌套 `/child:{exec_id}` 追加，字符集白名单 `[A-Za-z0-9:_/-]`，MAX_HIERARCHY_DEPTH=10），含 `from_execution`（消费 `ExecutionHierarchyMetadata`）与完整单元测试。
- **前置修复**：`wf-agent/src/trigger.rs` child 实体 id 改为新生成执行 id，消除同一 agent 定义多次触发的分区混写。
- `wf-types::FileCheckpointStorageMetadata`、`wf-storage/src/adapter/file_checkpoint.rs`、`wf-api/src/workflow/file_checkpoint.rs` CRUD 全部删除，`wf-storage/src/context.rs`、`entity_impl.rs`、`wf-runtime/src/storage_manager.rs`、wf-server checkpoints 端点同步清理。
- `FileCheckpointConfig` 增加分层策略字段：`approval_policy`（none/auto/llm/manual）、`conflict_behavior`（marker/fail/approval）、`manual_watch`。

**验收**：`FileCheckpointStorageMetadata` 及其适配层完全移除；`ActorId` 单元测试覆盖编码/解析/层级推演。

### Phase 2：FileCheckpointManager 引擎重构 — ✅ 已完成（提交 `b11c80d1`）

- `FileCheckpointManager` 内部持有 `layertwine::layered::StateMachine<SqliteStorage>` + `Arc<SqliteStorage>`。
- `create_checkpoint` / `create_checkpoint_with_content`：`ensure_agent_partition` → 逐文件 `apply_agent_edit`（二进制走 `Snapshot::new_with_content`）。
- `restore_workspace` / `restore_content`：`transition::reconstruct_text` 重建后写盘；路径逃逸防护与 ignore 规则保留。
- 手写 `compute_diff` / `apply_diff` / `resolve_chain` / `delta_chain_length` 及其测试删除。
- `FileCheckpoint` / `FileState` 降级为 layertwine `Checkpoint` + `Partition` 的轻量投影；`LayertwineFileContentStore` 重构为 `SnapshotContent::FileContent` 薄封装。
- 低层 merge 封装：`move_agent_to_approval` / `merge_agent_to_feature` / `merge_features_to_staged`（薄包装，返回 `MergeResult` 映射）。

**验收**：手写 delta 链代码删除；投影层字段形态与旧 API 对齐。

### Phase 3：修改来源接入（脚本 + manual）— ✅ 已完成（已暂存，待提交）

- **脚本变更采集**：
  - `wf-workflow` `ScriptHandler::execute` 与 `wf-api` 脚本执行 API 在 `execute_named` 返回后做「执行前后文件状态 diff」（`WorkspaceChangeCollector`，范围 = `PathPolicy.allowed_write` 前缀 + ignore 规则，前后 sha256 比对）。
  - 新增/修改 → `apply_agent_edit`；删除 → 空串 + 投影标记；二进制 → `new_with_content`。失败按 `failure_behavior`，不阻断脚本节点。
- **manual 捕获**：
  - `RecentAgentWrites` 注册表（path → sha256，容量上限 + 时间窗淘汰），`apply_agent_edit` 成功后登记。
  - watcher 事件处理链：ignore 过滤 → 当前 hash 与注册表比对（一致=代理自写跳过；否则 `apply_manual_edit`，Unlink 走删除语义）；100ms 写后窗口双保险。
  - watcher 生命周期接线：`FileCheckpointConfig.enabled && workspace_root && manual_watch` 时随 runtime 启动（`ManualChangeService`）。
- 集成测试 `tests/source_capture.rs`：脚本写/删/二进制归入正确分区、人工改文件被捕获为 manual、代理自写不被误捕。

### Phase 4：生产接线与删除/GC 语义 — ✅ 已完成（已暂存，待提交）

- **接线**：`bootstrap.rs` 依据 `FileCheckpointConfig.storage` 构建 `SqliteStorage` + `StateMachine` 注入 `FileCheckpointManager`，经 `ApiContext.with_file_checkpoint_manager` 注入 workflow/agent 协调器与脚本 handler；`save_file_snapshot` 保持 no-op 语义。
- **删除/GC**：`LayertwineGitAdapter::delete_checkpoint` 改为真实指针删除（删除 metadata 索引行，内容留待物理 GC）；`FileCheckpointManager::discard_execution`（映射 `discard_agent_edit` + `delete_partition`）；物理 GC 复用既有孤儿快照 GC 作为独立能力。
- 错误映射统一到 `CheckpointError`（`map_layertwine_error`）。

### Phase 5：审批层与合并入口 — ✅ 已完成（本次会话补全）

- **双层审批分工确立**：工具级（执行前副作用防护，`ToolApprovalHandler`）与分层审批（合并前内容审核）共存，不重复拦截。
- **`ApprovalPolicy` 触发点接线**（本次新增）：
  - `FileCheckpointManager` 持有 `approval_policy` / `conflict_behavior`（来自 config）。
  - `on_agent_complete(entity_id)`：none=no-op；auto=`merge_entity_changes`（默认 feature `exec-{entity_id}`）；llm/manual=`move_agent_to_approval` 挂起。
  - agent loop 结束两处路径（正常完成 / 达到最大迭代）调用 `AgentCheckpointIntegration::on_agent_complete`（best-effort，失败仅告警）。
- **`approve_changes` 审批工具**（本次新增）：`wf-runtime/src/approval_tool.rs` 注册 `approve_changes(agent_instance_id, approve, reason)` stateless 工具；approve=`approve_pending`（按配置 conflict_behavior 合并），reject=`reject_changes`。仅当 file checkpoint manager 附着时注册。
- **宿主 API**（本次新增）：
  - `wf-api::workflow::file_approval`：`list_pending_approvals` / `approve_changes` / `reject_changes`。
  - `wf-server` 端点：`GET /api/v1/file-checkpoint/approvals/pending`、`POST /api/v1/file-checkpoint/approvals/{id}/approve|reject`。Sqlite 持久化承载"结束后人工审核"跨执行。
- **合并入口**：`merge_entity_changes` 完整封装；冲突策略 marker（默认，`to_conflict_marker` 写入文件 + `CheckpointMergeConflicted` 事件）/ fail / approval。
- **wf-workflow LLM 节点工具级审批拦截**（本次新增）：
  - `ToolApprovalHandler` trait 与 `ToolApprovalRequest` / `ToolApprovalResult` 移至 `wf-execution-shared::approval`（wf-agent 重导出保持兼容）。
  - `ExecutorContext` / `NodeExecutionContext` 增加 `tool_approval_handler` / `tool_approval_options` 字段与 `with_tool_approval`；`build_node_context` 与 subgraph / fork-join / trigger / agent_loop 子执行链路全量透传。
  - `llm.rs execute_tool_call` 拦截：外部 handler 决策 → 否则 `ToolApprovalCoordinator` 策略引擎 → 否则自动放行；拒绝返回 Tool 错误消息。
  - 宿主适配器 `wf-api::workflow::tool_approval_handler::InteractionApprovalHandler`（基于 `request_user_approval` 持久化交互流），宿主按执行构造并附着（默认不启用，保持既有行为）。

### Phase 6：子执行隔离与溯源 API — ✅ 已完成（主体提交 `eb0f1c12`，接线与展示本次会话补全）

- **子执行隔离**（本次补全接线）：
  - `FileCheckpointManager` 增加 `actor_index`（entity_id → ActorId）与 `resolve_actor(entity_id, parent_execution_id)`：父已在索引中时编码为 `parent.child(self)`；`actor_id_for` / `actor_id_for_child` 优先返回索引中的层级解析结果。
  - `AgentCheckpointCoordinator` / `WorkflowCheckpointCoordinator` 增加 `prepare_with_parent`；wf-agent / wf-workflow 集成层从实体 `parent_execution_id()` 传入，子 agent（trigger 触发）因此获得 `agent:{parent}/child:{child}` 隔离分区。
  - 脚本采集路径改用 `resolve_actor`（含父执行链）。
  - fork-join 并行分支的 layertwine `Branch` 表达：layertwine 侧 `merge_features_to_staged` 已按 feature 建 Branch 指针；wf 侧完整 Branch 生命周期与合并语义留 **Phase 7 远期**（见下表）。
- **溯源查询 API**（`provenance.rs`，主体提交 + 本次补全）：
  - `list_partitions` / `list_changes_by_actor`（path 子串 + 时间窗过滤）/ `list_changes_by_path`（**本次补全时间窗过滤**）/ `get_actor_workspace`（`reconstruct_text`）/ `diff_actors` / `diff_against_staged`；实现基于 layertwine 存储 API，不直接碰 SQL。
  - 展示层（本次新增）：`wf-api::workflow::file_provenance` + `wf-server` 端点（`GET /api/v1/file-checkpoint/partitions`、`changes/actor|path/{id}?path=&start=&end=`、`workspace/{id}`、`diff/actors/{a}/{b}`、`diff/staged/{id}`）。
  - 事件流（本次新增）：`CheckpointEvent::FileChanged` 携带完整 `DeltaSummary`；`wf-runtime/src/checkpoint_event_bridge.rs` 桥接到共享 EventBus（`CheckpointFileChanged` / `CheckpointMergeConflicted`，metadata 含 file/source/timestamp/snapshot_id/hash），runtime 启停随生命周期。
- **生命周期**：分区默认保留（INSERT-ONLY 廉价指针）；删除仅显式（`discard_execution`）；重放/恢复用新 execution id 产生新分区，旧分区只读。

### Phase 7：远期（未排期，独立阶段）

| 项 | 说明 | 出处 |
|----|------|------|
| 三方合并完整语义 | 多 agent 并发合并链冲突处理、fork-join Branch 生命周期与合并、冲突解决流程 | 《决策点》D6、《待决策分析》问题 5 |
| sandbox overlay 真实化 | VFS 注入/取回 API、shell 策略经 overlay 读写、flush 后 diff | 《待决策分析》2.4 |
| 多工作区共库 | manual/staged 单例分区按 entity 拆分（layertwine 扩展） | 《待决策分析》3.2-6 |
| 文件级审批 | 合并前 per-file diff 展示 + 选择性合并（layertwine 扩展） | 《待决策分析》4.2 |
| layertwine 显式文件删除语义 | 替代"空串 + 投影标记" | 《待决策分析》2.3 |
| 物理 GC 独立阶段 | 引用计数/可达集 GC 的产品化（保留期配置） | 《整合方案》Stage 3 |
| 深层 ActorId 链 | `ExecutionHierarchyMetadata` 仅含根与直接父，三层以上链需完整链数据源（`ActorId::new`） | 《待决策分析》1.3 |
| 宿主默认启用工具级审批 | `InteractionApprovalHandler` 目前为 opt-in，默认保持 auto-approve | 本方案 Phase 5 |

---

## 四、风险与依赖

| 风险 | 影响 | 缓解 |
|------|------|------|
| FileCheckpointManager 重构破坏既有调用 | wf-checkpoint / wf-workflow / wf-agent 测试面大 | 分阶段 + 投影保持字段形态；每阶段 `cargo test` 验证 |
| `trigger.rs` child id 复用导致分区混写 | 溯源/合并数据错乱 | **Phase 1 前置修复已落地** |
| manual/staged 单例分区与多工作区冲突 | 跨工作区数据串台 | 约束：一个 DB 一个工作区（config 校验 db_path 与 workspace_root 对应） |
| 脚本采集依赖文件系统轮询 hash | 大工作区开销 | 范围收窄到 `allowed_write` 前缀集合；失败按 failure_behavior |
| 审批粒度/双层职责边界 | 重复拦截或漏审 | 分工明确：工具级=执行前、分层=合并前；`InteractionApprovalHandler` 默认 opt-in |
| 合并冲突影响执行流 | 冲突文件阻塞后续合并 | 默认 marker 不中断；`has_conflicts` 标记支撑 |
| 本轮改动尚未编译验证 | 潜在类型/路径错误 | 合并前必须通过 `cargo check --workspace` / `cargo clippy --all-targets --all-features` / `cargo test --workspace` |

## 五、总体验收

- 全局验收：`cargo clippy --all-targets --all-features`、`cargo test --workspace` 通过。
- 端到端闭环（Phase 4 起）：多 agent / workflow 编辑 → 分层归属 → 审批（可选，四种策略）→ 合并 → 溯源（REST + 事件流）→ 回滚。
- 代码规范：不引入 unsafe / unwrap；文档语言中文、代码语言英文（遵循 AGENTS.md）。

## 六、实施记录

| 提交/状态 | 内容 |
|-----------|------|
| `b11c80d1` file-checkpoint phase1-2 | Phase 1 + Phase 2 全部改动 |
| `eb0f1c12` file-checkpoint phase6 | Phase 6 主体（provenance、approval 底层、script_capture、recent_agent_writes、事件类型） |
| 已暂存未提交 | Phase 3（脚本/manual 采集、source_capture 测试）+ Phase 4（bootstrap 接线、上下文注入） |
| 未暂存（本次会话） | Phase 5 全接线（策略触发、approve_changes 工具、宿主 API、llm.rs 拦截、trait 迁移）+ Phase 6 补全（ActorId 层级、溯源 REST、DeltaSummary 事件桥、时间窗过滤） |
