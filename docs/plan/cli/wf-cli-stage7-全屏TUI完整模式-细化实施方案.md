# wf-cli 完整模式（全屏 TUI / Stage 7）细化实施方案

> 状态：方案设计（细化）
> 日期：2026-09-06
> 范围：`crates/app/wf-cli` 全屏 TUI（Stage 7）从骨架到可交付
> 关联文档：
> - `docs/plan/cli/wf-cli-分阶段实现方案.md`（§Stage 7）
> - `docs/plan/cli/wf-cli-剩余问题-TUI深化.md`（E1–E6 深化）
> - `docs/analysis/` 的 codex 模块分析（`input` / `ai-output` / `history` / `resize-and-exit` / `ratatui-implementation` / `tui`）+ `wf-agent-learnings.md` 借鉴清单
> 源码锚点：`screens.rs` / `modal.rs` / `tui.rs` / `lib.rs:248` `run_interactive` / `run.rs:226` `SessionRenderer` / `mini.rs:1206` `spawn_turn` / `reducer.rs` / `markdown.rs` / `footer.rs` / `composer.rs` / `queue.rs` / `terminal.rs` / `framer.rs` / `replay.rs` / `domain.rs`

---

## 一、当前状态盘点（事实依据）

通过逐文件核对 `crates/app/wf-cli/src/` 与 `git log` 确定：

| 项 | 现状 | 事实锚点 |
| :--- | :--- | :--- |
| Stage 0–6 | 已完成，mini 模式可用 | `git log` 含 `update mini mode`；分阶段方案完成记录（6A–6E，2026-08-24） |
| markdown 流式正确性 | 已完成（表 holdback / 换行门控 / 引用链接回退 / 定稿兜底 / `streamed_equals_full` 测试） | `markdown.rs:164` `finish`、`:214` `boundary`、`:236` `rfind('\n')` 换行门控、`:298` `has_reference_definition`、`:303` `TableHoldback`、`:776` `streamed_equals_full_table_holdback` |
| Stage 7 骨架 | `screens` / `modal` / `tui` 占位 + `lib` 的 Tui 分支已接 | `tui.rs:31` `TuiApp::run`；`screens.rs:8` 8 屏枚举；`modal.rs:88/134` `ConfirmModal`/`HelpModal`；`lib.rs:270` `CliMode::Tui` 分支调 `TuiApp::new(adapter).run()` |
| E1 屏幕数据 | 8 屏全为静态占位文本 | `screens.rs:152–265` 各 `draw_*` 为硬编码字符串 |
| E2 模态栈 | 仅 Confirm/Help，无 `oneshot` | `modal.rs:28` `ModalStack` 无 `push_with_result`；`Modal` trait 无 `is_transparent` |
| E3 会话屏 | 占位（log 文本框 + `>` 输入框） | `screens.rs:206` `draw_session` |
| E5 重放分页 | `replay_scrollack` 全量重建，无 cursor 分页 | `replay.rs:12` `replay_scrollack`、`:latest_session_id` |
| E6 帧调度 | `tui.rs:70` `poll 100ms`，未用 `FrameRequester` | `framer.rs:51` `FrameRequester` 已实现（`request_frame`/`request_frame_in`/`frame_done`） |
| E6 挂起恢复 | `terminal.rs` 无 `SIGTSTP`/`SIGCONT`/`SuspendContext` | `terminal.rs` grep 无 `Suspend`/`SIGTSTP`（`with_restored`/`DoublePressTracker`/`install_panic_hook` 已存在） |
| E6 退出防 failover | 未吸收（learnings §6.3 P0） | 待在 `wf-runtime` 关闭路径打标 |

**结论**：完整模式的真实缺口集中在 E1–E6 的"接线"层——把 Stage 0–6 已落地的 reducer / markdown / footer / composer / queue / terminal / framer 资产接入 8 屏 alt-screen TUI。markdown 流式正确性等 P0 项已在 Stage 6 落实，无需在 Stage 7 重复造轮子。

---

## 二、复用组件清单（避免重复造轮子）

以下资产在 Stage 0–6 已验证，E1–E6 一律复用，不在 TUI 侧另写等价实现：

| 组件 | 文件:行 | 用于阶段 |
| :--- | :--- | :--- |
| 会话驱动内核 `SessionReducer`（流式折叠） | `reducer.rs:73` `SessionReducer` / `:fold` | E3 |
| 流式 markdown `MarkdownStream`（holdback/换行门控/定稿） | `markdown.rs:21` `MarkdownStream`、`:164` `finish` | E3 |
| 回合构造 `build_agent_loop_params` + `MiniApprovalHandler` | `mini.rs:1233`、`:1230` 注入 handler | E3 |
| 执行流入口 `agent_execution::stream` | `mini.rs:1231` | E3 |
| 单行输入 `Composer`（P0） | `composer.rs:33` `Composer`（`insert_char`/`history_prev`/`submit`） | E3 |
| 排队 `PromptQueue` / `QueuedPrompt` | `queue.rs` | E3 |
| 状态栏 `Footer` / `FooterRoute` / `FooterState` / `Phase` | `footer.rs:168` `Footer`、`:80` `FooterRoute`、`:115` `FooterState` | E3 |
| 审批/追问视图 `ApprovalView` / `QuestionView` | `approval.rs` / `question.rs` | E3 |
| 两按退出 `DoublePressTracker` | `terminal.rs:443` | E3 / E6 |
| 帧调度 `FrameRequester` + `FrameRateLimiter` | `framer.rs:51` / `:19` | E6 |
| 历史行 `HistoryLine::display_lines/reflow/desired_height` | `scrollback.rs` | E3 / E5 |
| 数据访问 `DomainAdapter::api_context/storage/llm_gateway` | `domain.rs:125` / `:140` / `:147` | E1 |
| 查询逻辑（workflow/execution/query/checkpoint/search/llm） | `cmd/workflow.rs` `cmd/execution.rs` `cmd/query.rs` `cmd/checkpoint.rs` `cmd/search.rs` `cmd/llm.rs` | E1 |
| 尺寸防抖 `ResizeDebouncer` | `size.rs` | E6 |

> E3 本质是**把 mini 的会话驱动管线移植到全屏 Session 屏**：`mini.rs:334` 事件循环 + `mini.rs:1206` `spawn_turn` 的 `tokio::spawn(agent_execution::stream)` + mpsc 回投 `MiniSessionEvent` 模式，原样复用到 `TuiApp` 的 `Session` 态。

---

## 三、Codex 设计借鉴映射（来自 7 篇分析 + learnings）

| Codex 模式（出处） | 本方案落点 |
| :--- | :--- |
| 事件循环 `tokio::select!` 多源（tui / `resize-and-exit` §2） | `tui.rs` 事件循环改造：stream rx + key + resize + theme signal(mpsc) + adapter shutdown 五源 `select!`（E6） |
| `FrameRequester` 限速 120FPS（ratatui-implementation §2.3） | `framer` 已实现，仅需在 `tui.rs` 接入 `request_frame`/`should_draw`（E6） |
| 流式 stable/tail + 表 holdback + 换行门控 + 定稿兜底（`ai-output` §4 / learnings §4） | 已落地 `markdown.rs`，E3 直接复用；补充 `streamed_equals_full` 回归测试覆盖 TUI 路径 |
| 历史分页 `Partial/Complete/LoadingBeginning`（`history` §3.3/3.4） | `replay.rs` 扩展 cursor 分页状态机（E5） |
| 退出 `ShutdownFirst` 防 failover（`resize-and-exit` §6.2 / learnings §6.3） | `wf-runtime` 关闭路径标记 `active_shutdown`，主动关闭不派发 `Failed`（E6 P0） |
| 输入边界（初始化丢弃早到输入 + 排空残留）（`input` §3.5 / learnings §3.5） | `TuiApp` 启动 + `with_restored` 恢复后 `discard_pending_terminal_input`（E6） |
| 挂起 `SuspendContext` 重应用 raw mode + `Resume` 重查（`resize-and-exit` §7.2 / learnings §6.5） | `terminal.rs` 增 `SIGTSTP`/`SIGCONT`（`SuspendContext`），恢复时重应用 raw mode + 清输入残留 + 强制重查尺寸（E6） |

---

## 四、分阶段细化方案

### 阶段 E1 — 屏幕数据绑定（静态 → 真数据）

**目标**：`Dashboard/Workflow/Executions/Checkpoints/Search/Settings` 6 屏可浏览真实数据，与 `wf ... list` 同等数据面。

**改动**：
- `screens.rs` 各 `draw_*` 改为 `draw_*_with_data(frame, area, data: &ScreenData)`；`ScreenData` 枚举按屏聚合：`Dashboard`→workflow/execution/checkpoint 计数；`Workflow`→`Vec<WorkflowSummary>`（分页 `limit 20`）；`Executions`→`Vec<ExecutionSummary>`（`ExecutionType` 统一 + `status` 过滤）；`Checkpoints`→`Vec<Checkpoint>`；`Search`→`SearchResult{workflows,executions,tasks}`（复用 `composer::Composer` 单行输入）；`Settings`→`LlmProfileSummary + Theme`。
- `tui.rs:22` `TuiApp` 增 `data: ScreenDataCache`（`HashMap<ScreenKind, {data, fetched_at}>`）；首帧前 `tokio::spawn` 预取 `Dashboard`，`navigate_to` 时按需 `fetch`（`fetched_at` 过期 5s 重拉）。
- **复用**：数据查询逻辑直接调用 `cmd/workflow.rs`、`cmd/execution.rs`、`cmd/query.rs`、`cmd/checkpoint.rs`、`cmd/search.rs`、`cmd/llm.rs` 中既有的查询函数（或经 `domain.rs:125` `api_context()` 调 `wf_api::agent::agent_loop_registry::summaries` / `checkpoint::list` / `search` 等）；保持 `draw` 纯同步、数据预取在 `event_loop` 的 `tokio::spawn` 异步分支（对齐 learnings E6 风险一）。

**验收**：`wf --tui` 首屏 `Dashboard` 显示真实 workflow/execution 计数；`1` 切 `Workflow` 列表；`5` 切 `Search` 可输入并显示结果；数据缺失显示 `Loading...`/`Empty` 不 panic。

---

### 阶段 E2 — 模态栈全量 + oneshot

**目标**：模态从 2 个补齐至 02 文档要求的 6+，支持 `oneshot` 异步结果。

**改动**：
- `modal.rs:28` `ModalStack` 增 `push_with_result`（`oneshot::channel<ModalResult>` 变体），`handle_key` 返回 `Some(result)` 时 `sender.send(result)`（对齐 `history` §3.2 的 `Modal trait + oneshot`）。
- `modal.rs:15` `Modal` trait 增 `fn is_transparent() -> bool`（`Help` 半透明遮罩不挡底层重绘）。
- 新增模态：`FileViewer`（复用 `scrollback::HistoryLine::display_lines`，`j/k` 滚动）、`DiffViewer`（workflow version diff 着色）、`ModelPicker`（`select::SelectList` 复用）、`SessionPicker`（`replay` 数据源）、`PasswordModal`（`composer::Composer` 掩码 `*`）、`FileSelectionDialog`（`scan_files` 异步）。
- 复用：`select.rs:SelectList`、`composer.rs:Composer`、`replay.rs` 会话列表。

**验收**：`Workflow` 屏 `Delete` 触发 `ConfirmModal` → `oneshot` 回填后执行 `delete_workflow`；`Settings` `ModelPicker` 切换 `llm_profile`；`?` `HelpModal` 半透明不阻断底层重绘。

**风险**：`oneshot` 在 `TuiApp::run` `Drop` 时显式 `drop(sender)`，调用侧 `tokio::select!(result, _ = shutdown)` 超时 500ms 回退 `Cancelled`（learnings E6 风险二）。

---

### 阶段 E3 — 会话屏流式联调（价值核心）

**目标**：`Session` 屏达到 mini 同等"日志流 + 底部输入 + 状态行 + 阶段感知输入"。

**改动**（直接移植 mini 管线，不复写流式内核）：
- `tui.rs:22` `TuiApp` 增 `session: Option<SessionHandle>`，`SessionHandle { execution_id, rx: mpsc::Receiver<MiniSessionEvent>, reducer: SessionReducer, markdown: MarkdownStream, composer: Composer, footer: Footer, queue: PromptQueue }`。
- `screens.rs:206` `draw_session` → `draw_session_with_state(frame, area, &SessionState)`，`SessionState { scrollback: Vec<HistoryLine>, composer: Composer, footer: FooterState, phase: SessionPhase }`。
- `Session` 屏激活时输入 `Enter`：`turn::build_agent_loop_params` + 注入 `MiniApprovalHandler`（复用 `mini.rs:1230`），`tokio::spawn` 驱动 `agent_execution::stream`（`mini.rs:1231` 同款），经 mpsc 回投 `MiniSessionEvent`。
- `event_loop` 消费 `rx`：`ExecutionStreamEvent` → `reducer::SessionReducer::push_batch` → `Vec<MiniCommit>` → `HistoryLine` 追加 `scrollback` + `FrameRequester::request_frame`；`LlmDelta` 经 `markdown::MarkdownStream` 按 top-level block 增量（复用 `markdown.rs` 已落地的 holdback/换行门控，未完结块不固化）。
- `footer.phase: Idle|Streaming|Approval|Question` 路由 `approval::ApprovalView` / `question::QuestionView`（复用 mini 的 handler 层）。
- 输入 `composer::Composer` 单行（P0），`Enter` 提交 → 经 `queue::PromptQueue` 串行/排队 `spawn_turn`；复用 `terminal::DoublePressTracker` 两按退出 + SIGINT 退出（`terminal.rs:443`）。

**验收**：`wf --tui` `Session` 屏可输入 prompt 并流式显示 LLM 回答与 `ToolStart/End` 的 `▲/✓/✗`；`Approval` 阶段 `y/a/d/n/c` 生效（对齐 `approval.rs`）；`Ctrl-C` 两按退出无 raw mode 残留；与 mini 输出同源（同一 reducer 产物）。

**风险**：① `draw` 内误用 `await` 阻塞帧 → `screens.rs` `draw` 保持纯同步，预取在 `tokio::spawn`（同 E1）；② 流式与 `markdown` 不一致 → 直接复用 `markdown.rs`，TUI 侧不自实现增量逻辑；③ `run.rs:226` `SessionRenderer` 是 headless sink 写入器，TUI **不复用其 IO 写入**，仅参考其事件消费形状。

---

### 阶段 E4 — 执行跟踪统一与过滤

**改动**：
- `screens.rs:196` `draw_executions` 增 `FilterBar { status, workflow_id, type }`（复用 `select::SelectList` 单行过滤器）。
- `ScreenDataCache::Executions` 存 `Vec<ExecutionSummary> + Filter`；`j/k` 导航 + `Enter` 下钻 `Session`（对齐 `execution inspect` 详情）。
- 过滤器变更时 `tokio::spawn` 重新聚合 `agent_loop_registry::summaries` + `list_executions`，去重后 `sort_by(created_at)`。

**验收**：`Executions` 按 `status` 过滤（`Running` 仅运行中）；`Enter` 下钻 `Session` 显示 timeline/iterations 的 audit 数据。

---

### 阶段 E5 — 重放与分页补载

**改动**：
- `replay.rs` 扩展 `ReplayState { scrollback, cursor, kind: Partial/Complete/LoadingBeginning }`（对齐 `history` §3.3/3.4）。
- `Session` 屏 scrollback 滚到顶部（`selected == 0` 且 `ScrollUp`）触发 `replay::replay_scrollback(ctx, cursor, limit=50)` cursor 分页补载，`cursor` 取最早 `HistoryLine` 的 `execution_id`；`replay.rs:12` `replay_scrollack` 改为 cursor 分页版。
- `TerminalGuard` resize 经 `size::ResizeDebouncer` 75ms 防抖后 `HistoryLine::reflow(width)` 按新宽度重算（复用 `scrollback.rs`）。

**验收**：长会话 `scrollback > 200` 行滚顶自动补载更早历史；`resize` 后 `reflow` 无截断。

---

### 阶段 E6 — 终端与性能硬化

**改动**：
- `tui.rs:70` `poll 100ms` → `framer::FrameRequester`（30–60fps 限帧）：reducer/markdown 产出时 `request_frame`，`should_draw(now)` 决定是否 `terminal.draw`，输入事件即时唤醒（对齐 ratatui-implementation §2.3）。
- `terminal.rs` 增 `SIGTSTP`/`SIGCONT`：`SIGTSTP` → `with_restored(None, || {})` 暂停 TUI 并记录 `ResumeAction`；`SIGCONT` → `reapply_raw_mode_after_resume`（重应用而非假设未变）+ 清输入残留 + 强制重查尺寸（learnings §6.5）。
- `theme.rs` `SIGUSR2` 经 `theme_reload_signals` mpsc 在 `event_loop` 的 `select!` 分支处理，`Theme` 变更后全量重绘。
- **退出防 failover**（learnings §6.3 P0）：`wf-runtime` 关闭路径标记 `active_shutdown`，用户主动退出/取消的执行关闭事件不派发 `Failed`（防退出瞬间闪错误行/后台重试）；headless 退出码语义已对齐（`lib.rs` 注释）。
- **输入边界**（learnings §3.5）：`TuiApp` 启动 + `with_restored` 恢复后 `discard_pending_terminal_input`。

**验收**：`cargo bench` reducer 万级事件 `<100ms`；TUI `scrollback 1000+` 行 `render <8ms/帧`；`SIGTSTP` 挂起终端状态干净，`fg` 恢复重绘；退出无 failover 闪烁。

---

## 五、依赖与顺序

```
E1 数据绑定 ─► E2 模态全量 ─► E3 会话流式 ─► E4 执行跟踪 ─► E5 重放分页 ─► E6 性能硬化
     │              │
     └──────────────┴── E1/E2 可并行部分（模态不依赖数据面）
```

- E1 为前置；E3 依赖 E1 数据面 + E2 模态；E6 最后。
- E3 是完整模式的价值核心，与 E1/E2 串行推进。

---

## 六、验证与验收策略

| 层 | 用例 |
| :--- | :--- |
| 纯函数 | `screens` 导航栈（已有）、`screens_data_cache_fetch` 的 `fetched_at` 过期单测；`modal` `oneshot` 单测；`reducer`/`markdown` `streamed_equals_full` 补 TUI 路径回归；`replay` 分页状态机 `Partial/Complete/LoadingBeginning` |
| 组件渲染 | `insta` 快照：`Session` `Phase::Streaming` / `Phase::Approval` 的 footer 形态；`FilterBar` 三状态（`All/Running/Failed`） |
| 形态冒烟 | 复用 `examples/mini_demo.rs` 合成 `ExecutionStreamEvent` 驱动 `wf --tui`；`tests/` 用 `FakeTerminal` 跑 `tui::tests::tui_smoke` |
| 集成 | `tests/tui_screens.rs`（`DomainAdapter` 内存存储 + 预置 workflow/execution → `TuiApp` `fetch` 断言） |
| 性能 | `benches/tui_render.rs`（scrollback 1000 行 `draw` 耗时） |
| 合规 | `cargo clippy --all-targets --all-features`；`cargo test -p wf-cli`；`cargo fmt` |

> 遵循 `AGENTS.md`：代码注释只用英文、不引用文档结构标识符；规划文档用中文、避免完整代码片段（上文仅列接口锚点与最小示意）；模块保持 lib.rs 扁平声明、无 `mod.rs`。

---

## 七、待拍板 / 开放点（如实记录，不占位）

1. **E3 审批 handler**：复用 mini 的 `MiniApprovalHandler`，还是为 TUI 新建 `TuiApprovalHandler`？建议**复用同一 handler 层**（handler 与视图解耦，视图层各自渲染）。
2. **退出防 failover 落点**：`active_shutdown` 标记在 `wf-runtime` 关闭路径还是事件 dispatch 层打？需与 runtime owner 对齐（learnings §9.3 跨形态一致性）。
3. **历史 `raw_lines` 复制视图**（learnings §5.1 P0）：是否 Stage 7 一并做，还是延后 Stage 8 重放？建议 Stage 7 先定型 `HistoryLine` 三通道（`display/raw/height`），复制视图延后。
4. **转录浮层（codex `pager_overlay`）**：`Ctrl+T` 全屏转录查看器是否 Stage 7 做？建议延后到 Stage 8 或 P1。
5. **多行输入框**：E2/E3 用 `composer::Composer` P0 单行是否够？`ratatui-textarea` 多行（learnings §9.2 路线分歧）建议 P1 评估，先不引入。
6. **构建环境前置**：当前 sandbox 未装 `libluajit-5.1-dev`（`wf-sandbox` 默认 `lua-mlua-sandbox` feature 的编译前置）。编译 `wf-cli` 前需 `apt-get install -y libluajit-5.1-dev`（或 workspace 关闭该默认 feature）。

---

## 八、与既有文档关系

本文细化 `docs/plan/cli/wf-cli-剩余问题-TUI深化.md` 的 E1–E6，补充：
1. **精确文件:行锚点**——基于 2026-09-06 实际代码（原文档多为规划态占位锚点）；
2. **Codex 设计映射**——把 7 篇 codex 分析与 `wf-agent-learnings.md` 借鉴清单落到具体阶段；
3. **learnings 未吸收 P0 项并入**——退出防 failover（§6.3）、输入边界（§3.5）、挂起重应用 raw mode（§6.5）、`raw_lines` 视图（§5.1）分别挂到 E3/E6/§七；
4. **复用清单**——明确 Stage 0–6 已验证资产（reducer/markdown/footer/composer/queue/terminal/framer/`cmd/`）在 TUI 侧直接移植，避免重复造轮子。

> 说明：markdown 流式正确性（表 holdback/换行门控/定稿兜底）等 P0 项已在 Stage 6 落实（`markdown.rs:303` `TableHoldback`、`:236` 换行门控、`:776` 测试），故 Stage 7 不再重述，仅要求 E3 复用并补 TUI 路径回归测试。
