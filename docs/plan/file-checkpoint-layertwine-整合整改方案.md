# file-checkpoint 与 layertwine 整合整改方案（路径 B：正式引入 layertwine）

> 状态：方案设计（待评审）
> 范围：`crates/wf-checkpoint/`、`crates/wf-storage/`、`crates/wf-types/`、`crates/wf-api/`、`crates/wf-runtime/` 中的 file-checkpoint 相关实现，以及 `crates/layertwine/` 的正式接入
> 关联文档：`docs/plan/rust迁移-分阶段方案.md`、`crates/layertwine/AGENTS.md`
> 分析依据：对 `wf-checkpoint/src/file.rs`、`wf-checkpoint/src/layertwine.rs`、`layertwine/src/{core,engine,layered,checkpoint,storage}/` 的代码审计

## 一、背景与目标

workflow 的原始设计意图包含**多 agent 编排**（fork/join、多路并行、多操作者协同编辑同一工作区）。当前 file-checkpoint 只是"单操作者、无归属、无合并"的字节快照，无法表达：

1. 多个 agent 对同一工作区的并发修改与**三方合并**。
2. 每次修改的**操作者溯源**（哪个 agent / 哪次执行实例改的）。
3. **嵌套子执行实例**（sub-workflow / child execution）之间的修改隔离与归属区分。
4. 通过 script 节点等方式**合并各路文件变更**，并保留可回溯的变更历史。

layertwine 恰好是一套为此设计的文件编辑历史存储层：`AgentInstanceId` 分区（`PartitionType::Agent/Approval`）、`SourceType::Agent` 溯源、`CheckpointMetadata.author`、DAG 分支（`Branch` + 多父 `Checkpoint`）、`merge_texts` 三方合并。本文目标：

1. 以 layertwine 的 `AgentInstanceId` / 分区 / 分层状态机 / 三方合并为核心，重建 file-checkpoint 的权威模型。
2. 消除当前"手写 delta 链 + 把 layertwine 当 Sqlite blob 表"的倒挂与三套割裂。
3. 打通 `FileCheckpointConfig → layertwine 后端 → coordinator → 生产接线` 的完整链路。
4. 让多 agent 合并、溯源、子执行隔离成为一等能力。

## 二、现状分析

### 2.1 三套并存的 "file checkpoint"

| 层 | 位置 | 模型 | 存什么 | 谁在用 |
|----|------|------|--------|--------|
| 存储元数据层 | `wf-types/src/storage/file_checkpoint.rs` + `wf-storage/src/adapter/file_checkpoint.rs` | `FileCheckpointStorageMetadata` | 仅指针：`id / entity_id / file_path / checkpoint_id / size_bytes / compressed / created_at`，不存内容 | `wf-api` CRUD |
| 工作区内容层 | `wf-checkpoint/src/file.rs` | `FileCheckpoint`（`FileState` + full/incremental 手写 delta 链） | 文件内容 + 元数据；`FileContentStore` 存字节 | `FileCheckpointManager`（被 coordinator 调用） |
| layertwine 适配层 | `wf-checkpoint/src/layertwine.rs` | `GitCheckpointAdapter` / `BranchStorageAdapter` | checkpoint blob 当 `SnapshotContent::Structured` 塞 Sqlite | 仅测试 |

关键问题：**三层互不连通**。`FileCheckpointStorageMetadata.workspace_root` 从未被内容层填充；`FileCheckpointManager` 只落 `InMemoryFileCheckpointStorage`；layertwine 的适配层只在测试中构造。

### 2.2 layertwine 的降级复用

layertwine 的完整能力（`core/` 内容寻址 Snapshot/Delta/FileNode、`engine/` 三方合并、`layered/` 六层状态机、`checkpoint/` DAG/分支、`storage/` Sqlite）中，`wf-checkpoint` 只用了 `SqliteStorage` + `SnapshotStore` + `MetadataStore` 三个底层 trait，把字节当 blob 存，手写 SHA-256 delta 链与 layertwine 的 Blake3 内容寻址 Delta/Snapshot/DAG 高度重叠。

### 2.3 具体问题清单

1. **两套 file checkpoint 语义并存且不连通**（见 2.1）。
2. **架构倒挂**：手写 `compute_diff / apply_diff` 与 layertwine 的 `Delta`/`Snapshot`/`merge_texts` 重复，却只把 layertwine 当字节表。
3. **无操作者归属与溯源**：`FileCheckpoint` 没有 author / agent / source 字段。
4. **无多路合并能力**：只有线性 delta 链（`base_checkpoint_id`），无法表达 fork/join 与三方合并。
5. **无子执行实例隔离**：`entity_id` 单一维度，嵌套执行实例无法区分不同操作者。
6. **伪删除 / 无 GC**：`MetadataStore` 无 delete，`delete_checkpoint` 靠清空 value（`layertwine.rs:228-231`），`remove_content` 默认 no-op（`file.rs:119-122`）。
7. **生产接线缺失**：`LayertwineFileContentStore` / `LayertwineGitAdapter` 仅测试构造；`save_file_snapshot` 默认 no-op（`coordinator/base.rs:64-70`）；`FileCheckpointConfig.storage.sqlite` 从未驱动初始化。
8. **错误映射风格不一致**：`file.rs` 用 `CheckpointError::Internal`，`layertwine.rs` 另定义 `LayertwineError`。

## 三、方案决策：以 layertwine 为权威模型

### 3.1 概念映射

将 workflow 的执行模型映射到 layertwine 的分层状态机：

| workflow 概念 | layertwine 概念 | 说明 |
|---------------|-----------------|------|
| 操作者（agent 实例 / 子执行实例） | `AgentInstanceId` | 每个 agent/子执行实例一个稳定 id，作为分区键 |
| 工作区文件集合 | `FileNode` + `Snapshot` | 每个文件一个 `FileNode`（相对路径 + Blake3 基哈希），快照引用 delta 链 |
| 一次修改 | `Delta`（`SourceType::Agent(id)`） | 行级 diff，携带操作者来源，支持溯源 |
| 操作者的独立工作区 | `PartitionType::Agent(id)` 分区 | `layered/agent.rs`：`ensure_agent_partition` + `apply_agent_edit` |
| 待合并/审批的变更 | `PartitionType::Approval(id)` 分区 | `move_agent_to_approval` |
| 合并各路变更（script 节点触发） | `PartitionType::Integrated(name)` + `merge_agent_to_feature` | 三方合并，返回 `MergeResult.conflicts` |
| 最终收敛区 | `PartitionType::Staged` | `merge_features_to_staged` |
| 可回滚/溯源的提交点 | `Checkpoint`（`CheckpointMetadata.author` + DAG 多父） | 多文件 commit，支持 merge commit |
| 分支（多路编排） | `Branch`（head 指针） | `checkpoint/branch.rs` |
| 三方合并 | `engine::merge::merge_texts` | `base/ours/theirs` 冲突检测 |

**关键复用点**：layertwine 的 `SourceType::Agent(AgentInstanceId)`、`PartitionType::Agent/Approval(AgentInstanceId)`、`CheckpointMetadata.author` 天然满足"区分不同操作者作出的修改"；`merge_agent_to_feature` / `merge_features_to_staged` 天然满足"script 节点合并各路文件变更"；`Checkpoint` 的 `parents: Vec<CheckpointId>` + `Branch` 天然满足"嵌套子执行实例的分支与溯源"。

### 3.2 核心决策

1. **权威模型改为 layertwine**：废弃 `wf-checkpoint::file::FileCheckpoint` 手写的 `FileState` + 增量链 + `compute_diff/apply_diff`，改为直接驱动 layertwine 的 `layered/` + `engine/` + `checkpoint/`。`FileCheckpoint` 结构体降级为对 layertwine `Checkpoint` + `Partition` 的轻量投影（projection），用于 API/事件输出。
2. **操作者身份即 `AgentInstanceId`**：`entity_id`（execution id）+ 可选 `agent_id`（agent 实例）组合成 `AgentInstanceId`。子执行实例用 `parent_execution_id` / `root_execution_id` 表达层级，每级独立一个 `AgentInstanceId`，从而在 layertwine 分区层面天然隔离。
3. **保留 `FileContentStore` 概念，但落到 layertwine**：不再新建 Sqlite blob 表，直接使用 layertwine 的 `FileNodeStore` + `SnapshotStore` + `DeltaStore`（内容寻址、INSERT-ONLY、可压缩 zstd）。`LayertwineFileContentStore` 重构为对 layertwine `Snapshot`（`SnapshotContent::FileContent`）的薄封装，而非手写 `wf-file-content:` 前缀索引。
4. **删除/GC 由 layertwine 语义承载**：分区的 `rollback_to` / `rollback_one`（`partition.rs:36-57`）是"指针回退"而非物理删除；物理 GC 走 `git_sync/gc.rs`（已有）或后续独立 GC 阶段，不引入"清空 metadata value"的伪删除。
5. **合并入口**：`FileCheckpointManager` 增加 `merge_entity_changes` 之类的方法，封装 `merge_agent_to_feature` / `merge_features_to_staged`，供 script 节点调用；冲突结果（`MergeResult`）映射到 `CheckpointError` 或事件。

### 3.3 待评审

1. `AgentInstanceId` 的生成规则：直接用 execution id，还是 "execution-id + agent-instance-id" 复合？
2. 是否需要保留"审批"层（Approval）语义，还是 multi-agent 场景直接 agent → integrated？
3. `FileCheckpointStorageMetadata`（wf-types）是删除还是保留为投影？

## 四、分阶段实施

### Stage 1：收敛类型层，确立 AgentInstanceId 为操作者身份

1. 在 `wf-checkpoint` 新增 `agent_id` 映射：定义 `AgentInstanceId` 的生成函数（如 `agent_instance_id(entity_id: &str, actor: Option<&str>) -> AgentInstanceId`），统一 workflow/agent coordinator 的操作者身份来源。
2. 移除/降级 `wf-types::FileCheckpointStorageMetadata`、`wf-storage::adapter::file_checkpoint`、`wf-api/src/workflow/file_checkpoint.rs` 的 CRUD（先 `#[deprecated]` 标记，再按 no-backward-compatible 原则删除）。
3. `wf-storage/src/context.rs`、`entity_impl.rs`、`wf-runtime/src/storage_manager.rs` 中 `file_checkpoint` 引用同步清理或改为 layertwine 投影。
4. `FileCheckpointConfig` 增加 `agent_id` / `feature_name` / 分层策略字段（或新增 `LayertwineConfig`）。

**验收**：`cargo check --workspace` 通过；`FileCheckpointStorageMetadata` 仅剩 deprecated 标记。

### Stage 2：重构 FileCheckpointManager 驱动 layertwine 分层状态机

1. `FileCheckpointManager` 内部持有 `layertwine::layered::StateMachine<SqliteStorage>` + `Arc<SqliteStorage>`，替换 `Option<Arc<dyn FileCheckpointStorageAdapter>>` + `Option<Arc<dyn FileContentStore>>` 的手写组合。
2. `create_checkpoint` / `create_checkpoint_with_content` 改为：
   - 确保 `agent` 分区存在（`ensure_agent_partition`）。
   - 用 `apply_agent_edit` 记录每个文件的新内容（自动 diff + `SourceType::Agent`）。
   - 产出 layertwine `Checkpoint`（`baseline_snapshots` + `metadata.author = agent_id`）。
3. `restore_workspace` / `restore_content` 改为 `transition::reconstruct_text` 重建文件内容后写回磁盘。
4. 新增 `merge` 系列方法（封装 `move_agent_to_approval` + `merge_agent_to_feature` / `merge_features_to_staged`），返回冲突信息。
5. 删除 `wf-checkpoint/src/file.rs` 中手写的 `compute_diff / apply_diff / resolve_chain / delta_chain_length`（由 layertwine `engine` 与 `Partition.history` 取代）。

**验收**：`cargo test -p wf-checkpoint` 通过；手写 delta 链代码删除；`wf-checkpoint` 不再有 `layertwine_to_storage_error` 之类的冗余映射。

### Stage 3：删除/GC 语义

1. 移除 `LayertwineGitAdapter::delete_checkpoint` 的"清空 value"伪删除（`layertwine.rs:228-231`）。
2. 删除用分区指针回退（`rollback_to`）或 `git_sync/gc.rs` 的 GC 承载；为 `FileCheckpointManager` 提供 `discard(entity_id)`（映射 `discard_agent_edit`）。
3. 若需要物理删除，在 layertwine 内部实现引用计数 GC（遍历 `Snapshot.parents`/`Checkpoint.baseline_snapshots`，删除无引用实体），作为独立 crate 能力。

**验收**：集成测试断言 `discard` / GC 后字节可回收；无"清空 value"路径。

### Stage 4：打通生产接线

1. `bootstrap.rs` 依据 `FileCheckpointConfig.storage`（type/db_path）构建 `SqliteStorage` + `StateMachine`，注入 `FileCheckpointManager`。
2. `wf-runtime` / `wf-server` 在构造 `WorkflowCheckpointCoordinator` / `AgentCheckpointCoordinator` 时调用 `with_file_checkpoint_manager`，并把 `agent_id`（操作者身份）随 `CheckpointContext` 传入。
3. `save_file_snapshot` 默认 no-op 保持，但在文档明确：`enabled=true` 时，coordinator 在 persist 后调用 `FileCheckpointManager` 记录操作者文件快照。
4. 暴露 script 节点可调用的合并入口（`merge_entity_changes`），供"合并各路文件变更"场景。

**验收**：`cargo test --workspace` 全量通过；新增 runtime 级集成测试覆盖"多 agent 编辑 → 合并 → 溯源 → 回滚"闭环。

### Stage 5：子执行实例隔离与溯源

1. 子执行实例以 `parent_execution_id` / `root_execution_id`（`wf-types/src/execution/hierarchy.rs`）构造独立 `AgentInstanceId`。
2. 用 layertwine `Branch`（`checkpoint/branch.rs`）为每个子执行实例建分支，父实例合并子实例时用 `Checkpoint` 多父（merge commit）表达。
3. 溯源查询：遍历 `Delta.source`（`SourceType::Agent`）+ `CheckpointMetadata.author` + `Snapshot.source`，暴露 `list_changes_by_actor` 之类的 API。

**验收**：集成测试覆盖嵌套子执行实例的修改隔离、合并与溯源；`Checkpoint` 出现多父 merge commit。

## 五、风险与影响

| 项 | 影响 | 缓解 |
|----|------|------|
| 重构 FileCheckpointManager 破坏现有调用 | `wf-checkpoint` / `wf-workflow` / `wf-agent` 大量测试改动 | 分阶段、保留 `FileCheckpoint` 为投影类型以最小化 API 面改动；每阶段 `cargo test` 验证 |
| layertwine 六层状态机 vs workflow 现有 checkpoint 语义 | 语义对齐成本 | 仅启用 agent → integrated → staged 三条链路，approval/manual 层可裁剪（见 3.3） |
| layertwine 行级 diff 仅适用于文本文件 | 二进制文件无法 diff | 二进制文件走 `SnapshotContent::FileContent` 直接快照（`new_with_content`），不经过 `apply_agent_edit` |
| 删除/GC 语义复杂 | INSERT-ONLY 存储增长 | 分区回退 + 独立 GC 阶段；不引入伪删除 |
| `AgentInstanceId` 复合规则不确定 | 溯源粒度 | 见 3.3 待评审，先以 execution id 为最简粒度 |

## 六、待评审问题（汇总）

1. `AgentInstanceId` 生成规则：execution id 单层，还是 "execution + agent-instance" 复合？
2. 是否裁剪 Approval / Manual 分层，仅保留 agent → integrated → staged？
3. `FileCheckpointStorageMetadata` 删除还是保留为投影？
4. 三方合并冲突的处理策略：失败中断、写入冲突标记、还是走审批？
