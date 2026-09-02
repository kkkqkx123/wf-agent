# wf-cli 剩余问题 - 全屏 TUI 深化方案

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 全屏 TUI（Stage 7）的联调与硬化
> 关联文档：`docs/plan/cli/wf-cli-剩余问题-分阶段深化总览.md`、`docs/plan/cli/wf-cli-分阶段实现方案.md:Stage7`、`docs/cli/02-ui布局与页面划分.md`、`docs/cli/03-组件设计方案.md`、`docs/cli/04-终端交互设计.md`、`docs/cli/05-opencode-mini模式与无头模式设计.md`
> 源码锚点：`crates/app/wf-cli/src/screens.rs:1` / `modal.rs:1` / `tui.rs:16` / `terminal.rs:1` / `theme.rs:1` / `replay.rs:1` / `lib.rs:244` `run_interactive` / `wf-api/src/workflow/*` / `wf-api/src/infra/events.rs`

---

## 一、现状与剩余问题

### 1.1 已完成（骨架）

- `screens.rs:1` 实现 8 屏枚举 `ScreenKind::{Dashboard, Workflow, Executions, Session, Checkpoints, Search, Settings, Help}` + `Screens` 栈（`push/pop/depth/selected/select_next/select_prev/navigate_to/go_back`）+ `draw` 的占位渲染（`Block + Paragraph/List`，`Dashboard` 的 `selected` 高亮、`Session` 的 `Layout [Min(5), Length(3)]` 输入占位）。
- `modal.rs:1` 实现 `Modal` trait + `ModalStack` + `ConfirmModal`（`y/n/q/Esc`）+ `HelpModal`（`Esc/q/?`）+ `centered_rect(60,30)` 布局。
- `tui.rs:16` 实现 `TuiApp { adapter, screens, modals }` 的 `TerminalGuard::enter(TUI) → Terminal::new(CrosstermBackend) → event_loop(poll 100ms)` 循环，键映射 `map_key(CKey)` + `digit_to_screen`（`1-8` 切屏）+ `j/k/Up/Down` 导航 + `Enter` 下钻 + `q/Esc` 返回/退出 + `?` 弹窗 + `Ctrl-C` 退出 + `Resize` 重绘。
- `lib.rs:244` `run_interactive` 的 `Tui` 分支已从 `Configuration("full TUI not yet implemented")` 改为 `TuiApp::new(adapter).run().await`。

### 1.2 剩余问题

| 编号 | 层 | 现状 | 剩余缺口 | 影响 |
| :--- | :--- | :--- | :--- | :--- |
| E1 | 屏幕数据 | 8 屏均为静态占位文本，未接 `DomainAdapter` 真实数据 | 未调用 `workflow_summaries / agent_loop_registry::summaries / checkpoint::list / search` 等；`Workflow/Executions` 列表为空；`Search` 无输入框 | TUI 无业务价值 |
| E2 | 模态框 | 仅 `Confirm/Help`，缺 `FileViewer/DiffViewer/ModelPicker/SessionPicker/PasswordModal/FileSelectionDialog` 等 02 文档要求的 6+ 模态 | 未实现 `Modal` 的 `on_confirm` 异步回填；缺 `oneshot` 结果通道（03 文档 `Modal trait + oneshot`） | 删除/凭证/文件选择无法交互 |
| E3 | 会话屏幕 | `Session` 为 `Placeholder` 日志 + `>` 输入框，未接 `workflow_execution::stream` 的 `ExecutionStreamEvent` 流式 | 未复用 `run.rs:402` 的 `SessionRenderer` + `reducer.rs:14` 的 `MiniCommit` + `markdown.rs:14` 的 `MarkdownStream` | 无法前台运行 |
| E4 | 执行跟踪 | `Executions` 为静态文本，未实现 `ExecutionType` 统一过滤（`Workflow + AgentLoop`）与 `status` 过滤 | 未暴露 `ExecutionType::{Workflow, AgentLoop}` 的统一列表；缺 `execution list --status` 的 TUI 侧 `Filter` 组件 | 执行跟踪与 headless 不一致 |
| E5 | 状态与重放 | 缺 `replay.rs` 的 `Partial/Complete/LoadingBeginning` 分页补载与 `resize` 后的 `reflow` | 未实现 `scrollback.rs:HistoryLine` 的 `display_lines(width)/reflow` 在 TUI 侧的宽度键控缓存；长会话滚动到顶部不补载 | 长会话截断 |
| E6 | 终端与性能 | `tui.rs:70` 的 `poll 100ms` 未与 `FrameRequester` 限帧联动；未处理 `SIGTSTP/SIGCONT` 挂起恢复 | 未复用 `framer.rs:FrameRequester` 的 `30-60fps` 限帧；`size.rs:ResizeDebouncer` 的 `75ms` 防抖未接入 | 帧率抖动、resize 闪烁 |

---

## 二、修改目标

1. 8 屏从占位补齐至"可浏览真实数据"，`Workflow/Executions/Search` 三屏达到与 `wf workflow list / execution list / search` 同等数据面。
2. 模态栈从 2 个补齐至 6+ 且支持 `oneshot` 异步结果（删除确认、凭证输入、文件选择）。
3. 会话屏达到 `mini` 同等流式能力（`ExecutionStreamEvent → Reducer → MarkdownStream → HistoryLine → Frame::draw`）。
4. 达到 `02-ui布局与页面划分.md` 的 8 屏 + `03-组件设计方案.md` 的模态/组件全量验收。

---

## 三、分阶段修改方案

### 阶段 E1 - 屏幕数据绑定（静态 → 真数据）

**目标**：`Dashboard/Workflow/Executions/Checkpoints/Search/Settings` 5 屏可浏览真实数据。

**改动**：

- `screens.rs:1` 重构每屏 `draw_*` 为 `draw_*_with_data(frame, area, data: &ScreenData)`，`ScreenData` 枚举按屏聚合：
  - `Dashboard` → `workflow count / execution count / checkpoint count / recent searches`（`tokio::join!(workflow_summaries, agent_loop_registry::summaries, checkpoint::list)` 预取）
  - `Workflow` → `Vec<WorkflowSummary>`（分页 `limit 20`，`keyword` 输入态）
  - `Executions` → `Vec<ExecutionSummary>`（`ExecutionType` 统一，`status` 过滤器 `Running/Paused/Completed/Failed/Cancelled/All`）
  - `Checkpoints` → `Vec<Checkpoint>`（按 `execution_id` 聚合）
  - `Search` → `SearchResult { workflows, executions, tasks }`（输入框 `Composer` 单行复用）
  - `Settings` → `LlmProfileSummary + Theme`（`llm_profile::list` + `theme::probe_theme`）
- `tui.rs:22` `TuiApp` 新增 `data: ScreenDataCache`（`HashMap<ScreenKind, CachedData{data, fetched_at}>`），`event_loop` 首帧前 `tokio::spawn` 预取 `Dashboard` 数据，`navigate_to` 时按需 `fetch`（`fetched_at` 过期 5s 重新拉取）。
- 保持 `draw` 内无 `await`，数据缺失时显示 `Loading...` 或 `Empty` 占位。

**涉及文件**：`crates/app/wf-cli/src/screens.rs`、`crates/app/wf-cli/src/tui.rs`、`crates/app/wf-cli/src/domain.rs`（`ApiContext` 访问）

**验收**：`wf --tui` 首屏 `Dashboard` 显示真实 `workflow/execution` 计数；`1` 切 `Workflow` 显示列表；`5` 切 `Search` 可输入并显示结果。

---

### 阶段 E2 - 模态栈全量

**目标**：模态从 2 个补齐至 02 文档要求的 6+。

**改动**：

- `modal.rs:1` 扩展 `Modal` trait 增加 `fn is_transparent() -> bool`（默认 `false`，`Help` 为 `true` 的半透明遮罩）。
- 新增模态：
  - `FileViewer { title, content: Vec<Line> }`（`scrollback::HistoryLine` 的 `display_lines` 复用，`j/k` 滚动）
  - `DiffViewer { from, to, diff: Vec<DiffLine> }`（`workflow version diff` 的 `serde_json::to_value` 差分渲染，`+/-` 着色）
  - `ModelPicker { profiles: Vec<LlmProfile>, selected }`（`select.rs:SelectList` 复用，分组滚动）
  - `SessionPicker { sessions: Vec<SessionMeta>, selected }`（`replay.rs` 的 `list_sessions` 数据源）
  - `PasswordModal { prompt, masked_input }`（`composer.rs` 的 `Input` 复用，`input` 掩码 `*`）
  - `FileSelectionDialog { cwd, files, selected }`（`scan_files` 的 `deferred` 异步，`Enter` 确认）
- `ModalStack` 增加 `oneshot::channel<ModalResult>` 的 `push_with_result` 变体，`handle_key` 返回 `Some(result)` 时 `sender.send(result)`（03 文档 `Modal trait + oneshot`）。

**涉及文件**：`crates/app/wf-cli/src/modal.rs`、`crates/app/wf-cli/src/select.rs`（复用）、`crates/app/wf-cli/src/composer.rs`（复用）

**验收**：`Workflow` 屏 `Delete` 触发 `ConfirmModal` 并 `oneshot` 回填后执行 `delete_workflow`；`Settings` 屏 `ModelPicker` 可切换 `llm_profile`；`?` 的 `HelpModal` 半透明遮罩不阻断底层重绘。

---

### 阶段 E3 - 会话屏流式联调

**目标**：`Session` 屏达到 `mini` 同等"日志流 + 底部输入 + 状态行 + 阶段感知输入"。

**改动**：

- `screens.rs:206` `draw_session` 重构为 `draw_session_with_state(frame, area, state: &SessionState)`，`SessionState { scrollback: Vec<HistoryLine>, composer: Composer, footer: FooterState, phase: SessionPhase }`。
- `tui.rs:53` `TuiApp` 新增 `session: Option<SessionHandle>`，`SessionHandle { execution_id, rx: mpsc::Receiver<ExecutionStreamEvent>, reducer: Reducer, markdown: MarkdownStream }`。
- `event_loop` 中 `Session` 屏激活时：
  - 输入 `Enter` 时 `DomainAdapter::api_context()` 经 `workflow_execution::stream` 或 `agent_execution::stream` 启动，`tokio::spawn` 驱动 `rx` 消费
  - 每 `ExecutionStreamEvent` 经 `reducer::reduce` → `Vec<MiniCommit>` → `HistoryLine` 追加 `scrollback` + `FrameRequester::request_frame` 标记重绘
  - `LlmDelta` 经 `markdown.rs:14` 的 `MarkdownStream` 按 `top-level block` 增量提交，未完结 block 不固化（复用 `Stage5` 的 `holdback/table` 逻辑）
  - `Footer` 按 `phase: Idle|Streaming|Approval|Question` 路由 `approval.rs:ApprovalView` / `question.rs:QuestionView`
- 复用 `mini.rs` 的 `Queue` 排队与两按 `SIGINT` 退出语义。

**涉及文件**：`crates/app/wf-cli/src/screens.rs`、`crates/app/wf-cli/src/tui.rs`、`crates/app/wf-cli/src/reducer.rs`、`crates/app/wf-cli/src/markdown.rs`、`crates/app/wf-cli/src/footer.rs`、`crates/app/wf-cli/src/composer.rs`、`crates/app/wf-cli/src/approval.rs`、`crates/app/wf-cli/src/question.rs`

**验收**：`wf --tui` 的 `Session` 屏可输入 `prompt` 并流式显示 `LLM` 回答与 `ToolStart/End` 的 `▲/✓/✗` 行；`Approval` 阶段 `y/a/d/n/c` 键位生效（对齐 `approval.rs:ApprovalView`）；`Ctrl-C` 两按退出不残留 `raw mode`。

---

### 阶段 E4 - 执行跟踪统一与过滤

**改动**：

- `screens.rs:196` `draw_executions` 增加 `FilterBar { status: ExecutionStatusFilter, workflow_id: Option<String>, type: ExecutionType }`（`select.rs:SelectList` 的单行过滤器复用）。
- `TuiApp` 的 `ScreenDataCache::Executions` 存储 `Vec<ExecutionSummary>` + `Filter`，`j/k` 导航时高亮行与 `Enter` 下钻 `Session`（`execution inspect` 的详情数据）。
- 过滤器变更时 `tokio::spawn` 重新 `agent_loop_registry::summaries` + `list_executions` 双源聚合，去重后 `sort_by(created_at)`。

**涉及文件**：`crates/app/wf-cli/src/screens.rs`、`crates/app/wf-cli/src/tui.rs`、`crates/app/wf-cli/src/select.rs`

**验收**：`Executions` 屏可按 `status` 过滤（`Running` 仅运行中）；`Enter` 下钻 `Session` 显示 `timeline/iterations` 的 `audit` 数据。

---

### 阶段 E5 - 重放与分页补载

**改动**：

- `replay.rs:1` 扩展 `ReplayState { scrollback: Vec<HistoryLine>, cursor: Option<String>, state: ReplayStateKind }`，`ReplayStateKind::{Partial, Complete, LoadingBeginning}`。
- `TuiApp` 的 `SessionState` 在 `scrollback` 滚到顶部（`selected == 0` 且 `ScrollUp`）时触发 `replay::replay_scrollback(ctx, cursor, limit=50).await` 的分页补载，`cursor` 为最早 `HistoryLine` 的 `execution_id`。
- `TerminalGuard` 的 `resize` 事件经 `size.rs:ResizeDebouncer` 的 `75ms` 防抖后，`HistoryLine::reflow(width)` 按新宽度重算 `display_lines`。

**涉及文件**：`crates/app/wf-cli/src/replay.rs`、`crates/app/wf-cli/src/scrollback.rs`、`crates/app/wf-cli/src/size.rs`、`crates/app/wf-cli/src/tui.rs`

**验收**：长会话（`scrollback > 200` 行）在 TUI 中滚到顶部时自动补载更早历史；`resize` 后 `reflow` 无截断。

---

### 阶段 E6 - 终端与性能硬化

**改动**：

- `tui.rs:70` `poll 100ms` 改为 `framer.rs:FrameRequester` 的 `30-60fps` 限帧：`FrameRequester::new(60)` 的 `request_frame` 在 `reducer/markdown` 产出时调用，`should_draw(now)` 决定是否 `terminal.draw`，输入事件即时唤醒。
- `terminal.rs:1` 增加 `SIGTSTP/SIGCONT` 处理：`SIGTSTP` 时 `guard.with_restored(None, || {})` 暂停 TUI，`SIGCONT` 时重绘。
- `theme.rs:1` 的 `SIGUSR2` 热更新经 `theme_reload_signals` 的 `mpsc` 在 `event_loop` 中 `select!` 分支处理，`Theme` 变更后 `frame` 全量重绘。

**涉及文件**：`crates/app/wf-cli/src/tui.rs`、`crates/app/wf-cli/src/framer.rs`、`crates/app/wf-cli/src/terminal.rs`、`crates/app/wf-cli/src/theme.rs`

**验收**：`cargo bench` 的 `reducer 万级事件 <100ms`；`TUI` 在 `scrollback 1000+` 行时 `render <8ms/帧`；`SIGTSTP` 挂起后终端状态干净，`fg` 恢复后重绘正常。

---

## 四、依赖与顺序

```
E1 数据绑定 ─► E2 模态全量 ─► E3 会话流式 ─► E4 执行跟踪 ─► E5 重放分页 ─► E6 性能硬化
     │              │
     └──────────────┴── E1/E2 可并行部分（模态不依赖数据面）
```

- E1 为前置，E3 依赖 E1 的 `Workflow/Executions` 数据面与 E2 的 `ConfirmModal`，E6 最后。

---

## 五、测试与验收

| 层 | 用例 |
| :--- | :--- |
| 纯函数 | `screens.rs:tests::screens_navigation_stack` 已有，新增 `screens_data_cache_fetch` 的 `fetched_at` 过期单测 |
| 组件渲染 | `insta` 快照：`Sessions` 的 `FilterBar` 三状态（`All/Running/Failed`）、`Session` 的 `Phase::Streaming` 与 `Phase::Approval` 的 `footer` 形态 |
| 形态冒烟 | `wf --tui` 的 `script` 合成事件驱动（`examples/mini_demo.rs` 复用 `ExecutionStreamEvent` 合成），`cargo test -p wf-cli --lib` 的 `tui::tests::tui_smoke`（`FakeTerminal`） |
| 集成 | `tests/tui_screens.rs`（`DomainAdapter` 内存存储 + 预置 `workflow/execution` 后 `TuiApp` 的 `fetch` 断言） |
| 性能 | `benches/tui_render.rs`（`scrollback 1000` 行的 `draw` 耗时） |

---

## 六、风险

| 风险 | 缓解 |
| :--- | :--- |
| `draw` 内误用 `await` 阻塞帧 | `screens.rs` 的 `draw` 签名保持 `fn draw(&self, frame: &mut Frame, area: Rect)` 纯同步，数据预取在 `TuiApp::event_loop` 的 `tokio::spawn` 异步分支 |
| `Session` 的 `LlmDelta` 流式与 `MarkdownStream` 的 `holdback` 状态机不一致 | 直接复用 `mini.rs` 的 `reducer + markdown` 管线（`Stage5/6` 已验证），TUI 侧不自实现 `markdown` 增量逻辑 |
| `Modal` 的 `oneshot` 在 `TuiApp` 退出时未 `drop(sender)` 导致 `await` 永不返回 | `ModalStack::push_with_result` 的 `sender` 在 `TuiApp::run` 的 `Drop` 时显式 `drop`，调用侧 `tokio::select!(result, _ = shutdown)` 超时 `500ms` 后回退 `Cancelled` |

