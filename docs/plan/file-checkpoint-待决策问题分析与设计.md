# file-checkpoint 待决策问题分析与设计建议

> 状态：分析建议稿（待评审）
> 范围：针对 `docs/plan/file-checkpoint-决策点与备选方案.md` 文末「待考虑问题清单」（1-8）与 `docs/plan/file-checkpoint-layertwine-整合整改方案.md` 3.3 / 六 的遗留问题，给出代码事实核对、备选分析与设计建议
> 依据：对 `wf-checkpoint`、`layertwine`、`wf-sandbox`、`wf-agent`、`wf-workflow`、`wf-types` 相关代码的审计（file:line 见文末附录）
> 约束：本文不推翻已定决策 D1-D6，只在其约束内做细化设计；「推荐」为分析结论，待评审确认

## 〇、结论速览

| # | 待决策问题 | 建议结论 | 决策状态 |
|---|-----------|----------|----------|
| 1 | `AgentInstanceId` 编码格式 | `{kind}:{hierarchy}`，kind ∈ {wf, agent, sub}，嵌套用 `/child:{exec_id}` 追加；前置修复 `trigger.rs` child id 复用 bug | 待评审 |
| 2 | 脚本 VFS 回写与 diff 采集 | **修正前提**：当前 overlay 从未被脚本实际写入；近期用「执行前后文件状态 diff」替代「VFS 回写」，overlay 真实化列为 sandbox 远期增强 | 待评审 |
| 3 | manual 层具体设计 | 哈希注册表 + watcher 事件比对（确定性互斥）；manual/staged 单例分区 ⇒ 一个 DB 对应一个工作区 | 待评审 |
| 4 | approval 层形态 | 双层审批：工具级（前置防护）+ 分层审批（合并前审核）；`ApprovalPolicy` 枚举 auto/llm/manual/none；补 workflow LLM 节点审批缺口 | 待评审 |
| 5 | 合并设计与冲突策略 | 默认冲突标记入文件 + 事件报告；`conflict_behavior` = marker/fail/approval 可配 | 待评审 |
| 6 | 子执行分区/Branch 生命周期 | 默认保留（廉价指针 + INSERT-ONLY）；删除仅显式 `discard_execution`；物理 GC 复用既有孤儿 GC | 待评审 |
| 7 | `FileCheckpointStorageMetadata` 去留 | **删除**（no-backward-compatible），wf-api 改查投影与溯源 API | 已定（Stage 1 路径不变） |
| 8 | 溯源查询 API 形态 | 以 actor 分区为中心：按 actor / 路径 / 时间三维查询 + 工作区状态与差异查询 | 待评审 |

---

## 一、问题 1：`AgentInstanceId` 命名前缀的最终编码格式

### 1.1 代码事实与约束

- `AgentInstanceId` 是 newtype(`String`)，serde 平铺序列化为普通 JSON 字符串，`Display` 直出内部串（`layertwine/src/core/types.rs:5-12`）；SQLite 侧存 TEXT（`deltas.source_data`、`partitions`）。
- 分区 id = `UUIDv5(固定 namespace, name = agent_id 字节)`（`layered/agent.rs:17-20`、`layered/approval.rs:12-15`、`layered/integrated.rs:20`）。即 `AgentInstanceId` 只要**稳定、唯一、可逆解析**即可，无长度/字符硬性限制（UUIDv5 对任意长度输入做哈希）。
- 执行 id 为 UUIDv7 字符串（含连字符，`wf-common/src/id.rs:3-5`），全局唯一；层级深度上限 `MAX_DEPTH = 10`（`wf-core/src/hierarchy/manager.rs:10`）。
- 已定约束（D1）：agent 主执行实例独立分区；workflow 其他节点（LLM/脚本）共用 workflow 级分区；subgraph 独立分区；triggered subworkflow 走独立工作流。
- 已定约束（D5）：嵌套子执行用命名前缀表达层级（如 `root/{root}/child/{child}/...`）；fork-join 场景用 layertwine `Branch`（分支名独立命名，不占用 `AgentInstanceId` 空间）。

### 1.2 备选格式

**格式 A（推荐）：`{kind}:{hierarchy}`**

```
hierarchy := {exec_id} | {hierarchy}/child:{exec_id}
kind       := "wf" | "agent" | "sub"
```

| 场景 | AgentInstanceId 示例 |
|------|----------------------|
| workflow 顶层（LLM/脚本节点共用分区） | `wf:{workflow_exec_id}` |
| agent 顶层（AgentLoop 主实例） | `agent:{loop_exec_id}` |
| workflow 内 subgraph | `wf:{parent_exec_id}/child:{subgraph_exec_id}` |
| agent 嵌套子循环 | `agent:{root_loop_id}/child:{child_loop_id}/child:{grandchild_loop_id}` |

- 最坏长度：10 级 × (36 + 7) ≈ 430 字符，SQLite TEXT 无压力。
- 与 D5 的 `root/{root}/child/{child}` 等价：hierarchy 链即"根到自身"路径，kind 前缀补充分区语义。

**格式 B：纯层级 `root/{root}/child/{child}/...`（决策文档原文示例，无 kind）**

- 优点：严格贴合 D5 字面；缺点：分区语义（wf/agent/sub）无法自描述，查询与审计需另查元数据，字符串歧义（顶层与子级形态不一致）。

**格式 C：每级带 kind 的冗长版（如 `wf:root/{r}/child:sub:{s}`）**

- 优点：语义最全；缺点：长度膨胀、解析复杂，且子级 kind 多数可从父级推演（wf 之下只会是 sub，agent 之下只会是 agent），冗余。

### 1.3 推荐与理由

- **采用格式 A**：kind 前缀使分区语义自描述（溯源查询/审计/API 过滤可按前缀路由）；hierarchy 链满足 D5 且可逆解析；子级 kind 由父级推演，不冗余。
- 字符集白名单 `[A-Za-z0-9:_/-]`，禁止空格与中文（日志、错误信息、UUIDv5 输入的稳定性）。
- 编码/解析集中在 `wf-checkpoint` 单个模块（如 `actor_id.rs`）：`ActorId::new(kind, &[exec_ids])` / `parse` / `kind()` / `hierarchy()` / `parent()`；生成时直接消费 `ExecutionHierarchyManager::to_metadata()`（`wf-core/src/hierarchy/manager.rs:156-165`），**不要**依赖实体上的 `get_root_execution_id()`——该实现目前是 stub（恒返回自身，`wf-workflow/src/entity.rs:220-224`、`wf-agent/src/entity.rs:312-314`）。
- **前置修复（必做）**：`wf-agent/src/trigger.rs:91-101` 用 `child_config.agent_id`（agent **定义** id）充当 child entity id，会导致同一 agent 定义多次触发的子执行**共用同一个 `AgentInstanceId` → 分区混写**。须改为新生成执行 id（与 `executor.rs:131` 的做法对齐）。

### 1.4 关键子问题

- 子执行结束后分区保留与否 → 问题 6。
- fork-join 的 `Branch.name` 建议直接用 feature 名/分支语义名，与 `AgentInstanceId` 空间隔离（D5 例外已定）。

---

## 二、问题 2：脚本 VFS overlay 回写与 diff 采集

### 2.1 代码事实（对决策 2-A 前提的重要修正）

- `OverlayVFS.delta` 是纯内存 `HashMap`，**没有 commit/flush/写回 API**（`wf-sandbox/src/vfs/overlay.rs:50-56`，delta 只在内存插入）。
- 更关键：**当前脚本执行路径实际上从不写 overlay**——
  - shell 策略链 `["static-analyzer", "vfs-gate", "os-hook"]`（`wf-sandbox/src/resolver.rs:23-26`），`os-hook` 直接在宿主执行命令；`vfs-gate` 只调用 `check_read/check_write` 做权限检查，从不写 delta。
  - `SandboxRuntime::execute_named` 在函数内部创建 VFS、返回即丢弃（`wf-sandbox/src/runtime.rs:336-356`）。
  - 脚本节点默认 `vfs: None`（`wf-workflow/src/handler/script.rs:136-155`）。
  - 脚本写盘与文件工具是两套互不相通的机制（`wf-tools/src/filesystem.rs` 直接 `std::fs` 写宿主盘）。
- 结论：决策 2-A 中「回写 VFS overlay 并采集 diff」在当前代码形态下**没有回写对象**。

### 2.2 推荐设计：以「执行前后文件状态 diff」替代「VFS 回写」（近期）

1. **时机**：`ScriptHandler::execute` 在 `execute_named` 返回后立即采集（`script.rs:82-84` 之后、`if !result.success` 之前）；「脚本执行工具」变体在工具 executor 内同样位置挂钩。
2. **范围**：按 `PathPolicy.allowed_write` 前缀匹配 workspace 内路径（与脚本写权限对齐），叠加现有 ignore 规则。
3. **采集**：执行前对该路径集合计算 sha256（复用 `WorkspaceScanner` + `sha256_hex`），执行后重算；新增/修改 → `apply_agent_edit(path, new_content)`（文本走行级 diff）；删除 → 见 2.3。
4. **归属**：`AgentInstanceId` = 触发脚本的节点所属分区（wf 共用分区或 agent 分区，沿用决策点 1 规则）。
5. **二进制/大文件**：文本走 `apply_agent_edit`；二进制走 `Snapshot::new_with_content`（`SnapshotContent::FileContent`）直接快照，不经过行级 diff（整合方案风险表已有此约定）。
6. **失败语义**：采集失败按 `FileCheckpointConfig.failure_behavior`（Warn/Error/Ignore）处理，不阻断脚本节点执行。

### 2.3 关键子问题：文件删除的表示

- `apply_agent_edit` 只接受 `new_content: &str`（`layered/agent.rs:51-58`），layertwine 无文件级删除操作；`LineDiff` 可表达全删（`DiffOp::Delete`），重建后内容为空串。
- **推荐**：近期以「内容为空串 + `FileCheckpoint` 投影标记 deleted」表达删除（恢复时按文件状态列表处理）；layertwine 增加显式文件删除语义列为远期增强，不与现有三方合并语义冲突。

### 2.4 远期（可选）：overlay 真实化

- `SandboxRuntime` 增加 VFS 注入/取回 API（把 VFS 创建移出 `execute_named`，允许外部传入并取回 delta）；shell 执行策略改为经 overlay 读写；执行结束后 flush delta 到磁盘。
- 这是 **sandbox 阶段的独立工作**，file-checkpoint 只消费其结果（flush 后同样走 2.2 的 diff 采集），两者解耦，不阻塞本阶段实施。

---

## 三、问题 3：manual 层具体设计（watcher 与显式写入的互斥协调）

### 3.1 代码事实与约束

- watcher 已具备：notify 递归监听、debounce、`flush()`（add→unlink 抵消）、`scanner.is_ignored` 过滤、`notify_file_change` 手动注入（`wf-checkpoint/src/watcher.rs:13-18, 44-62, 267-300`）。
- 显式路径（`apply_agent_edit`）写 agent 分区，天然与 watcher 消费互斥——互斥问题的本质是 **watcher 无法区分"变更是谁发的"**。
- layertwine 的 manual 分区与 staged 分区是**单例**（固定 UUID，`layered/manual.rs:16-18`、`layered/staged.rs:52`）→ 隐含假设：**一个 layertwine DB 对应一个工作区**。
- 已定约束（D3）：方案 3-C，模型/脚本显式走 agent 层；其余 watcher 捕获归 manual。

### 3.2 推荐设计：哈希注册表 + watcher 事件比对

1. **注册表** `RecentAgentWrites: DashMap<PathBuf, String>`（path → 内容 sha256）：`apply_agent_edit` 成功后登记；容量上限 + 时间窗（如 30s）淘汰。
2. **watcher 处理**：事件 flush 后逐条——ignore 规则过滤 → 读当前文件 sha256 → 与注册表比对：
   - 一致 = 代理自写（已被 `apply_agent_edit` 记录）→ 跳过；
   - 不一致或无记录 = 人工/外部修改 → `apply_manual_edit`（`SourceType::Manual`；Unlink 删除语义同 2.3）。
3. **竞态双保险**：代理写盘后 100ms 窗口内同路径 watcher 事件直接跳过；哈希比对是确定性主判据，时间窗只是兜底（避免事件与登记间的时序竞态）。
4. **CheckpointTiming 联动**：manual 变更**不触发** checkpoint 创建（`CheckpointTiming` 保持现语义，`wf-types/src/checkpoint/base.rs:155-174`）；checkpoint 提交点仍由既有 timing 驱动。manual → staged 的合并是**显式动作**（`merge_manual_to_staged` 或统一 merge 入口），不自动进行。
5. **生命周期**：watcher 由 runtime bootstrap 在 `FileCheckpointConfig.enabled && workspace_root` 时启动（`wf-runtime/src/bootstrap.rs:106, 852` 现有初始化点），随执行生命周期停止。
6. **DB 约束声明**：manual/staged 单例分区 ⇒ `FileCheckpointConfig.storage.db_path` 与 `workspace_root` **一一对应**（一个 DB 一个工作区）；多工作区共库需扩展 layertwine（按 entity 分区），列为远期。当前 `storage.sqlite` 字段天然支持每工作区一个 db_path。

### 3.3 备选（不推荐）

- OS 级来源标记（xattr 等）：跨进程不可靠、不可移植（Linux 下无从知道谁写的），放弃。
- 纯显式 API（方案 3-B）：漏捕 `vim`/IDE/外部进程直改；作为补充入口保留（宿主可调 `notify_file_change` 显式注入，`watcher.rs:203-214`）。

---

## 四、问题 4：approval 层的恢复与形态

### 4.1 代码事实

- 工具级审批（执行前拦截）：`ToolExecutionCoordinator::approve_tool_calls`（`wf-agent/src/coordinator/tool.rs:291-420`），模式：auto（无 handler）/ 策略规则（`ToolApprovalCoordinator` 按风险分级）/ manual（外部 `ToolApprovalHandler`）；**当前无 LLM 审批模式**。
- **缺口**：wf-workflow 的 LLM 节点路径没有任何审批拦截（`wf-workflow/src/handler/llm.rs:500-523` 直接 `execute_tool`），与 agent 路径不对称。
- layertwine 分层审批基建齐备：`move_agent_to_approval`（三方合并，base = approval.history[0]，`layered/agent.rs:132-212`）、`merge_agent_to_feature`（合并后 approval 自动重置回 baseline，`layered/integrated.rs:100-188`）、`reject_approval`（回 baseline，`layered/approval.rs:76-97`）、`list_pending_approvals`（约定 `history.len() > 1` 为待审批，`layered/approval.rs:63-66`）。
- 已定约束（D4）：保留工具级审批基建与分层审批候选，形态后续恢复。

### 4.2 推荐设计：双层审批 + `ApprovalPolicy` 枚举

**分工（不重叠）**：

| 层 | 时机 | 粒度 | 职责 |
|----|------|------|------|
| 工具级（既有） | 工具执行前 | 工具调用级 | 副作用防护（危险工具拦截：shell/网络/系统级） |
| 分层审批（layertwine） | 合并前 | agent 分区级（一批改动） | 内容审核（文件改动整体审） |

**`ApprovalPolicy`：auto / llm / manual / none**

- `none`：不进 approval 层，agent 结束直接 `merge_agent_to_feature`（当前默认，行为不变）。
- `auto`：`move_agent_to_approval` 后立即 `merge_agent_to_feature`。
- `llm`：审批由 workflow 内审批节点/审批工具完成——新增工具 `approve_changes(agent_instance_id, approve, reason)`（LLM 节点调用，内部映射 `merge_agent_to_feature` / `reject_approval` + 可选 `discard_agent_edit`）。
- `manual`：挂起（`history.len() > 1`），宿主 API `list_pending_approvals` / `approve` / `reject`；**跨执行持久化天然满足**（SQLite 持久，执行结束不影响审批分区状态）——这正是"结束后人工审核"的落点。

**触发点**：agent loop 结束（AgentLoop 节点完成后 / agent coordinator 主循环结束）；可配置在每次迭代结束。

**审批粒度**：默认分区级（一个 agent 实例一批审批）；文件级审批需 layertwine 扩展（合并前 per-file diff 展示 + 选择性合并），列为远期。

**补缺口（建议）**：wf-workflow LLM 节点路径接入工具级审批，与 agent 路径对齐；若短期不做，则文件工具在 workflow 场景的关口完全依赖分层审批，需在文档明确该语义差异。

**冲突处理**：`merge_agent_to_feature` 返回 `MergeResult.conflicts` → 按问题 5 的冲突策略。

---

## 五、问题 5：合并设计（下一阶段）

### 5.1 合并模型

- **共同祖先**：agent 分区创建时的初始 snapshot；layertwine 的合并 base 已确定（`move_agent_to_approval` / `merge_agent_to_feature` 均以 approval 的 baseline 为 base，`layered/integrated.rs:110-115` 有注释说明）。**多 agent 顺序合并天然构成三方合并链**：后合并者的 theirs 已包含先前 agent 的变更，base 不变 ⇒ 不同 agent 改不同位置自动合入，同位置冲突检测。
- **fork-join**：并行分支各自独立分区，join 时合并 → `Snapshot::merge`（多父）→ 多父 `Checkpoint`（merge commit）。
- **入口**：`FileCheckpointManager::merge_entity_changes`（封装 `move_agent_to_approval` + `merge_agent_to_feature` / `merge_features_to_staged`），供 script 节点/编排节点调用（整合方案 Stage 4 已定）。
- **二进制文件**：不走三方合并（内容寻址直接快照，同 2.2 与整合方案风险表）。

### 5.2 冲突策略（推荐）

- **默认 `marker`**：冲突区域以 `MergeConflict::to_conflict_marker`（`engine/merge.rs:136-150`）写入文件 + `MergeResult` 报告 + 工作流事件携带冲突信息；**不中断执行**。
- **可配 `conflict_behavior`**：
  - `marker`（默认）：如上；
  - `fail`：有冲突即中断返回错误；
  - `approval`：冲突变更进入审批等待解决。
- **冲突解决**：agent 重写 / 人工编辑 / 显式 resolve；冲突文件在解决前不参与后续自动合并（staged 侧标记 `has_conflicts` 已存在，`Snapshot.has_conflicts` 可支撑）。

---

## 六、问题 6：子执行分区 / Branch 生命周期

### 6.1 推荐

- **默认保留**：layertwine 不可变实体 INSERT-ONLY，分区与 history 只是廉价指针（`core/partition.rs:6-17`）；保留是溯源、审计与"结束后人工审核"（问题 4 manual 模式）的前提。
- **删除仅显式**：提供 `discard_execution(execution_id)`（映射 `discard_agent_edit` + `delete_partition` + 可选物理 GC），由宿主/清理任务调用，不做自动 GC。
- **物理 GC**：复用既有孤儿快照 GC（可达集遍历 snapshot parents / checkpoint baseline_snapshots，`layertwine/src/storage/sqlite/connection.rs:225-240`），作为独立 GC 阶段（整合方案 Stage 3 已定路径）。
- **fork-join Branch**：join 完成后可删分支指针（`delete_branch` 已存在，`sqlite/checkpoint.rs:235-238`）；DAG 数据仍被 checkpoint 引用，删除 head 指针不影响溯源。
- **重放/恢复执行**：新执行实例 = 新 execution id = 新分区；恢复历史状态走 staged/checkpoint 恢复机制，**不写回旧分区**（旧分区只读，保证不可变性）。

---

## 七、问题 7：`FileCheckpointStorageMetadata` 去留

### 7.1 现状

- 指针层：`id / entity_id / file_path / checkpoint_id / size_bytes / compressed / created_at`（`wf-types/src/storage/file_checkpoint.rs`），**不含内容、不含归属、无合并语义**；`workspace_root` 从未被内容层填充（整合方案 2.1 已证）。
- 使用方：`wf-api` CRUD（`wf-api/src/workflow/file_checkpoint.rs`）。

### 7.2 推荐：删除

- 三套割裂实现之一，内容与历史的真相在 layertwine `Checkpoint`/`Partition`；在 no-backward-compatible 原则下直接删除（整合方案 Stage 1 的 `#[deprecated]` → 删除路径不变）。
- wf-api 改查 `wf-checkpoint` 提供的投影与溯源 API（问题 8）；`FileCheckpoint` 轻量投影（核心决策 3.2 已定）承担对外读模型。

---

## 八、问题 8：溯源查询 API 形态

### 8.1 推荐：以 actor 分区为中心，三维查询 + 状态/差异查询

| 维度 | API | 实现 |
|------|-----|------|
| 按 actor | `list_partitions()` / `list_changes_by_actor(actor, path_filter, time_range)` | 遍历分区 history → `DeltaSummary`（file / source / timestamp） |
| 按路径 | `list_changes_by_path(path)` | 跨分区扫描 `Delta.file` |
| 按时间 | 时间窗过滤 | layertwine `time_index` 表已有 |
| 状态 | `get_actor_workspace(actor)` | `transition::reconstruct_text` 重建当前快照 |
| 差异 | `diff_actors(a, b)` / `diff_against_staged(actor)` | 两分区快照内容 diff |

### 8.2 展示形态

- wf-api REST（`/file-checkpoints/...` 的 changes/actors 端点）+ 事件流（ChangeEvent 携带 `DeltaSummary`）。
- `FileCheckpoint` 投影输出保持字段形态（id / timestamp / files / hash 等由 layertwine 状态映射），降低消费方迁移成本。
- **实现位置**：`wf-checkpoint` 新增溯源查询模块（基于 layertwine `Repository` trait，不直接碰 SQL），与 layertwine 适配层重构合并进行。

---

## 九、实施顺序建议

| 阶段 | 内容 | 依赖 |
|------|------|------|
| P0 | `ActorId` 编码模块（问题 1）+ `trigger.rs` child id 修复 + `FileCheckpointStorageMetadata` 删除（问题 7） | 整合方案 Stage 1 |
| P1 | 脚本前后状态 diff 采集（问题 2）+ manual watcher 与哈希注册表（问题 3） | P0（需 ActorId 归属） |
| P2 | `ApprovalPolicy` 与分层审批接线（问题 4）+ `merge_entity_changes` 入口与冲突策略（问题 5） | P1 |
| P3 | 溯源查询 API（问题 8）、生命周期/GC（问题 6）、sandbox overlay 真实化（问题 2 远期） | P2 |

---

## 附录：本文依赖的代码事实清单

| 事实 | 位置 |
|------|------|
| `AgentInstanceId` newtype、平铺序列化 | `layertwine/src/core/types.rs:5-12` |
| agent/approval/integrated 分区 = UUIDv5(agent_id) | `layertwine/src/layered/agent.rs:17-20`、`approval.rs:12-15`、`integrated.rs:20` |
| manual / staged 分区为固定单例 UUID | `layertwine/src/layered/manual.rs:16-18`、`staged.rs:52` |
| `apply_agent_edit` 仅接受 `&str` 内容 | `layertwine/src/layered/agent.rs:51-58` |
| `move_agent_to_approval` 三方合并（base=approval.history[0]） | `layertwine/src/layered/agent.rs:132-212` |
| `merge_agent_to_feature` 合并后 approval 重置回 baseline | `layertwine/src/layered/integrated.rs:100-188`（:182） |
| `reject_approval`、待审批约定 history.len() > 1 | `layertwine/src/layered/approval.rs:63-97` |
| `merge_texts` / `MergeConflict::to_conflict_marker` | `layertwine/src/engine/merge.rs:159, 136-150` |
| `OverlayVFS.delta` 纯内存、无写回 API | `wf-sandbox/src/vfs/overlay.rs:50-56` |
| VFS 在 `execute_named` 内部创建、返回即丢弃；shell 策略链含 os-hook 宿主执行 | `wf-sandbox/src/runtime.rs:336-356`、`resolver.rs:23-26` |
| 脚本节点默认 `vfs: None`；执行钩子位置 | `wf-workflow/src/handler/script.rs:82-84, 136-155` |
| workflow LLM 节点无审批拦截，直接 `execute_tool` | `wf-workflow/src/handler/llm.rs:500-523` |
| 工具级审批（auto/规则/manual，无 llm 模式） | `wf-agent/src/coordinator/tool.rs:291-420` |
| watcher 事件模型（Add/Change/Unlink、debounce、flush、ignore、手动注入） | `wf-checkpoint/src/watcher.rs:13-18, 44-62, 203-214, 267-300` |
| `trigger.rs` 以 agent 定义 id 充当 child entity id | `wf-agent/src/trigger.rs:91-101` |
| `get_root_execution_id` 为 stub（恒返回自身） | `wf-workflow/src/entity.rs:220-224`、`wf-agent/src/entity.rs:312-314` |
| 层级元数据正确来源（root/depth 计算） | `wf-core/src/hierarchy/manager.rs:86-165` |
| 孤儿快照 GC（可达集） | `layertwine/src/storage/sqlite/connection.rs:225-240` |
| `FileCheckpointStorageMetadata` 仅指针字段 | `wf-types/src/storage/file_checkpoint.rs` |
| `FileCheckpointConfig` 字段与默认值 | `wf-types/src/config/file_checkpoint.rs:25-53` |
| bootstrap 中 `enabled` 驱动初始化 | `wf-runtime/src/bootstrap.rs:106, 852` |