# Tool Observer、Diff 与 File Checkpoint 根本重构方案

## 一、方案定位

本方案针对当前 `wf-tools` observer、文件 diff、shell 文件监控和 file checkpoint 的整体关系进行重构。目标不是扩展现有接口，而是重新定义副作用事件、内容差异、文件历史和存储边界。

本方案遵循以下前提：

- 不保留旧 observer 接口、旧 diff 数据结构或旧 checkpoint 兼容路径。
- `layertwine` 不再作为独立 vendor crate 维护，源代码和职责合并进 `wf-checkpoint`。
- 项目只保留一套文件差异算法和一套文件历史存储模型。
- 工具行为事件、文件内容差异、checkpoint 存储 delta 分层，但使用同一套基础 diff 类型。
- shell 只负责声明执行范围和生命周期；文件变化由 checkpoint 的采样器和统一 diff 管线确认。
- 前端、审计、checkpoint、恢复都读取统一的文件变更模型，不再各自定义相似类型。

## 二、现状问题

### Observer 层

`wf-tools::ToolSideEffectObserver` 目前通过多个方法表达 `PreciseFileChange`、`ScopeOutcome` 和 `SessionBoundary`。这些类型分别描述单文件操作、shell 作用域和 session 生命周期，无法承载统一的行为事件，也没有文件内容、旧新 hash 或 diff 信息。

observer 事件被直接消费后，`AgentCheckpointObserver` 再次从磁盘读取文件。工具层拥有旧内容和新内容时没有把它们传出，造成以下问题：

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
wf-tools
  └─ ToolEffect 事件生产者
       └─ 只知道工具行为、路径、执行范围和可选内容变更

wf-checkpoint
  ├─ effect     统一副作用事件模型
  ├─ diff       唯一文本/二进制差异基础设施
  ├─ capture    shell/session 前后状态采样
  ├─ history    文件内容、删除、rename、快照和 delta
  ├─ partition  actor/workflow/manual/staged 分区
  ├─ merge      三方合并与冲突
  ├─ watcher    外部文件变化捕获
  └─ query      文件时间线、工作区和 diff 查询

wf-agent / wf-workflow
  └─ 将 ToolEffect 路由到 checkpoint、事件总线和执行记录

wf-api / wf-server / CLI / Web
  └─ 读取统一的 FileChangeView 和 ToolEffectView
```

`wf-checkpoint` 内部吸收原 `layertwine` 的 core、storage、layered、merge、checkpoint 和 diff 能力。对外只暴露 checkpoint 所需的文件历史 API，不再暴露独立的 layertwine 领域名词。

## 四、统一数据模型

### ToolEffect

`wf-tools` 不再暴露多个 notify 方法，而是生产一个有序的工具行为事件流。事件至少包含：

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

暂不新增独立 `wf-diff` crate。diff 依赖的是文件历史和 checkpoint 内容模型，直接放在 `wf-checkpoint` 可以避免为了抽象而抽象。未来只有在 `wf-tools` 无法通过 observer 传递已生成结果、且多个基础设施 crate 需要直接计算 diff 时，才拆出 infra crate。

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

## 六、工具层改造

### 文件工具

重写 `filesystem.rs` 的写入路径，使每个文件工具在一次磁盘提交中生成一个 `FileMutation`：

- `write_file`：读取旧内容（若存在），写入新内容，计算 create/modify 和可选 diff。
- `edit_file`：复用已经读取的旧内容和生成的新内容，禁止再次读取以计算 diff。
- `apply_diff`：对内存中的旧内容应用所有 block，生成一次文件级 mutation；不为每个 block 产生独立 checkpoint 写入。
- `apply_patch`：先完成整组 patch，按文件聚合 mutation；rename 同时记录路径变化和内容变化。

写盘失败不产生成功事件。部分 patch 的行为必须明确：成功的文件 mutation 立即进入事件流，失败文件不产生事件，事件中携带 patch 的整体 effect_id。

工具返回值只保留面向模型的简短结果；详细 diff 通过 ToolEffect 事件和执行事件流传递，避免在返回文本中嵌入不可解析的 diff。

### Shell 工具

shell 工具只发送：

- scope start
- scope finish
- session start
- session command finish
- session finish

它不负责生成文本 diff，也不直接发送 watcher 事件。

scope finish 必须携带：

- success
- terminated
- exit information
- scope path

`terminated=false` 时 checkpoint 不得把结果标记为完整完成。

## 七、Checkpoint 事件处理

### 文件工具事件

`wf-checkpoint` 收到带有 `FileMutation` 的事件后：

1. 验证绝对路径和 workspace 边界。
2. 校验事件中的 old hash 与当前 actor 分区基线是否一致。
3. 如果一致，直接使用事件中的 new content 或内容引用提交历史。
4. 如果不一致，拒绝静默覆盖，生成冲突事件并要求重新读取当前状态。
5. 写入 actor partition，并登记 watcher 抑制信息。
6. 将同一个 `effect_id` 写入 provenance 记录。

checkpoint 不再通过“仅有路径的事件”盲目重新读取并覆盖。磁盘读取只作为事件没有内容引用时的明确采样路径。

### Shell 事件

`ScopeStarted` 创建一个采样上下文：

- scope path
- before file index
- before hash
- execution/effect ID
- active lease

`ScopeFinished` 在确认 terminated 后执行：

1. 采集 after file index。
2. 根据 hash 找出新增、修改、删除和 rename 候选。
3. 读取变化文件内容。
4. 使用统一 diff 模块生成内容 diff 或二进制变化摘要。
5. 生成多个 `FileMutation`，但共享原始 shell effect ID。
6. 提交 actor partition 并释放 lease。

长时间运行 session 在每条命令完成时创建一个新的采样边界。session 启动本身不提交文件变化。

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

删除 `crates/vendor/layertwine` 作为独立 workspace crate。其必要源代码迁移到 `crates/engine/wf-checkpoint/src/` 下，按职责组织：

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

- `ToolSideEffectObserver` 的旧多方法 trait
- `PreciseFileChange`、`ScopeOutcome`、`SessionBoundary` 旧公共模型
- `wf-checkpoint::DiffEngine` 的旧独立接口
- layertwine 对外 crate 名称和 workspace 成员
- vendor 独立 diff、merge、checkpoint 公共 API
- 依赖旧 layertwine 类型的 adapter 和转换层
- 仅为兼容旧 checkpoint JSON 形状保留的字段和转换代码

不保留旧类型别名、旧 serde 名称、旧 API 路由或旧数据读取路径。

## 十一、实施顺序

### 第一阶段：冻结并重建核心模型

- 在 `wf-checkpoint` 内建立统一 diff、FileMutation、ToolEffect 和 FileChangeView。
- 将原 layertwine 的必要存储能力迁入 checkpoint。
- 删除旧 diff 类型和旧 vendor 公共接口。
- 建立单一内容 hash、文本判定和二进制策略。

验收：checkpoint 内可以独立完成文本 diff、二进制变化、删除、rename、快照和 delta 恢复。

### 第二阶段：重写 wf-tools observer

- 重写文件工具的 mutation 聚合和内容复用。
- 重写 shell/session 生命周期事件。
- 更新 ToolExecutionContext 和所有执行器调用点。
- 为每次行为生成 effect_id 和 sequence。

验收：文件工具不再通过旧 notify API 上报，edit/apply_patch 不重复读取旧内容计算 diff。

### 第三阶段：接入 checkpoint effect pipeline

- 实现文件 mutation 提交。
- 实现 shell scope/session 采样。
- 实现 watcher 候选转换和来源判定。
- 将 effect_id、execution_id 写入文件历史。

验收：文件工具、shell、session、manual watcher 对同一文件不会重复产生最终状态记录。

### 第四阶段：重建 API 和事件流

- 重建 checkpoint provenance API。
- 增加 tool effect 查询和实时事件。
- 更新 wf-server、CLI、TUI 和 Web 前端。
- 删除旧 diff view、旧结果解析和旧路由。

验收：前端可以直接显示文件工具 diff、shell 导致的文件变化和 checkpoint 时间线。

### 第五阶段：清理 workspace 和文档

- 从根 `Cargo.toml` 删除 vendor crate。
- 删除无用依赖、adapter、转换和测试夹具。
- 更新架构文档、API 文档和部署说明。
- 统一错误、日志和序列化命名。

验收：全仓不存在独立 layertwine 依赖、旧 observer 类型或重复 diff 实现。

## 十二、测试与验证

测试围绕行为契约重写，不迁移旧测试名称或旧兼容断言。

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

- agent 文件编辑 → ToolEffect → actor history → API diff。
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

## 十四、最终结果

最终系统只保留一条清晰链路：

```text
工具行为
  → ToolEffect
  → FileMutation / ScopeSample
  → 统一 Diff
  → File History / Partition
  → Checkpoint / Merge / Query
  → API / Event Stream / UI
```

工具层不负责 checkpoint 存储，checkpoint 不依赖工具类型猜测内容，前端不自行计算差异，shell 不重复生成文本 diff，历史存储也不再与独立 vendor 包分裂。
