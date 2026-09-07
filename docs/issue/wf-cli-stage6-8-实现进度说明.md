# wf-cli 阶段6-8（issue 归档 I4/I5/I6）实现进度说明

> 文档属性：实现进度记录（issue 归档）
> 基准：2026-09-06 `wf-agent` 工作区（含未提交的暂存/未暂存改动）
> 上游：`docs/issue/wf-cli-stage7-剩余问题-运行时冒烟收尾-分析与修改方案.md`（§四 执行顺序表第 6-8 项 = I4/I5/I6）
> 执行决策：用户已确认"全量实现（含跨 crate）"，即 I4 打通存储层游标 API、I5 增加运行期 active_shutdown 全局标记，不再停留在"开放项不做"的保守档

---

## 一、进度总览

| 顺序 | Issue | 任务 | 状态 | 说明 |
| :--- | :--- | :--- | :--- | :--- |
| 6 | I4 cursor 分页补载 | 存储游标 API（I4-1） | ✅ 已落地 + 单测通过 | wf-api `entity/message.rs` 新增 `page_by_execution` / `page_by_agent_loop`，wf-storage `MessageListOptions` 增加 `before_timestamp` |
| 6 | I4 | replay 层分页函数（I4-2） | ✅ 已落地 + 单测通过 | `replay.rs` 提取共享 `records_to_lines` / `summary_history_line`，新增 `replay_scrollack_page`；全量版保留 |
| 6 | I4 | session 层 Partial 补载（I4-3） | ✅ 已落地 + 状态机单测通过 | `ReplayPager` 纯状态机、`ReplayPhase::Partial`、`ReplayEarlier` 前插、PageUp 滚顶补载 |
| 7 | I5 退出防 failover | TUI 本地 graceful（I5-1） | ✅ 已落地 + 单测通过 | `TuiApp::run` / `apply_session_exit` 清理前置位；session 抑制 Failed/Interrupted 落屏 |
| 7 | I5 | runtime 全局 active_shutdown（I5-2） | 🔧 代码已落地，编译/单测验证被打断（见 §四） | wf-common 标记模块 + wf-runtime 关闭路径置位 + wf-agent 终结决策改为 Cancel |
| 8 | I6 模态配色 | Modal draw 透传 &Theme | ⏳ 未开始 | 文档建议 B；本轮"全量实现"意向 A，需在最终同步时定稿 |
| — | 收尾 | 全量编译/测试/clippy/fmt/文档同步 | ⏳ 未开始 | — |

---

## 二、已完成改动明细（按文件）

### I4-1 存储层游标查询 API

- `crates/infra/wf-storage/src/adapter/message.rs`
  - `MessageListOptions` 新增 `before_timestamp: Option<i64>` 字段；`From<MessageListOptions> for QueryFilter` 在设置游标时追加 `FilterOp::Lt("timestamp", ..)` 与 `FilterOp::OrderBy("timestamp", true)`（先排序后 limit，确定性分页）。三后端（memory / sqlite / postgres）既有 `Lt/OrderBy/Limit` 语义可直接承接。
- `crates/app/wf-api/src/entity/message.rs`
  - 新增 `MessagePage { records, has_more }`（新页面以 limit+1 探测是否有更旧记录，返回截断到 limit 的最新在前记录）。
  - 新增 `page_by_execution(ctx, id, before_timestamp, limit)` 与 `page_by_agent_loop(...)`，游标 `None` 从最新开始（内部以 `i64::MAX` 兜底）。
  - 既有 `MessageListOptions` 构造点补齐新字段（`recent` / `by_execution_paginated`）。
  - 单测：`cursor_page_newest_first_with_more`（跨两页推进 + 末页 `has_more=false`）、`cursor_page_boundaries`（空页 / 恰好 limit / 旧于全部 / agent-loop 空作用域）。
- `crates/app/wf-server/src/api/workflow/messages.rs`：构造点补 `before_timestamp: None`。

### I4-2 replay 层分页读取

- `crates/app/wf-cli/src/replay.rs`
  - 从全量 `replay_scrollack` 提取共享行转换 `records_to_lines`（消息 → 与 live 一致的 `HistoryLine` 角色）与 `summary_history_line`（▣ 摘要行），全量版改为复用。
  - 新增 `ReplayPage { lines, next_before, has_more }` 与 `replay_scrollack_page(ctx, id, before, limit)`：校验 summary → 优先 agent-loop 作用域 → 分页拉取 → 页内按时间升序转行；tail 页（before=None）附加摘要；无消息纯迭代历史路径回退调用全量版一次（保持语义不变）。
  - 常量 `REPLAY_PAGE_LIMIT = 200`。
  - 单测：`replay_page_starts_at_tail_and_pages_older`（尾页 → 更早页 → 首页，逐页游标推进，拼装结果与全量版一致、摘要不重复）、`replay_page_falls_back_for_iteration_only_sessions`。
  - 全量 `replay_scrollack` 保留：mini 模式与 `tests/replay_fixture.rs` 零回归。

### I4-3 session 层 Partial 补载与前插

- `crates/app/wf-cli/src/session.rs`
  - `SessionEvent`：`ReplayLoaded` 改为带 `has_more` / `next_before` 的结构化载荷；新增 `ReplayEarlier`（前插语义）。
  - `ReplayPhase` 增加 `Partial`；新增纯状态机 `ReplayPager`（`begin / land_initial / can_load_earlier / start_earlier / land_earlier / fail`），无 I/O、可单测。
  - `SessionController`：`load_replay` 改为拉取 tail 页（后台任务）；新增 `request_earlier_page()`；事件分支按 phase 区分"整体替换"与"前插"；补载页到达时前插并抬高 `view_scroll` 保持视口锚定。
  - 滚动接线：`PageUp/PageDown`（在 footer 各 view 之外先消费），`scroll_history_up` 在已到最顶且 `Partial` 时触发更早页请求；`draw_scrollback` 支持 `view_scroll` 并维护 `scroll_at_top`。
  - 单测：ReplayPager 全状态迁移（initial→Partial→…→Complete、首页即 Complete、失败停止、begin 复位）、scroll 顶格触发。
- 注：`tui.rs` 的 `apply_pending_replay` 仍只调 `session.load_replay(&id)`，事件形态变化封闭在 session 内，无跨文件改动。

### I5-1 TUI 本地 graceful 防护

- `crates/app/wf-cli/src/session.rs`
  - `SessionController` 新增 `graceful` 标志与 `begin_graceful_exit()`；`should_render_terminal(graceful, event)` 纯函数：graceful 期间 `Failed`/`Interrupted` 不再渲染错误行（错误仍推进内部状态收尾，只是不落屏）。
  - 单测：`terminal_events_render_normally_when_active`、`terminal_failures_are_suppressed_after_graceful_exit`。
- `crates/app/wf-cli/src/tui.rs`
  - `TuiApp::run` 退出清理段在 `session.shutdown()` 之前 `begin_graceful_exit()`；`apply_session_exit`（Ctrl-C 两按离开会话）同样先置位再 shutdown。

### I5-2 运行期全局 active_shutdown（代码已落地，验证未完成）

- `crates/foundation/wf-common/src/shutdown.rs`（新文件，`lib.rs` 已导出）
  - 进程级 `ACTIVE_SHUTDOWN: AtomicBool` + `begin_active_shutdown()` / `is_active_shutdown()` / `clear_active_shutdown()`（clear 仅供测试与作用域复位）。
- `crates/app/wf-runtime/src/bootstrap.rs`
  - `Runtime::shutdown()` 开头置位标记，并持有局部 `ActiveShutdownScope`（RAII，drop 时复位），保证整个 teardown 期间生效、结束后不污染同进程后续 runtime；`ActiveShutdownScope` 类型定义同文件。
- `crates/engine/wf-agent/src/coordinator/lifecycle.rs`
  - 抽出纯决策 `settle_kind(err, active_shutdown) -> Timeout | Cancel | Fail`：host 关闭中一律 `Cancel`（走 `cancel_agent_loop`，不派发 `AgentFailed`、不落 `Failed` 状态），运行期 timeout 走 `Timeout`、其余 `Fail`，原语义保持。
  - Err 分支改为按该决策分流；已追加三个单测（generic 错误运行期 Fail / timeout 运行期 Timeout 且关闭期 Cancel / 关闭期全 Cancel）。

---

## 三、范围与兼容性说明

- **未触碰的模块**：I4 未删除全量 replay 路径（mini 与既有测试继续走 `replay_scrollack`）；I5 未改 wf-api dispatch 与 wf-server、未改 workflow（非 agent）执行引擎的失败路径——本次关闭防 failover 覆盖 agent-loop 生命周期终结点与 wf-runtime 关闭入口。
- **I6 决策**：issue 文档 I6 建议"先选 B 并文档化"（模态维持内置回退主题），规划文档已按 B 记录；本轮用户选择"全量实现（含跨 crate）"的选项文字同时给 A/B 两个取向，最终取 A（透传 &Theme）还是维持 B，需在任务 #9 定稿（见 §四 未决点）。
- **暂存区**：阶段1-5（I1/I7/I3/I2/I8）改动仍在暂存区（HEAD `db77622` 之上），本阶段改动为未暂存工作区改动，尚未提交。

---

## 四、当前未决 / 下一步

1. **I5-2 验证**（本应紧接的任务）：`cargo test -p wf-common --lib shutdown` 与 `cargo test -p wf-agent --lib coordinator::lifecycle` 尚未跑完（执行被中断），需先确认全绿。
2. **任务 #9（I6）**：在 A（`Modal::draw` 增加 `&Theme` 并改造 8 个模态实现 + `ModalStack::draw` + `tui.rs:444` 调用点）与 B（维持 + 文档声明）之间定稿并落地。
3. **任务 #10（收尾验证）**：`cargo test -p wf-cli`（含 replay/session 新单测）、`cargo clippy --all-targets --all-features`、`cargo fmt`；把 I4/I5 落地状态同步回 `docs/issue/wf-cli-stage7-剩余问题-运行时冒烟收尾-分析与修改方案.md`（§四 表 6-8 行状态）与规划文档相关条目。

---

## 五、与既有文档关系

- 本文件为"阶段6-8 执行中"的进度快照，随任务推进持续更新；最终完成态将以 §四 的文档同步并入上游 issue/规划文档。
- 遵循 `AGENTS.md`：代码注释仅英文；本文件为中文规划，仅自然语言描述与锚点，不含完整代码片段。
