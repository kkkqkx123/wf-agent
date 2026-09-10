# 检查点、前端 Diff 与 Layertwine 归属方案

> 定位：承接现状分析，对 `docs/plan/tool-diff-checkpoint-统一重构方案.md` 的部分结论提出修正。
> 该历史方案主张“layertwine 合并进 wf-checkpoint、diff 内置于 wf-checkpoint”，本文不同意这两点，给出渐进替代路径。
> 全文只描述方向与验收，不贴完整代码。

## 一、现状一句话

检查点是双轨：执行状态走结构化快照，文件内容走分区投影，权威存储是 `layertwine`，策略门面是 `wf-checkpoint`。
三路捕获各有分工：文件工具走精确事件，shell 走前后采样，外部变更走异步 watcher，靠 `RecentAgentWrites` 做归因去重。
行级 diff 重复了两套，同算法不同类型：`wf-checkpoint/diff` 与 `layertwine/engine/diff`，另有独一份词级 `word_diff` 只在 layertwine。
前端目前无自研 diff 算法，TUI 只有纯渲染器，这是好消息。

## 二、修改总原则

不推倒重来，不搞大爆炸迁移。按依赖方向收敛，而不是按物理位置合并。
存储语义归 layertwine，归因与策略归 wf-checkpoint，展示归前端。任何一层不私自实现另一层的规则。

## 三、Diff 治理

### 保留一套算法，一个权威来源

短期不新建 crate。删除 `wf-checkpoint/src/diff.rs`，统一复用 `layertwine::engine::diff`。
`wf-checkpoint/provenance` 与 `file` 的查询只做薄封装，把 layertwine 的行级结果转成对外 `FileDiffView`。
词级 `word_diff` 保持为 layertwine 独有能力，正好给前端做行内高亮，不需要在上层重写。

长期只有同时满足下述两条才抽独立 `infra/wf-diff`：多个 infra crate 需要不经 layertwine 直接算 diff，且展示层需要流式 hunk 与统一缓存策略。
抽取时该 crate 必须位于依赖最底层，不依赖 checkpoint 内容模型，只依赖字节与选项，否则会制造循环依赖。
在此之前新建 crate 属于为抽象而抽象。

### 缓存与选项归属

diff 缓存放在 layertwine 的 diff 层，键为内容 hash 加选项，不带路径与执行标识。缓存是可丢弃派生数据，失效后可重算，不得影响恢复正确性。
二进制判定、大文件阈值、换行策略只在一处定义，工具层与 API 层禁止各自定义阈值。

### 前端 diff 展示不重复

前端只渲染，不计算。后端查询与工具事件流返回统一视图，包含路径、操作、来源、新老 hash、统计与 unified 文本，前端按 hunk 折叠染色即可。
文件工具本身不算 diff，只上报变更事件与内容引用，详细 diff 由 checkpoint 管线事后生成并经事件总线下发，避免在工具返回文本里塞不可解析的 diff。
TUI 已有纯渲染器可复用，Web 与 VSCode 只需新增同语义渲染组件，不引入新算法依赖。

## 四、检查点渐进改进

以下均可在现有分层内完成，无需合并 crate。

* 精确事件携带内容 hash。checkpoint 提交前校验事件旧 hash 与 actor 分区基线是否一致，不一致拒绝静默覆盖，转冲突事件。这是当前盲目重读覆盖的主要修复点。
* 每个行为贯穿 effect 标识。文件工具单次落盘只产生一个文件级 mutation，多 block 的 apply_diff 与多文件的 apply_patch 按文件聚合后共享 effect，shell 采样的多文件结果共享原始 shell effect，便于聚合查询。
* shell 采样加 hash 快路径。先比大小与 mtime，命中后再做内容 hash，采样范围严格保持现有工作区交集规则，越界直接放弃，不退化全扫。大工作区需有预算与 ignore 约束。
* watcher 单一驱动。明确手动变更泵为唯一消费者，现有两套驱动入口收敛为其薄封装，避免双驱动竞争同一队列导致的丢事件与重复提交。shell 租约与 agent 写注册继续作为抑制依据，但判定以内容 hash 为主，时间窗口只补竞态。
* hash 口径收敛。对外展示沿用现有十六进制摘要，内部内容寻址沿用 layertwine 的寻址方式，转换只在门面边界做一处，不在各模块散落两套实现。
* 状态 delta 与文本 diff 保持正交。前者是字段级比对，后者是行级算法，不合并，不共用缓存。

## 五、Layertwine 是否应该合并进 Checkpoint

结论：不应该整体合并，应该瘦身分工。理由如下。

支持合并的观点通常是减少一层边界与一套重复类型。这个动机成立，但合并手段不对。
当前两边体量都已过大，合并后形成六万行级单体 crate，违背模块化初衷。重复并不会因物理搬运而消失，只是从跨 crate 重复变成 crate 内重复。

更关键的是职责不同。layertwine 是不可变存储引擎，管内容寻址、分区历史、快照与 delta、合并、垃圾回收与持久化，天然独立可测，也被远端执行器等非 checkpoint 路径直接使用。
wf-checkpoint 是策略与协调层，管 actor 归因、采样租约、watcher 抑制、执行状态 checkpoint、审批与分支策略、对外投影。两者是上下游，不是同一物。

依赖方向也支持保持分离。现有方向是 `wf-checkpoint` 依赖 `layertwine`，符合 foundation、infra、engine、app 的单向链。
若把 diff 抽到上层或把存储炸开到上层，vendor 反向依赖 infra，就会破坏该链。正确的收敛方向是上层复用下层，而不是下层搬进上层。

建议的归属划分：

* layertwine 留在 vendor，但瘦身。保留核心存储、引擎、分层、检查点与回收能力，移除传输形态的 CLI、HTTP、gRPC 与独立展示类型。远端执行能力如仍需保留，应收为独立客户端，不污染存储核心。
* wf-checkpoint 保持门面与策略，不再自建存储与 diff，只做投影、归因、采样与查询。现有门面臃肿问题靠拆分查询与恢复子模块缓解，而不是靠吞掉下层。
* 对外只暴露 checkpoint 语义的领域名词，不再透出 layertwine 内部类型。现有 `wf-api` 直引 layertwine 类型的透传点应收敛到 checkpoint 视图。

只有一种情况可考虑合并：团队明确 layertwine 永不独立发布与复用，且维护人力只够支撑一个 crate。
即便如此，也必须先完成上述瘦身与重命名再搬运，否则只是把两套实现放进同一目录，验收时的零重复目标无法达成。

## 六、实施顺序与验收

先收敛 diff，再修归因，最后动边界。每步可独立上线。

* 收敛 diff：删除上层自研 diff，全量走 layertwine 引擎，保留词级能力给前端。验收为全仓只剩一处行级算法入口，查询结果类型统一。
* 修精确与采样链路：事件携带 hash 并做一致性校验，shell 加快路径，watcher 收敛单驱动。验收为同一最终状态不重复提交，旧 hash 不匹配不静默覆盖。
* 重建前端消费：工具事件流与查询统一返回可渲染视图，前端只做展示。验收为编辑、shell 产物、外部变更均可在界面看到一致 diff，无前端自算。
* 瘦身 layertwine：移除传输与展示类型，收敛对外透出。验收为 `wf-api` 不再直接依赖 vendor 内部类型，workspace 依赖边只剩 checkpoint 到 layertwine。

## 七、分阶段执行记录

以下四阶段已按序实施并验证通过（`cargo clippy` 受影响 crate 全目标干净，
`wf-checkpoint` 385 个单测与全部集成测试通过，`layertwine` 289 个单测通过）。

### 阶段一：diff 收敛到 layertwine（已完成）

删除 `wf-checkpoint/src/diff.rs` 自研 Myers 实现，重写为委托门面，
仅保留二进制判定、内容哈希、统一文本入口、行统计与词级行内高亮。
新增 `layertwine::engine::diff::diff_stat_counts` 作为唯一行计数实现。
`provenance::text_diff` 与 `FileCheckpointManager::unified_diff` 改走该门面，
移除 `wf-checkpoint` 对 `similar` 的依赖，`wf-api` 的重导出同步收敛。
已确认 `compare_bytes` 等旧类型无外部调用者，直接删除。

### 阶段二：采样归因加固（已完成，含一次回退）

原计划给前后采样加 size 加 mtime 快路径，实测发现本机文件系统同 tick
内重写不推进 mtime，快路径会漏检内容变化，已整体回退并记录为禁止项。
实际落地为等价小改：`capture.rs` 租约解除时复用 after 快照哈希，
不再二次读盘哈希，既省一次全量读取又消除中间读盘竞态窗口。

### 阶段三：前端消费统一（已完成）

确认服务端 diff 查询已是透传统一视图的薄适配，无需改动。
新增 `inline_word_diff` 委托入口并经 `wf-api` 透出，给前端行内高亮使用，
前端继续只渲染不计算。

### 阶段四：layertwine 边界收敛（已完成第一步）

`wf-checkpoint` 透出 `EditSession`、`GcRetention`、`GcStats` 重导出，
`wf-api` 改走该门面并移除对 `layertwine` 的直接依赖。
`wf-runtime` 的直接依赖与 layertwine 传输形态瘦身列为后续项，
本次未动，避免扩大爆炸半径。

## 八、后续项执行记录

### R1：wf-runtime 直引收敛（已完成）

`bootstrap_helpers.rs` 的 GC 定时器改走 `wf_checkpoint::GcRetention`，
`wf-runtime` 移除对 `layertwine` 的直接依赖。应用层不再命名 vendor 类型。

### R2：传输形态瘦身（已完成第一步）

核查确认 `remote-layertwine` 特性无人启用，全仓仅 `wf-tools` 自身引用。
删除 `executor/remote.rs` 内约 860 行 layertwine gRPC 执行器与配套测试，
删除特性门与可选依赖，`wf-tools` 不再依赖 `layertwine`。
结论：vendor 现仅被 `wf-checkpoint` 依赖，engine 到 vendor 的边已消除。
layertwine 自身 CLI、HTTP、gRPC 形态是否保留属产品决策，
涉及其自有测试与协议文件，本次未动。

### R3：watcher 单一消费者（已完成）

核查确认旧驱动（`drive_watcher_incremental`、`drive_watcher_batch`、
`create_incremental_checkpoint`）零生产调用者，仅单测引用，
且其语义是把外部变更写入 agent 分区，归因错误。
删除上述三函数与 `WatcherDriveStats`，生产唯一驱动为
`ManualChangeService` 经 `process_manual_changes` 写入 manual 分区，
泵注释同步更新为独占消费契约。`source_capture.rs` 集成测试继续覆盖该路径。

### R4：layertwine 迁移至 infra 与服务模式清理（已完成）

`crates/vendor/layertwine` 经 `git mv` 迁至 `crates/infra/layertwine`，
包名保持 `layertwine`，调用方零改动；根 `Cargo.toml` 与
`wf-checkpoint` 依赖路径同步，`crates/vendor/` 目录消除。
根 `AGENTS.md` 架构树与依赖 DAG 同步（vendor 段删除）。

服务模式同步下线：删除 `api/http`、`api/rpc`（含 proto 与构建脚本）、
`cli`、`runtime`、`main` 二进制、全部传输特性与可选依赖、gRPC/HTTP
集成测试、传输层用户指南；`LayertwineError` 删除 CLI 变体、退出码与
格式化辅助；`ApiService` 保留为进程内门面（测试套件共用底座）。
`tonic`、`prost`、`tonic-build` 无其他使用者，随工作区依赖一并移除。
README 双语重写传输章节为嵌入使用说明，架构文档剔除 CLI 选型行。

核查确认旧驱动（`drive_watcher_incremental`、`drive_watcher_batch`、
`create_incremental_checkpoint`）零生产调用者，仅单测引用，
且其语义是把外部变更写入 agent 分区，归因错误。
删除上述三函数与 `WatcherDriveStats`，生产唯一驱动为
`ManualChangeService` 经 `process_manual_changes` 写入 manual 分区，
泵注释同步更新为独占消费契约。`source_capture.rs` 集成测试继续覆盖该路径。

## 九、与历史方案的分歧点

历史方案要求 layertwine 源码合并进 checkpoint 且 diff 内置于 checkpoint。本文修正为 layertwine 保留并瘦身，diff 收敛到 layertwine，上层删除重复实现。
历史方案倾向大爆炸重写与删除旧类型。本文主张渐进收敛，每阶段保持可运行与可验证。
两者在统一视图、effect 贯穿、前端只渲染这三点一致，可直接沿用。
