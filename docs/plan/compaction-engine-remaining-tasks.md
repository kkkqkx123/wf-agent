# 压缩引擎替换：剩余任务细化方案

本文是 `docs/plan/compaction-engine-replacement-design.md` §13.3 的落地细化，覆盖四项待续任务。分析基线 `496b86cb`（已 pull，工作区干净）。遵循 `AGENTS.md`：文档中文、代码英文、禁 `mod.rs`、无向后兼容、动态派发已登记。所有现状声明均附 `file:line`，方案给出可直接落地的改动点与代码形态，开放点集中在 §11。

## 0. 结论速览

* 四项任务按依赖排序：**Replace 写回链 → checkpoint engine_id → 可观测/示例引擎 → `/compact` 命令面**。前两项可自包含完成；命令面跨 crate 最多、风险最高，放在写回链接通之后。
* Replace 写回链的最小闭环不新增事件类型：宿主拿到 `CompactionOutcome::Replace` 后，用 `validate_replacement`（`compaction/engine.rs:120`）复算 `ShrinkProof`，再在会话上做一次受版本约束的区间替换；归档引用随 outcome 传入，持久化由会话快照承接。
* `/compact` 命令面必须走 HTTP API：命令层只持 `client + active_agent_loop_id`，不持 `AgentLoopEntity`（`crates/app/cli/wf-mini/src/session/commands.rs:10`）；服务端可经 `ctx.agent_loop(id)` 取实体（`crates/app/wf-api/src/infra/context.rs:542`）。
* checkpoint 恢复当前**不走快照里的引擎配置**：`coordinator/lifecycle.rs:478` 取配置后由 `run.rs:237-256` 用宿主 `self.compaction_limits` 重建引擎，故需把引擎 id/config 写入 `AgentLoopConfig` 并让恢复路径优先使用。
* 可观测复用现有 `ProgressSink`（`engine.rs:224`）与 metrics collector（`wf-metrics/src/collectors/compaction.rs:11`），不新造事件总线类型；示例引擎以 detached workspace crate 形式放 `crates/infra/wf-plugin/examples/`。

## 1. 事实核验（基线 496b86cb）

| # | 事实 | 证据 |
|---|---|---|
| F1 | `WritebackOp` 仅有 `Append`，`from_operation_name` 只识别 append | `crates/engine/wf-execution-shared/src/context_store.rs:16-31` |
| F2 | `apply_versioned_writeback` 的 match 只有一臂，且会话历史是 append-only 投影 | `crates/engine/wf-agent/src/conversation_compression.rs:243-265`；`conversation_session.rs:33-36` |
| F3 | `CompactionOutcome::Replace` 结构含 `start_seq/end_seq/summary/archive_ref/proof_hint`；宿主复算忽略自报 | `compaction/engine.rs:171-196`；`recompute_shrink_proof:102-116`；`validate_replacement:120-150` |
| F4 | 自动结算路径遇到 `Replace` 仅告警并跳过，未消费 | `crates/engine/wf-agent/src/coordinator/iteration/compression.rs:451-459` |
| F5 | `AgentLoopEntity::compact_now` 只返回 outcome，调用方未消费 | `crates/engine/wf-agent/src/entity.rs:184-218` |
| F6 | 会话提供 seq 与只读快照，但无按 seq 区间破坏式替换方法 | `conversation_session/history.rs:105,146,153,160,173` |
| F7 | 命令层不持 entity；服务端可取；`/recap` 是同类异步 side-call 范例 | `wf-mini/src/session/commands.rs:10`；`wf-api/src/infra/context.rs:542`；`wf-server/src/api/recap.rs:50,113` |
| F8 | `AgentLoopConfig` 无引擎字段，快照在 `coordinator.rs:918` 序列化 | `wf-types/src/agent_execution/loop_config.rs:64-120`；`wf-agent/src/checkpoint/coordinator.rs:918` |
| F9 | 恢复时引擎用宿主 limits 重建，非快照配置 | `coordinator/lifecycle.rs:478,546`；`lifecycle/run.rs:237-256` |
| F10 | 工厂构建 API 已支持私有 config；未知引擎启动期报错 | `compaction/registry.rs:26-49,78-89` |
| F11 | metrics 采集器已有标签维度（mode/reason），ProgressSink 无真实消费方 | `wf-metrics/src/collectors/compaction.rs:11-64`；`engine.rs:224-233` |
| F12 | 压缩事件仅 Requested/Completed/Failed/Discarded；`EventType` 无 start/end | `token_events/compression.rs:118,251,295,323`；`wf-types/src/events/base.rs:32-43` |
| F13 | native 示例插件目录规律已存在；工厂 trait 可直接注册 | `crates/infra/wf-plugin/examples/rag-advisory-demo/`；`engine.rs:279-284`；`registry.rs:26` |

## 2. 目标与非目标

* **目标**：四项待续任务在基线 496b86cb 上落地，`cargo check --workspace` 通过，相关 crate 单测全绿；Replace 具备宿主复算硬闸门与版本约束；实例引擎跨 checkpoint 恢复唯一；`/compact` 至少 CLI 与 API 贯通；可观测与示例引擎可替换 builtin。
* **非目标**：不改 Weave/DSH 对标清单；不内置缓存前缀/分页接续；不开 Lua/Wasm 压缩通道；不为模型主动申请压缩开专用工具。
* **务实边界**：写回链的「archive → hash verify → persist → publish」在单进程会话内以「归档引用登记 → ShrinkProof 复算 → 会话快照持久化 → 完成事件发布」落地，不引入新的分布式存储协议。

## 3. 任务一：破坏式 Replace 完整写回链

### 3.1 现状与差距

F3/F4/F5/F6：契约与硬闸门已就位，但没有任何执行者消费 `Replace`；会话只能 append 并切视图，不能按 seq 破坏式收缩历史。这是「默认开启」承诺的唯一实质缺口。

### 3.2 改动点

1. **`conversation_session/history.rs`** 新增 `replace_range(&mut self, start_seq, end_seq, summary, archive_ref) -> bool`：
   * 用 `state.seqs` 定位首尾下标，越界或非连续返回 `false`；
   * 把 `[start, end]` 区间消息移入归档登记（`compaction` 状态记录 `archive_ref`，保留原有 `history_normalization` 语义）；
   * `state.messages`/`state.seqs` 用 summary 替换该区间，`next_seq` 递增，重建 `active_view = MessageView::Full`；
   * 重算 ledger 与 tracker 的活跃估算，返回 `true`；版本号沿用 `add_message` 的单调计数。
2. **`conversation_compression.rs`** 新增 `apply_replacement(conversation, call, engine_name, capabilities, replacement) -> CompactionEngineResult<ShrinkProof>`：
   * 先 `validate_replacement(engine_name, capabilities, call, replacement, min_reclaim)`（Q8 复用 `r_min` 原则，默认 2000）；
   * 再加会话写锁、校验 anchor 版本，调 `replace_range`；失败统一映射为 `CompactionEngineError::Rejected`。
3. **`coordinator/iteration/compression.rs:451-459`**：自动路径 `Replace` 分支改为调用宿主应用函数；成功后复用现有 checkpoint gated 与 `still_over_budget` 处理，失败写 `ContextCompressionFailed` 审计并释放 flight。
4. **`entity.rs:184-218`**：新增 `compact_now_applied(&self, reason) -> Option<Result<(CompactionOutcome, Option<ShrinkProof>), String>>`，在 `compact_now` 得到 outcome 后直接应用 `View`（复用现有信号链）或 `Replace`（新链），供 API 层使用；原 `compact_now` 保持不变供测试。
5. **`context_store.rs:16-31`**：`WritebackOp` 增加 `Replace` 变体与 wire 名 `WRITEBACK_OPERATION_REPLACE`，`apply_versioned_writeback` 增加对应分支（保留 Append 兼容嵌套 agent 写回，非向后兼容负担而是不同语义共存）。

### 3.3 代码形态

```rust
// conversation_session/history.rs
pub fn replace_range(
    &mut self,
    start_seq: u64,
    end_seq: u64,
    summary: Message,
    archive_ref: &str,
) -> bool {
    let Some(start) = self.state.seqs.iter().position(|s| *s == start_seq) else { return false };
    let Some(end) = self.state.seqs.iter().position(|s| *s == end_seq) else { return false };
    if start > end { return false; }
    // archive + replace + re-project
}
```

## 4. 任务二：checkpoint engine_id 快照字段

### 4.1 现状与差距

F8/F9/F10：工厂已支持私有 config，但 `AgentLoopConfig` 无字段，恢复用宿主 limits 而非快照值，跨恢复可能换引擎。

### 4.2 改动点

1. `wf-types/src/agent_execution/loop_config.rs:119` 后新增（serde 默认，老快照缺字段时回退 builtin）：
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub compaction_engine: Option<String>,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub compaction_engine_config: Option<serde_json::Value>,
   ```
2. `coordinator/lifecycle.rs:478,546` 把恢复出的 `config` 传入 `run_loop`；`run.rs:237-256` 优先用 `config.compaction_engine`/`compaction_engine_config` 覆盖宿主 limits 后再构建与 `bind_compaction`。
3. `build_compaction_engine_with_config` 返回 `None` 时按 Q5 运行期告警并回退 builtin（启动期仍严格报错），避免恢复被未知引擎卡死。
4. 所有 `AgentLoopConfig { .. }` 构造点补 `..Default::default()` 或显式 None；用 `cargo check --workspace` 定位遗漏。

## 5. 任务三：可观测与示例引擎

### 5.1 现状与差距

F11/F12/F13：ProgressSink 契约与 metrics 采集器在，但无消费方、无 start/end 事件、无完整示例引擎。

### 5.2 改动点

1. `wf-types/src/events/base.rs:32-43` 增加 `ContextCompressionStarted`/`ContextCompressionEnded` 两类事件；`token_events/compression.rs` 增加构造函数，`token_events/meta.rs` 增加 Meta 与 TryFrom。
2. `compaction/engine.rs` 新增 `TracingProgressSink`（或 `EventProgressSink`）实现 `ProgressSink::report`，按阶段阈值发 start/end 事件；宿主在自动与手动入口注入。
3. `wf-metrics/src/collectors/compaction.rs` 的 `record_plan_emitted` 等增加 `engine` 标签，调用点传引擎名。
4. 新增 `crates/infra/wf-plugin/examples/compaction-engine-demo/`：detached workspace crate，`WfNativePlugin` + `export_plugin!`，在加载钩子里调 `register_compaction_engine` 注册一个返回 `Noop`/`View` 的示例工厂；配 `plugin.toml` 与 README。
5. registry 测试补一条：注册示例引擎 → `build_compaction_engine_with_config` 构建 → 返回非 builtin → 注销，证明完整替换。

## 6. 任务四：`/compact` 命令挂载

### 6.1 现状与差距

F7：命令层不持 entity，必须经 HTTP API；`/recap` 是同构范例；引擎侧 `compact_now` 已就位。

### 6.2 改动点（按 Q6：CLI/TUI 优先，Web 随后）

1. **后端**：仿 `wf-server/src/api/recap.rs:50,113` 新增 `POST /agent-loops/{id}/compact`，调 `wf-api` face → `ctx.agent_loop(id).compact_now_applied(reason)`；返回压缩摘要（outcome 类型、reclaim tokens、engine 名）。在 `router.rs:131` 合并路由。
2. **CLI**：`wf-mini/src/input/commands.rs:200` 增加 `SlashCommand::Compact { reason }` 与解析/help；`wf-mini/src/session/commands.rs:10` 增加分发，复用 recap 的 HTTP 客户端封装发 POST。
3. **TUI**：`tui-core/src/command.rs:107` 增加 `CommandSpec`；`wf-tui/src/interactive/panels/panel_command.rs:14` 增加 match 分支，调用同一后端 API。
4. **Web**：`apps/web-app/src/lib/config/commands.ts:3,15` 增加 action/行；`chat-session.svelte.ts:740` 增加执行器，复用 REST 服务封装；i18n 补文案。
5. **状态呈现**：TUI `context_sidebar.rs:198-256`、Web `stream-parser.ts:31` 消费 start/end 事件显示引擎名与进度。

## 7. 依赖与实施顺序

| 顺序 | 任务 | 依赖 | 验收 |
|---|---|---|---|
| 1 | Replace 写回链 | F1-F6 | Replace 可被消费、闸门生效、单测覆盖成功/拒绝/过期版本 |
| 2 | checkpoint engine_id | F8-F10 | 快照带引擎字段，恢复走快照配置，老快照回退 builtin |
| 3 | 可观察与示例引擎 | F11-F13 | start/end 事件可见、示例引擎注册并替换 builtin |
| 4 | `/compact` 命令面 | 1+3 | CLI/TUI 命令贯通，Web 命令可见并调用 API |

## 8. 测试策略

* 写回链：区间替换成功、反向区间拒绝、proof 不足拒绝、陈旧版本丢弃、归档引用登记。
* checkpoint：含引擎字段快照往返一致、缺字段回退、未知引擎告警回退。
* 可观测：ProgressSink 上报阶段事件、metrics engine 标签、示例引擎替换 builtin 跑通。
* 命令：解析 `/compact`、API 调用 mock、TUI action 映射、Web action 解析。
* 全量：`cargo check --workspace` + 相关 crate 单测。

## 9. 风险与回退

* **风险 1**：破坏式替换改变 append-only 不变量，可能影响 handoff/checkpoint 回放。缓解：仅 Replace outcome 走该路径，View 保持原 append-only；替换前后各打一次 checkpoint。
* **风险 2**：跨 crate 命令面改动导致 app 层编译断裂。缓解：每加一端跑一次 `cargo check -p <crate>`。
* **风险 3**：恢复时未知引擎导致执行卡死。缓解：Q5 运行期回退 builtin 并告警，启动期仍严格拒绝。

## 10. 验收清单

- [ ] `cargo check --workspace` EXIT=0
- [ ] 写回链、checkpoint、registry、命令解析相关单测全绿
- [ ] 一个执行实例的引擎在 checkpoint 恢复后仍唯一
- [ ] `/compact` CLI/TUI 可触发并回显结果
- [ ] 示例引擎完整替换 builtin 并通过同一套契约测试
- [ ] 设计文档 §13 同步更新，过期「待续」条目改写为「已落地」

## 11. 待拍板开放点

| # | 开放点 | 备选 | 建议 |
|---|---|---|---|
| R1 | Replace 的 archive 是否落盘为独立文件 | 会话状态内登记引用 / 复用现有归档工具写文件 | 先登记引用，避免新增 IO 协议；示例引擎可自行写文件 |
| R2 | `/compact` 手动命令是否允许 force 参数 | 仅固定 force=true / 命令行可选 | 固定 force=true，与设计文档 `ManualRequest` 一致 |
| R3 | Web 端是否与 CLI/TUI 同批落地 | 同批 / 随后 | 按 Q6 随后；先保证 CLI/TUI/API 贯通 |
| R4 | start/end 事件是否新增 EventType | 新增 / 复用 Completed 带 phase | 复用现有四类 + meta phase，避免扩散事件类型；若 TUI 需要独立订阅再新增 |

## 12. 落地状态（随代码持续更新）

| 任务 | 状态 | 验收 |
|---|---|---|
| 一、Replace 写回链 | 已落地 | `replace_range_with_view` + `apply_replacement` + 自动/手动入口消费；新增 4 项单测；`validate_replacement` 闸门生效 |
| 二、checkpoint engine_id | 已落地 | `AgentLoopConfig` 两字段 + 快照合并 + 恢复优先用快照；32 处构造点补字段；Q5 未知引擎回退 builtin |
| 三、可观测与示例引擎 | 已落地 | `TracingProgressSink` + `record_engine_outcome`；`examples/compaction-engine-demo` 单测通过 |
| 四、`/compact` 命令 | API/CLI/TUI 已落地，Web 随后 | `POST /agent-loops/{id}/compact` + CLI SlashCommand + TUI `session.compact`；wf-server/wf-mini/wf-tui 编译通过 |

**务实调整**（相对 §3/§5/§6 原方案）：
- Replace 采用视图层破坏式替换（history 保留、可回滚），`WritebackOp::Replace` 仅作 wire 契约保留，事件总线不传归档引用；宿主直接在入口应用。
- 可观测不新增 `EventType`，用 metrics engine 维度 + `TracingProgressSink` 代替独立 start/end 事件。
- Web 命令按 Q6 随后（前端无法在本沙箱编译验证）。

**验证命令与结果**：
- `cargo check --workspace --tests`：通过（仅 2 个既有无关 warning）
- `cargo test -p wf-execution-shared -p wf-agent`：全绿（含新增 4 项写回链测试；修正 `writeback_op_resolves_wire_names` 以接受 replace 变体）
- `cargo test`（examples/compaction-engine-demo）：1 项通过
- `cargo check -p wf-server -p wf-mini -p wf-tui -p tui-core`：通过

**仍待续**：Web `/compact` 命令与 start/end 状态卡。
