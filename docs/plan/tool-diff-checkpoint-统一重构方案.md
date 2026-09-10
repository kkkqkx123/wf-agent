# Tool 副作用、Diff 与 File Checkpoint 统一重构方案

> 本文档合并并取代以下两份历史方案（已删除）：
>
> - `tool-observer-diff-checkpoint-根本重构方案.md`
> - `tool-observer-diff-checkpoint-撤销observer与infra化重构方案.md`

## 一、方案定位

本方案针对 `wf-tools` 工具副作用上报、文件 diff、shell 文件监控和 file checkpoint 的整体关系进行重构。目标不是扩展现有接口，而是重新定义副作用事件、内容差异、文件历史和存储边界。

**前提（已完成，作为既定事实）**：

- `wf-checkpoint` 已提升为 infra 模块，位于 `crates/infra/wf-checkpoint`，不依赖任何 engine crate（依赖链 foundation ← infra ← engine ← app 成立）。
- `wf-runtime` 中 `wf-checkpoint` 为非可选硬依赖，`checkpoint` feature 及全部 `feature = "checkpoint"` 条件编译已移除，checkpoint 代码在任何 feature 组合下都参与编译。
- 撤销 `ToolSideEffectObserver` 机制的决策已定：工具层不再经 observer trait 上报副作用，而是直接持有并调用 infra 层的 `CheckpointSession`。

本方案遵循以下原则：

- 不保留旧 observer 接口、旧 diff 数据结构或旧 checkpoint 兼容路径。
- `layertwine` 不再作为独立 vendor crate 维护，源代码和职责合并进 `wf-checkpoint`。
- 项目只保留一套文件差异算法和一套文件历史存储模型。
- 工具行为事件、文件内容差异、checkpoint 存储 delta 分层，但使用同一套基础 diff 类型。
- shell 只负责声明执行范围和生命周期；文件变化由 checkpoint 的采样器和统一 diff 管线确认。
- 前端、审计、checkpoint、恢复都读取统一的文件变更模型，不再各自定义相似类型。

## 二、现状问题

### Observer 层（已决定撤销）

`ToolSideEffectObserver` 是一层"只有单一生产实现"的纯粹间接层，带来数据重复与跨 crate 重管线，却未换取任何真实解耦。证据均来自当前代码库：

| 事实 | 位置 | 含义 |
|---|---|---|
| 生产环境唯一实现 `AgentCheckpointObserver` | `crates/engine/wf-agent/src/checkpoint_observer.rs:211` `impl ToolSideEffectObserver` | 全仓仅此一个真实实现 |
| 其余 4 处 `impl ToolSideEffectObserver` 均为测试记录器 | `observe.rs:199`、`shell.rs:274`、`session_observe.rs:114`、`filesystem.rs:1237` | observer 主要用于"测试可观测"，非生产多实现 |
| `ToolExecutionContext` 持 observer 字段并提供 `with_observer` | `crates/engine/wf-tools/src/executor/trait_def.rs:27`、`:69` | 每个工具调用上下文都背着这个字段 |
| `wf-agent` 构造并注入 observer | `crates/engine/wf-agent/src/coordinator/lifecycle.rs:487-493` | 适配器在编排层构造 |
| `wf-workflow` 也构造并注入同一 observer | `crates/engine/wf-workflow/src/handler/llm.rs:531-540` | 同一逻辑跨两个 engine crate 重复接线 |
| `wf-api` 作为透传通道 | `crates/app/wf-api/src/llm/tool.rs:50`、`:56`、`:67` `execute_with_observer` | API 仅把 handle 往下传 |
| shell 子系统 7 个文件持有 `Mutex<Option<ToolSideEffectObserverHandle>>` | `shell.rs:265`、`session_observe.rs:38`、`backend_shell.rs:51`、`get_or_create_shell.rs:44`、`execute_in_session.rs:42`、`release_sessions_for_task.rs:40`、`shell_kill.rs:38` | 仅为转发 session 生命周期事件，管线极重 |
| 数据重复：`PreciseFileChange`/`PreciseFileOp` ≈ `file_actor::PreciseFileEvent`/`PreciseFileEventKind` | `observe.rs:30` 与 `wf-checkpoint::file_actor` | observer 的 `notify_precise` 仅做字段重映射后调用 `manager.apply_precise_file_events` |
| `wf-tools` 当前不依赖 `wf-checkpoint` | `crates/engine/wf-tools/Cargo.toml` | observer 正是为避免该依赖而存在的"依赖反转"补丁；`wf-checkpoint` 落位 infra 后 `wf-tools` 本就允许依赖它 |

此外，observer 事件被直接消费后，`AgentCheckpointObserver` 再次从磁盘读取文件。工具层拥有旧内容和新内容时没有把它们传出，造成：

- 前端无法直接展示某次 `edit_file` 或 `apply_patch` 的具体变更。
- 审计系统无法区分小编辑、大范围覆盖和纯 rename。
- checkpoint、provenance 和 UI 可能针对同一对内容重复计算 diff。
- 文件工具和 shell 工具的行为没有统一的关联标识。

### Diff 层

当前同时存在：

- `wf-checkpoint::DiffEngine`
- `wf-checkpoint::provenance` 中的 workspace diff
- `layertwine` 内部 diff
- `WorkspaceChangeCollector` 的 hash-only 变化检测

这些实现的算法、结果类型和调用边界不统一。`wf-checkpoint` 与 `layertwine` 都承担了部分文件历史职责，导致存储 delta、行为 diff 和查询 diff 没有明确的单一权威来源。

### Vendor 层

`crates/vendor/layertwine` 原本计划作为独立包，但该计划已经放弃。继续作为 workspace 中的独立 crate 会保留一层不必要的 API 和依赖边界，也使 checkpoint 无法直接围绕自身的文件历史语义设计存储模型。

## 三、目标架构

```text
crates/infra/wf-checkpoint
  ├─ effect     统一副作用事件模型（ToolEffect，checkpoint 内部类型）
  ├─ diff       唯一文本/二进制差异基础设施（合并原 DiffEngine + layertwine diff）
  ├─ capture    shell/session 前后状态采样（原 AgentCheckpointObserver 逻辑迁入）
  ├─ history    文件内容、删除、rename、快照和 delta
  ├─ partition  actor/workflow/manual/staged 分区
  ├─ merge      三方合并与冲突
  ├─ watcher    外部文件变化捕获
  ├─ query      文件时间线、工作区和 diff 查询
  └─ session    CheckpointSession（原 observer 的注入态封装为会话对象）

crates/engine/wf-tools
  └─ 工具执行时直接持 CheckpointSession，落盘即 record，shell 起止即 begin/end

crates/engine/wf-agent / wf-workflow
  └─ 以 manager + entity_id + parent_execution_id 构造 CheckpointSession 注入工具上下文

crates/app/wf-api / wf-server / CLI / Web
  └─ 读取统一的 FileChangeView / ToolEffectView
```

`wf-checkpoint` 内部吸收原 `layertwine` 的 core、storage、layered、merge、checkpoint 和 diff 能力。对外只暴露 checkpoint 所需的文件历史 API，不再暴露独立的 layertwine 领域名词。

传输通道的关键差异：工具层 → checkpoint 不再经过 trait 事件流路由，而是 `wf-tools` 直接调用具体类型 `CheckpointSession` 的方法（无 trait 对象）；实时事件复用 checkpoint 既有 `CheckpointEventBus` 与查询。

## 四、统一数据模型

### ToolEffect

`ToolEffect` 作为 checkpoint 内部 mutation 日志类型保留，"有序、强顺序约束"的语义由 `CheckpointSession` 方法调用的天然顺序保证。事件至少包含：

- `effect_id`：单次副作用事件的稳定 ID
- `execution_id`
- `tool_id`
- `sequence`
- `timestamp`
- `scope`：文件、目录或 session 标识
- `payload`

payload 分为：

- `FileMutation`
- `ScopeStarted`
- `ScopeFinished`
- `SessionStarted`
- `SessionCommandFinished`
- `SessionFinished`

事件顺序是强约束。`ScopeStarted` 必须先于对应的 `ScopeFinished`，session command 必须位于 session 生命周期内部。事件接收方不再通过缺失字段推测生命周期。

### FileMutation

文件变更事件包含：

- `path`
- `operation`：create、modify、delete、rename
- `from`：rename 的源路径
- `before`：旧内容摘要或可选旧内容引用
- `after`：新内容摘要或可选新内容引用
- `diff`：可选的统一 diff 和统计信息
- `execution_id`
- `effect_id`

内容字段必须支持二进制文件。文本 diff 只对合法 UTF-8 内容生成，二进制文件只保留 hash、大小和内容引用。

### FileDiff

统一 diff 类型由 `wf-checkpoint::diff` 定义，所有消费者共享：

- old/new content hash
- old/new byte length
- added line count
- removed line count
- changed line count
- similarity
- hunks
- unified text
- binary flag

diff 结果携带生成选项，例如 context line 数和文本规范化策略。这样同一对内容的缓存键可以确定为：

```text
(old_hash, new_hash, diff_options)
```

### FileChangeView

查询和 API 层使用 `FileChangeView`，而不是直接暴露内部 delta：

- path
- operation
- source
- actor
- execution_id
- effect_id
- old_hash
- new_hash
- diff
- checkpoint_id
- timestamp

`FileChangeView` 是对外读模型；存储 delta、merge commit 和内部 snapshot 不直接暴露给前端。

## 五、统一 Diff 基础设施

### 归属

diff 实现放在 `wf-checkpoint` 内部的独立 `diff` 模块，而不是继续保留多个公共实现。当前 `wf-checkpoint::DiffEngine` 和 layertwine diff 需要合并为一个实现。

暂不新增独立 `wf-diff` crate。diff 依赖的是文件历史和 checkpoint 内容模型，直接放在 `wf-checkpoint` 可以避免为了抽象而抽象。未来只有在工具层无法通过 `CheckpointSession` 传递已生成结果、且多个基础设施 crate 需要直接计算 diff 时，才拆出 infra crate。

### 唯一算法入口

统一实现必须提供：

- `compare_bytes(old, new)`
- `compare_text(old, new, options)`
- `render_unified(diff)`
- `apply_diff(base, diff)`
- `merge_text(base, ours, theirs)`
- `diff_stats(diff)`

文件内容是否为文本、是否使用行级 delta、是否保存完整快照，全部由该模块和 checkpoint 存储策略共同决定，禁止工具、API 或 layertwine 私自实现另一套规则。

### 二进制和大文件

- 二进制内容不生成文本 hunks。
- 大文件只计算 hash、大小和变化类型，超过配置阈值时不生成 unified diff。
- 删除只保留旧 hash、旧大小和删除操作，不伪造空文本 diff。
- rename 默认复用内容引用；只有内容同时变化时才生成内容 diff。

### 缓存

diff 缓存属于 `wf-checkpoint::diff`，缓存键使用内容 hash 和选项，不使用路径或执行 ID。这样同一内容变化在 checkpoint、API、CLI 和前端查询之间只计算一次。

缓存必须是可丢弃的派生数据，不得成为恢复所需的权威状态。缓存失效后可根据 checkpoint 内容重新生成。

## 六、工具层改造：直接调用 CheckpointSession

### CheckpointSession（infra 公共类型）

在 `crates/infra/wf-checkpoint` 内新增 `session` 模块，提供 `CheckpointSession`（具体类型，无 trait 对象），封装：

- `actor`、`workspace_root`、`behavior`、`entity_id`（来自原 `AgentCheckpointObserver` 字段）。
- 原 observer 的 per-execution 采样状态：`scoped_before`/`session_before`/`session_scope`（`DashMap` 状态），以及 `recent_agent_writes` 的 lease 管理。
- 作用域/session 采样所需底层能力 `manager` 已具备：`resolve_actor`、`workspace_root`、`failure_behavior`、`scan_config`、`apply_precise_file_events`、`apply_workspace_changes`、`recent_agent_writes`、`WorkspaceScanner`、`WorkspaceChangeCollector`。

公开方法（语义对齐原 observer 的 6 个 `notify_*`）：

- `record_file_mutation(path, op, execution_id, before/after)` → 对应 `notify_precise`。
- `begin_scope(scope_dir, execution_id)` / `end_scope(scope_dir, outcome)` → 对应 `notify_scope_begin`/`notify_scope_end`。
- `begin_session(boundary)` / `command_finished(boundary)` / `end_session(boundary)` → 对应 `notify_session_*`。
- 内部保留原 `resolve_shell_scope` 的"工作区交集"规则与 terminated 不提交的约束。

### 文件工具

改造 `filesystem.rs` 的写入路径，使每个文件工具在一次磁盘提交中生成一次 `record_file_mutation`：

- `write_file`：读取旧内容（若存在），写入新内容，计算 create/modify 和可选 diff。
- `edit_file`：复用已经读取的旧内容和生成的新内容，禁止再次读取以计算 diff。
- `apply_diff`：对内存中的旧内容应用所有 block，生成一次文件级 mutation；不为每个 block 产生独立 checkpoint 写入。
- `apply_patch`：先完成整组 patch，按文件聚合 mutation；rename 同时记录路径变化和内容变化。

写盘失败不产生成功事件。部分 patch 的行为必须明确：成功的文件 mutation 立即进入事件流，失败文件不产生事件，事件中携带 patch 的整体 effect_id。

工具返回值只保留面向模型的简短结果；详细 diff 通过 `CheckpointEventBus` 事件和执行事件流传递，避免在返回文本中嵌入不可解析的 diff。

### Shell 工具

shell 相关工具（`shell.rs`、`predefined/shell/*`）将 `Mutex<Option<ToolSideEffectObserverHandle>>` 改为以 `CheckpointSession` 调用 `begin_scope`/`end_scope`；session 注册表 `session_observe.rs` 持有 `CheckpointSession` 而非 observer handle。

shell 工具只上报生命周期边界：

- scope start / scope finish
- session start / session command finish / session finish

它不负责生成文本 diff，也不直接发送 watcher 事件。

scope finish 必须携带：

- success
- terminated
- exit information
- scope path

`terminated=false` 时 checkpoint 不得把结果标记为完整完成。

## 七、Checkpoint 事件处理

`wf-checkpoint` 内由 `capture`/`session` 模块承接原 observer 的语义（不再有"observer 翻译"环节）。

### 文件工具事件

`wf-checkpoint` 收到 `record_file_mutation` 后：

1. 验证绝对路径和 workspace 边界。
2. 校验事件中的 old hash 与当前 actor 分区基线是否一致。
3. 如果一致，直接使用事件中的 new content 或内容引用提交历史。
4. 如果不一致，拒绝静默覆盖，生成冲突事件并要求重新读取当前状态。
5. 写入 actor partition，并登记 watcher 抑制信息。
6. 将同一个 `effect_id` 写入 provenance 记录。

checkpoint 不再通过"仅有路径的事件"盲目重新读取并覆盖。磁盘读取只作为事件没有内容引用时的明确采样路径。

### Shell 事件

`begin_scope` 创建一个采样上下文：

- scope path
- before file index
- before hash
- execution/effect ID
- active lease

`end_scope` 在确认 terminated 后执行：

1. 采集 after file index。
2. 根据 hash 找出新增、修改、删除和 rename 候选。
3. 读取变化文件内容。
4. 使用统一 diff 模块生成内容 diff 或二进制变化摘要。
5. 生成多个 `FileMutation`，但共享原始 shell effect ID。
6. 提交 actor partition 并释放 lease。

长时间运行 session 在每条命令完成时（`command_finished`）创建一个新的采样边界。session 启动本身不提交文件变化。

### Watcher 事件

watcher 只负责捕获外部文件系统变化。它不再直接构造另一套 `FileChangeRecord` 语义，而是转换成统一的 `FileMutationCandidate`。

候选事件经过以下判断：

- 是否处于 shell/session in-flight scope
- 是否命中 agent write registry
- 当前内容 hash 是否匹配 agent 提交
- 是否为确认的删除或 rename

通过判断后才提交到 manual partition。agent 自写、shell 采样和 manual watcher 不能为同一最终状态重复提交。

## 八、Checkpoint 与原 Layertwine 合并

### 目录调整

删除 `crates/vendor/layertwine` 作为独立 workspace crate。其必要源代码迁移到 `crates/infra/wf-checkpoint/src/` 下，按职责组织：

- `storage`：内容寻址、SQLite、仓储接口和可达性管理
- `snapshot`：文件快照、内容引用、父关系
- `partition`：agent、workflow、manual、approval、staged 分区
- `delta`：统一 diff、delta、恢复
- `merge`：三方合并和冲突
- `history`：时间线、来源、执行和 effect 关联

不保留 `layertwine` 名称作为公共概念。原 crate 的内部类型按 checkpoint 的文件历史语义重新命名。

### 删除重复职责

以下能力只保留一份：

- 原 `wf-checkpoint::DiffEngine` 与 layertwine diff 合并
- 原 checkpoint 手写 delta 链删除
- layertwine 独立 checkpoint/branch API 删除或改为 checkpoint 内部实现
- 旧的 `FileCheckpoint` 与 vendor snapshot 投影重新定义
- vendor 自己的 diff view、merge view 和 API 类型删除

### 存储原则

- snapshot 和 delta 是 checkpoint 内部实现细节。
- actor/workflow/manual/staged 是 checkpoint 的来源分区。
- 每次文件 mutation 都记录 effect_id、execution_id 和来源。
- checkpoint 创建是状态索引，不重新复制已经存在的内容。
- 删除使用显式删除状态，不使用空字符串伪造删除。
- rename 保存源路径和目标路径关系，并复用内容引用。

## 九、API、前端和事件流

撤销 observer 不损失实时能力：原 observer 只把数据写进 `manager`，并未承载实时事件流。前端/审计统一消费 checkpoint 的 `CheckpointEventBus` 与 provenance 查询（`FileChangeView`）；`wf-api` 改为 `execute_with_checkpoint(session: Option<CheckpointSession>)`，内部注入工具上下文。

### 执行事件

工具执行事件流新增统一的 `ToolEffectView`，至少支持：

- effect 类型
- 工具和执行标识
- 文件路径
- 操作类型
- diff 摘要
- unified diff（文本且未超阈值时）
- shell scope 结果
- 关联 checkpoint ID

前端展示工具编辑结果时直接消费该事件，不再通过执行结果文本解析 diff。

### Checkpoint 查询

保留三类查询：

- 按 actor/execution 查询文件变化
- 按路径查询时间线
- 比较两个 checkpoint/partition/workspace

所有查询返回 `FileChangeView`。比较查询和工具行为查询使用同一 diff 表示，但 source、effect_id 和 checkpoint_id 可以为空或有值。

### CLI/TUI/Web

前端只负责渲染 `FileDiff` 和 `FileChangeView`，不再自行调用不同 diff 算法。UI 的 context lines、二进制提示和大文件折叠由 API 返回的结果决定。

## 十、删除和重命名范围

本次重构完成后删除：

- `ToolSideEffectObserver` 的旧多方法 trait 及 `crates/engine/wf-tools/src/observe.rs`（trait、`handle`、`PreciseFileChange`、`ScopeOutcome`、`SessionBoundary`、`normalize_observer_path`）
- `AgentCheckpointObserver` 及 `wf-agent` 的 `checkpoint_observer.rs`
- 所有 `with_observer` / `execute_with_observer` 接口
- `PreciseFileChange`、`ScopeOutcome`、`SessionBoundary` 旧公共模型
- `wf-checkpoint::DiffEngine` 的旧独立接口
- layertwine 对外 crate 名称和 workspace 成员
- vendor 独立 diff、merge、checkpoint 公共 API
- 依赖旧 layertwine 类型的 adapter 和转换层
- 仅为兼容旧 checkpoint JSON 形状保留的字段和转换代码

不保留旧类型别名、旧 serde 名称、旧 API 路由或旧数据读取路径。

## 十一、实施顺序

**验收标准**：统一 diff、写入只算一次、恢复正确性、零旧 observer 类型残留。

### 第一阶段：冻结并重建核心模型

在 infra 内的 `wf-checkpoint` 建立：

- 统一 diff、`FileMutation`、`ToolEffect`、`FileChangeView`。
- `CheckpointSession`（`session` 模块）与 `capture` 模块。
- 迁入原 layertwine 的必要存储能力。

删除旧 diff 类型和旧 vendor 公共接口；建立单一内容 hash、文本判定和二进制策略。

验收：checkpoint 内可以独立完成文本 diff、二进制变化、删除、rename、快照和 delta 恢复。

### 第二阶段：重写 wf-tools（直接接线）

- 删除 `observe.rs`；`executor/trait_def.rs` 的 `observer` 字段改为 `checkpoint_session: Option<CheckpointSession>`，`with_observer` 改为 `with_checkpoint_session`。
- 文件/shell 工具直接调用 `CheckpointSession`；`filesystem.rs` 落盘成功后直接 `session.record_file_mutation(...)`，删除 `notify_precise` 调用与 `observe` 相关导入。
- 更新所有执行器调用点；为每次行为生成 effect_id 和 sequence。

验收：文件工具不再通过旧 notify API 上报，edit/apply_patch 不重复读取旧内容计算 diff。

### 第三阶段：接入 checkpoint pipeline

- 将原 `AgentCheckpointObserver` 的 scope/session 采样与 watcher 候选判定迁入 `wf-checkpoint::capture`/`session`，以 `CheckpointSession` 方法暴露。
- 实现文件 mutation 提交、shell scope/session 采样、watcher 候选转换和来源判定。
- 将 effect_id、execution_id 写入文件历史。

验收：文件工具、shell、session、manual watcher 对同一文件不会重复产生最终状态记录。

### 第四阶段：重建 API 和事件流

- `wf-agent/coordinator/lifecycle.rs` 与 `wf-workflow/handler/llm.rs` 的接线点由构造 `AgentCheckpointObserver` 改为构造 `CheckpointSession`（签名等价，参数仍是 `manager`/`entity_id`/`parent_execution_id`）。
- `wf-api` 的 `execute_with_observer` 改为 `execute_with_checkpoint(session: Option<CheckpointSession>)`。
- 重建 checkpoint provenance API；增加 tool effect 查询和实时事件。
- 更新 wf-server、CLI、TUI 和 Web 前端；删除旧 diff view、旧结果解析和旧路由。

验收：前端可以直接显示文件工具 diff、shell 导致的文件变化和 checkpoint 时间线。

### 第五阶段：清理 workspace 和文档

- 删除 `observe.rs`、`AgentCheckpointObserver`、所有 `with_observer` 残留（若前面阶段未清完）。
- 从根 `Cargo.toml` 删除 vendor crate。
- 删除无用依赖、adapter、转换和测试夹具。
- 更新架构文档、API 文档和部署说明。
- 统一错误、日志和序列化命名。

验收：全仓不存在独立 layertwine 依赖、旧 observer 类型或重复 diff 实现。

## 十二、测试与验证

测试围绕行为契约重写，不迁移旧测试名称或旧兼容断言。

**测试替身策略**：现 4 处测试用 `Recording`/`RecordingObserver` 实现 observer 来断言副作用。撤销后改用 `FileCheckpointManager::new_in_memory()` 直接断言 actor 工作区，提升保真度。

### Diff 测试

- 相同内容返回空变化。
- 文本新增、删除、替换和多 hunk。
- 不同换行符策略。
- 二进制内容不生成文本 diff。
- 大文件超过阈值只返回摘要。
- diff 应用后重新计算 hash 与目标一致。
- 三方合并成功、冲突和冲突标记。

### ToolEffect 测试

- edit_file 只读取一次旧内容并生成一个 mutation。
- apply_diff 多 block 聚合为一个文件 mutation。
- apply_patch 多文件事件具有稳定顺序。
- rename 同时记录路径关系和内容变化。
- 写盘失败不产生成功事件。
- scope/session 生命周期顺序非法时直接报错。

### Checkpoint 测试

- old hash 不匹配时拒绝静默覆盖。
- agent 文件事件不会被 manual watcher 重复记录。
- shell 命令失败但已终止时仍采集文件变化。
- 未终止的 shell 不提交完整变化。
- session 每条命令完成后刷新基线。
- 同一 effect 产生的多个文件变化可聚合查询。
- 删除、rename、二进制和文本变化均可恢复。

### 集成测试

- agent 文件编辑 → CheckpointSession → actor history → API diff。
- shell 写文件 → scope sampling → actor history → 前端事件。
- 外部编辑 → watcher → manual history。
- 多 actor 修改同一文件 → 三方合并和冲突查询。
- checkpoint 恢复后重新计算 hash 与历史一致。

## 十三、风险控制

- 大 workspace 的 shell 采样必须受 scope、ignore 和 hash 快路径限制。
- diff 缓存只能作为派生缓存，不能影响恢复正确性。
- 所有内容提交都以 hash 校验和路径边界校验为前置条件。
- 事件处理失败必须返回错误或明确记录失败，不允许静默丢弃。
- watcher、shell sampler 和 precise mutation 必须通过 effect/source/lease 关联，而不是依靠时间窗口猜测来源。
- vendor 合并期间禁止保留两套实现并行运行；迁移完成后删除旧模块和依赖。
- **依赖方向回退**：迁移后必须 grep 校验 `crates/infra/wf-checkpoint/src` 不含 `wf_tools`/`wf_agent`/`wf_workflow`/`wf_api` 导入；若出现，先消解再继续（`wf-checkpoint` 永不回依赖 `wf-tools`，如"按工具类型猜内容"被本方案禁止）。列为回归检查项。
- **测试替身迁移遗漏**：在 in-memory manager 上重建断言前，保证 `observe.rs` 的删除不残留对 `RecordingObserver` 的引用。
- **scope/session 语义等价**：迁入 `capture` 时必须逐条平移 `resolve_shell_scope` 交集规则、`terminated=false` 不提交约束、session 命令完成刷新基线，避免行为漂移。

## 十四、待拍板开放点

1. **`CheckpointSession` 落点形态**：建议作为 `wf-checkpoint` 内的独立 `session` 模块（封装 per-execution 状态，不污染共享 `manager`）；亦可仅给 `FileCheckpointManager` 加 `begin_scope/end_scope/...` 方法。倾向前者。
2. **实时事件通道**：确认 `CheckpointEventBus` + 查询足以支撑 wf-server/CLI/TUI 的工具 diff 流式展示；若需 app 层薄转发器，单列任务。
3. **命名**：公开会话类型建议 `CheckpointSession`（或 `FileCheckpointSession`），与既有 `FileCheckpointManager` 风格一致；`ToolEffect` 作为 checkpoint 内部事件类型名保留。

## 十五、最终结果

撤销 observer、checkpoint 落位 infra 后，最终系统只保留一条清晰链路：

```text
工具行为（wf-tools, engine）
  → CheckpointSession 直接调用（infra）
  → 统一 Diff / FileHistory / Partition（infra: wf-checkpoint）
  → Checkpoint / Merge / Query
  → CheckpointEventBus + API / Event Stream / UI
```

工具层不再携带 observer trait，编排层不再充当"工具↔checkpoint"适配器，checkpoint 不依赖工具类型猜测内容，前端不自行计算差异，shell 不重复生成文本 diff，历史存储也不再与独立 vendor 包分裂。
