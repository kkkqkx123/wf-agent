# wf-cli 完整模式（全屏 TUI / Stage 7）现状复核与细化修改方案

> 文档属性：现状复核 + 细化修改方案
> 复核基准：2026-09-06 `kkkqkx123/wf-agent` 当前 HEAD 源码（`crates/app/wf-cli/src/` 64 个 `.rs` 文件，约 2.2 万行）
> 取代/补充对象：`docs/plan/cli/wf-cli-stage7-全屏TUI完整模式-细化实施方案.md`（2026-09-06 版，所引锚点多属"占位态"，已被后续提交落地）
> 关联：`docs/plan/cli/wf-cli-分阶段实现方案.md`、`wf-cli-剩余问题-TUI深化.md`、`docs/analysis/` 的 codex 模块分析与 `wf-agent-learnings.md`

---

## 〇、修订说明（为什么需要这份文档）

2026-09-06 版的 stage7 细化方案基于"占位态"代码撰写，断言：E1 八屏全为静态占位文本、E2 模态仅 2 个且无 `push_with_result`、E3 `draw_session` 为占位、E5 全量重建、E6 仍 `poll 100ms`。**经逐文件核验，这些断言已与实际代码不符**——完整模式（全屏 TUI）的主体（E1–E4）已经正式实现并接线。本文以真实代码为准重新盘点，并将剩余缺口收敛为可执行的细化修改方案。

> 环境限制（如实记录，未跳过）：本沙箱 `static.rust-lang.org` 被墙、且未装 `libluajit-5.1-dev`，`cargo check -p wf-cli` 无法执行。所有"已实现"结论来自源码阅读，非编译产物。已全局配置 rsproxy 镜像（crates + rustup 两处），编译验证需在能下载工具链并 `apt-get install -y libluajit-5.1-dev` 的环境进行（见 §6）。

---

## 一、结论先行

| 阶段 | 内容 | 真实状态 | 事实锚点 |
| :--- | :--- | :--- | :--- |
| E1 | 屏幕数据绑定（静态→真数据） | ✅ **已正式实现** | `screens.rs` 全部 `draw_*` 已接 `ScreenData`；`tui.rs:637 fetch_for` 真实调用 `wf_api` |
| E2 | 模态栈全量 + oneshot | ✅ **已正式实现** | `modal.rs` 含 8 个模态 + `ModalStack::push_with_result`(82) + `is_transparent`(40) |
| E3 | 会话屏流式联调 | ✅ **已正式实现** | `session.rs::SessionController` 完整移植；`tui.rs:268` 路由 `session.draw` |
| E4 | 执行跟踪统一与过滤 | ✅ **已正式实现** | `tui.rs:716 fetch_executions` 带 `ExecStatusFilter`；Enter→replay 下钻(439-448) |
| E5 | 重放与 cursor 分页补载 | ❌ **未做** | `replay.rs:12 replay_scrollack` 仍是全量重建，无 `Partial/Complete/LoadingBeginning` 状态机 |
| E6-a | 帧调度接入 `FrameRequester` | ❌ **未做** | `tui.rs:183` 仍是 `event::poll(POLL_INTERVAL)`（100ms）；`framer.rs` 已实现但未被消费 |
| E6-b | SIGTSTP/SIGCONT 挂起恢复 | ❌ **未做** | `terminal.rs` grep 无 `SuspendContext`/`SIGTSTP`（仅 `DoublePressTracker` 443） |
| E6-c | resize 防抖 + scrollback reflow | ❌ **未做** | `size.rs:30 ResizeDebouncer` 已实现但未被 `tui.rs` 消费；`tui.rs:199-201` resize 分支为空 |
| E6-d | SIGUSR2 主题热更新接入事件循环 | ❌ **未做** | `theme.rs:543 theme_reload_signals` 已实现但 `tui.rs` 未消费；`session.rs:576` 用 `fallback_theme()` 而非缓存主题 |
| E6-e | 退出防 failover（`active_shutdown`） | ❌ **未做（跨 crate）** | 全仓 grep 无 `active_shutdown`；`tui.rs:207` 仅用通用 `is_shutting_down()` |
| 质量-1 | `screens.rs:433 draw_session` 死代码 | ⚠️ **待清理** | `tui.rs:268` 已绕开它走 `session.draw`，该占位函数永不执行 |
| 质量-2 | `session.rs:631 now_ms` 时钟错误 | ⚠️ **待修正** | 返回"距 `last_frame` 毫秒差"且每帧重置，但 `footer.set_now` 把它当单调时钟用（见 §4.7） |

**一句话结论**：完整模式"能跑通"的主体（8 屏数据、模态、会话流式、执行过滤/下钻）已正式实现；剩余工作集中在 **E5 重放分页** 与 **E6 终端/性能硬化** 两块，外加两处代码质量修正。下面给出精确锚点与最小 diff。

---

## 二、已落地模块详解（证明"正式实现"，供复核）

### 2.1 E1 屏幕数据（`screens.rs` + `tui.rs`）
- `screens.rs:14 ScreenKind` 八屏枚举；`:287 ScreenData` 聚合 `Dashboard/Workflow/Executions/Checkpoints/Search/Settings`；各 `draw_*`(362/387/402/450/462/508) 均从 `ScreenData` 取数渲染，`Empty - no records yet.` 与 `Loading...` 分支齐备。
- `tui.rs:126 run` 进入 alt-screen（`TerminalGuard::enter(TerminalModes::TUI)`）；`:293 request_data` + `:318 tokio::spawn(fetch_for)` 异步取数，`DATA_TTL=5s`(38) 缓存；`:332 drain_data` 折叠回 `data: HashMap<ScreenKind,(ScreenData,Instant)>`。
- 导航：`digit_to_screen`(873) 映射 `1-8`；`j/k` 选择；`?,q,Esc` 全局键（`:410 handle_key`）。

### 2.2 E2 模态栈（`modal.rs`）
- `Modal` trait(30) 含 `is_transparent()`(40，默认不透明)；`ModalStack::push_with_result`(82) 返回 `oneshot::Receiver<ModalResult>`；`HandleKey` 返回 `Some(result)` 时经 `sender.send` 回投（`:128` 透明判断、`ModalStack::handle_key`）。
- 已落地 8 个模态：`ConfirmModal`(252)、`HelpModal`(298, `is_transparent` 返回 true 见 305)、`FileViewer`(335)、`DiffViewer`(433)、`ModelPicker`(614)、`SessionPicker`(657)、`PasswordModal`(704)、`FileSelectionDialog`(787)。远超 02 文档要求的 6+。
- 接线：`tui.rs:533` 删除工作流走 `ConfirmModal` + oneshot；`:590 ModelPicker` 切换默认 `llm_profile`。

### 2.3 E3 会话屏（`session.rs` + `turn.rs`）
- `SessionController`(140) 复用 mini 管线：`turn.rs:67 stream_agent_turn` → `wf_api::agent::agent_execution::stream`；`reducer::SessionReducer::push_batch`（`:303`）；`markdown::MarkdownStream` 增量（`:330` `stream.push` + `committed_upto` 边界控制）；`Footer`/`ApprovalView`/`QuestionView`（`:284/:292`）；`DoublePressTracker` 两按退出（`:442`）。
- 领域 handler 与视图解耦：`TuiApprovalHandler`(64)/`TuiInteractionHandler`(107) 经 `mpsc` 把审批/追问需求回投到 `SessionEvent`，UI 侧渲染（与 stage7 开放点 #1 建议一致：**复用同一 handler 层、视图各自渲染**）。
- `draw`(574) 三区布局 scroll/footer/input；`draw_scrollback`(594) 每帧 `display_lines(width)` 自动 reflow，尾部锚定。

### 2.4 E4 执行跟踪（`tui.rs`）
- `fetch_executions`(716) 用 `AgentExecutionFilter` + 合并 `agent_loop_registry` 统一时间线；`ExecStatusFilter`(203) 六态，`f` 键循环(451)。
- Enter 下钻(439)：取选中 `exec.id` → `goto(Session)` + `pending_replay=id` → `apply_pending_replay`(248) → `session.load_replay(id)`。

---

## 三、真实缺口与细化修改方案

### 4.1 E5 — 重放 cursor 分页补载（`replay.rs` / `session.rs`）

**现状**：`replay.rs:12 replay_scrollack(ctx, session_id)` 一次性全量拉取 `by_agent_loop`/`by_execution` 全部消息并转 `HistoryLine`。长会话会被 `SessionController::settle_scrollback`(429) 的 `MAX_SCROLLBACK=10_000` 直接截断头部，且 `load_replay`(209) 无"滚到顶部补更早历史"能力。codex `history` §3.3/3.4 的 `Partial/Complete/LoadingBeginning` 未落地。

**方案**：
1. `replay.rs` 新增分页加载器（以"早于某游标的时间戳"为前滚条件，对齐 `history` §3.3）：

```rust
// 新增：按游标分页补载（替代纯全量重建）
pub enum ReplayState { Partial, Complete, LoadingBeginning }
pub async fn replay_scrollack_page(
    ctx: &ApiContext, session_id: &str,
    before: Option<i64>, limit: usize,
) -> Result<(Vec<HistoryLine>, ReplayState), ApiError> { /* ... */ }
```

2. `SessionController` 增加 `replay_cursor: Option<i64>` 与 `replay_state: ReplayState`；`draw_scrollback` 检测"选中行在顶部且 `ScrollUp` 且 `state==Partial`"时调用 `replay_scrollack_page` 前插，并置 `LoadingBeginning` 防重入。
3. `load_replay`(209) 改为首屏 `replay_scrollack_page(ctx, id, None, 200)`；保留 `replay_scrollack` 全量版供 `--tui --demo`/测试使用（不删除，避免破坏既有测试）。

**验收**：scrollback > 200 行滚顶自动补载更早历史；`ReplayState` 状态机单测（`Partial→LoadingBeginning→Partial/Complete`）；`streamed_equals_full` 不回归（仅数据源变化）。

### 4.2 E6-a — 帧调度接入 `FrameRequester`（`tui.rs` + `framer.rs`）

**现状**：`tui.rs:183` 固定 `event::poll(POLL_INTERVAL)`（100ms，`:42` 定义）。`framer.rs:51 FrameRequester`（`request_frame`/`request_frame_in`/`frame_done`）+ `:19 FrameRateLimiter` 已实现且单测完备，但 `tui.rs` 完全未引用（grep 确认）。

**方案**：
1. `TuiApp::run`(126) 创建 `let mut framer = FrameRequester::new(now_ms());`。
2. `event_loop`(165) 改为：**输入事件即时 `framer.request_frame()`**；`drain_data`/`handle_key`/`session.handle_events` 产生变更处 `request_frame`；渲染前 `if framer.should_draw(now) { terminal.draw(...) }`；绘制后 `framer.frame_done()`。
3. `poll` 保留为"无界等待输入"语义（或改为短超时轮询 `framer.deadline()`），输入到达立即唤醒——对齐 ratatui-implementation §2.3 的"输入即时唤醒 + 限帧批量"。

```rust
// tui.rs event_loop 核心改造示意（最小 diff）
let now = now_ms();
if framer.should_draw(now) {
    self.draw(...); framer.frame_done();
}
if event::poll(framer.poll_timeout()).await? { /* read key -> request_frame */ }
```

**验收**：`scrollback 1000+` 行 `render <8ms/帧`（复用 `benches/tui_render.rs`）；输入无延迟；`cargo clippy` 无未使用告警（`FrameRequester` 被消费）。

### 4.3 E6-b — SIGTSTP/SIGCONT 挂起恢复（`terminal.rs` + `tui.rs`）

**现状**：`terminal.rs` 仅有 `DoublePressTracker`(443)/`with_restored`/`install_panic_hook`，无 `SuspendContext`/`SIGTSTP`/`SIGCONT`。Ctrl-Z 挂起会破坏 alt-screen/raw mode 状态。

**方案**：
1. `terminal.rs` 新增 `SuspendContext`：`SIGTSTP` 时 `with_restored(None, || {})` 暂停 TUI 并记录 `ResumeAction`；`SIGCONT` 时 `reapply_raw_mode_after_resume`（重应用而非假设未变）+ 清输入残留 + 强制重查尺寸（learnings §6.5）。
2. `tui.rs::run` 注册该 signal handler；`SIGCONT` 后 `terminal.clear()` + 强制 `request_data(current)` 重拉。

```rust
// terminal.rs 新增骨架
pub struct SuspendContext { /* raw/alt-screen saved flags */ }
impl SuspendContext {
    pub fn install() -> io::Result<Self> { /* SIGTSTP -> suspend, SIGCONT -> resume */ }
}
```

**验收**：`Ctrl-Z` 挂起后终端状态干净，`fg` 恢复重绘；复用 `examples`/PTY 冒烟（对齐 stage3 的 `debug-terminal` 思路）。

### 4.4 E6-c — resize 防抖 + scrollback reflow（`tui.rs` + `size.rs`）

**现状**：`size.rs:30 ResizeDebouncer`（`settle_if_elapsed` 75ms 窗）已实现且单测完备，但 `tui.rs:199-201` 的 `Event::Resize` 分支为空（仅下一轮重绘）。因 `session.rs:612 display_lines(width)` 每帧按当前宽度重算，reflow 实际上已自动发生；缺口是**缺少 75ms 防抖**与"resize 后强制重拉/重排"的显式钩子。

**方案**：
1. `TuiApp` 持有 `resize: ResizeDebouncer`；`Event::Resize` 分支改为 `resize.push(Size::from(event), now)`。
2. `event_loop` 每轮 `if let Some(size) = resize.settle_if_elapsed(now) { terminal.resize?; request_frame(); /* 若需强制重排可置 session reflow 标记 */ }`。
3. 因 `HistoryLine` 为内容存储型，无需额外 reflow 逻辑；仅保证 `FrameRequester` 在 resize 后触发重绘（与 4.2 共用）。

**验收**：连续 resize 风暴下渲染不超过每 75ms 一次；无截断（复用 `scrollback.reflow` 既有语义）。

### 4.5 E6-d — SIGUSR2 主题热更新接入事件循环（`tui.rs` + `theme.rs`）

**现状**：`theme.rs:543 theme_reload_signals()` 已实现（返回 `mpsc::Receiver<()>`），但 `tui.rs` 未消费；`session.rs:576 draw` 用 `fallback_theme()`（静态降级主题），未用 `theme_reload_signals` 探测结果，主题变更不生效。

**方案**：
1. `TuiApp` 新增字段 `theme: Theme`，`run`(126) 初始化为 `crate::theme::load_theme_cache().unwrap_or_else(fallback_theme)`。
2. `run` 内 `tokio::spawn` 监听 `theme_reload_signals()`，收到后 `probe_theme()` 重探并 `tx.send(Feedback::ThemeChanged)`（新增 `Feedback` 变体）。
3. `event_loop` 的 `feedback_rx` 分支处理 `ThemeChanged` → 更新 `self.theme` → `request_frame()`。
4. `session.draw`(574) 签名增加 `&Theme` 参数，`tui.rs:270` 传入 `&self.theme` 替代 `fallback_theme()`。

**验收**：运行中 `kill -USR2 <pid>` 后 TUI 配色即时更新；非 TTY 降级不 panic；`theme` 字段被使用（`clippy` 无 dead_code）。

### 4.6 E6-e — 退出防 failover（`active_shutdown`，跨 wf-runtime，开放）

**现状**：全仓无 `active_shutdown`。`tui.rs:207` 仅用 `adapter.is_shutting_down()`（通用运行期标志）。codex `ExitMode::ShutdownFirst`/`pending_shutdown_exit_thread_id` 语义（learnings §6.3 P0）未吸收：用户两按退出/`/quit`/SIGINT 主动关闭的执行，仍可能被 runtime 派发 `Failed`，导致退出瞬间闪错误行或后台重试。

**方案（待与 runtime owner 对齐落点）**：在 `wf-runtime` 关闭路径标记 `active_shutdown`；`wf-api`/`wf-agent` 的事件 dispatch 在 `active_shutdown==true` 时不派发 `Failed`。`TuiApp::run`(126) 退出前 `session.shutdown()` + `adapter.shutdown()` 已是主动关闭场景，应打标。本文件**不代为修改 wf-runtime**，仅列为开放点（见 §5）。

### 4.7 代码质量修正

**质量-1：`screens.rs:433 draw_session` 死代码。** `tui.rs:268` 在 Session 屏直接调 `session.draw`，`Screens::draw` 的 `ScreenKind::Session => draw_session` 分支(159) 永不触发。`draw_session` 的占位文本会误导阅读。
- 处置：删除 `screens.rs:433-448 draw_session` 函数及 `Screens::draw` 中 `ScreenKind::Session` 分支（保留 `ScreenKind::Session` 变体本身，它仍是导航/数据路由键）。

**质量-2：`session.rs:631 now_ms` 时钟错误。**
- 现状：`now_ms` 返回 `Instant::now().duration_since(self.last_frame).as_millis()`，而 `draw`(575) 开头 `self.last_frame = Instant::now()`——即每帧返回 ≈0 的"距本帧起"毫秒差。但 `footer.set_now(now_ms)`(`footer.rs:210`) 把它当**单调时钟**用于 notice 过期(`:289` `now_ms + NOTICE_TTL_MS`)与 spinner 动画(`:411` `spinner_frame(now_ms)`)，导致状态栏计时/动画错乱。
- 修正：新增 `started_at: Instant`（在 `start`(161) 设为 `Instant::now()`），`now_ms` 改为 `Instant::now().duration_since(self.started_at).as_millis() as u64`；删除 `draw` 中对 `last_frame` 的误用（或保留 `last_frame` 仅作"距上次重绘"统计，不与 `now_ms` 耦合）。

```rust
// session.rs 最小 diff
// 新增字段
started_at: Instant,
// start() 中
started_at: Instant::now(),
// now_ms 改为
fn now_ms(&self) -> u64 {
    Instant::now().duration_since(self.started_at).as_millis() as u64
}
// draw 中删除 `self.last_frame = Instant::now();`（或仅用于独立统计）
```

---

## 五、待拍板 / 开放点（如实记录，不占位）

1. **E3 handler 复用**：`TuiApprovalHandler` 已新建（与 mini 的 `MiniApprovalHandler` 并列），符合"handler 层共享、视图各自渲染"。是否进一步合并为单 `CliApprovalHandler` 由 `CliMode` 参数化？建议暂保持两者分离（TUI 需 `mpsc` 回投到 `SessionController`，mini 走自身事件循环），后续若发现重复再抽公共 trait。
2. **E6-e 落点**：`active_shutdown` 标记应加在 `wf-runtime` 关闭路径还是事件 dispatch 层？需与 runtime owner 对齐（learnings §9.3 跨形态一致性）。本文不代为改动 wf-runtime。
3. **E5 游标语义**：长会话分页前滚以"时间戳游标"还是"execution_id + seq 游标"？建议先按 `message.timestamp` 降序分页（对齐 `replay.rs:46` 既有 `sort_by_key(timestamp)`），后续若消息无序再引入 seq。
4. **转录浮层**（codex `pager_overlay`）：`Ctrl+T` 全屏转录查看器是否 Stage 7 做？建议延后 Stage 8 / P1。
5. **多行输入框**：`composer.rs` P0 单行是否够？`ratatui-textarea` 多行（learnings §9.2 路线分歧）建议 P1 评估，先不引入。
6. **构建环境前置（必做）**：编译 `wf-cli` 前需 (a) rustup 镜像可用（已配 rsproxy，见 §0）；(b) `apt-get install -y libluajit-5.1-dev`（`wf-sandbox` 默认 `lua-mlua-sandbox` feature 编译前置，stage7 原开放点 #6 仍成立）。
7. **`remote.rs`(639 行) 与完整模式关系**：当前 `run_interactive` 的 TUI 分支走 `Embedded` 路径；`Remote`（远程会话客户端）是否要在完整模式提供"连接远程 agent"入口？本文未涉及，建议单独评估。

---

## 六、验证与验收策略

| 层 | 用例 | 当前可否执行 |
| :--- | :--- | :--- |
| 纯函数 | `ReplayState` 状态机单测；`now_ms` 单调性单测；`FrameRequester` 已被消费；`ModalStack::push_with_result` 既有单测(1003) | 单测可跑（需先解决 §0 构建前置） |
| 组件渲染 | `Session` `Phase::Streaming/Approval` footer 快照（insta）；`FilterBar` 三态 | 同上 |
| 形态冒烟 | 复用 `examples/mini_demo.rs` 合成 `ExecutionStreamEvent` 驱动 `wf --tui`；`tests/` 用 `FakeTerminal` 跑 `tui::tests` | TTY 冒烟需 PTY，CI 走降级 |
| 集成 | `tests/tui_screens.rs`（`DomainAdapter` 内存存储 + 预置 workflow/execution → `TuiApp::fetch` 断言） | 同上 |
| 性能 | `benches/tui_render.rs`（`scrollback 1000` 行 draw <8ms）；reducer 万级事件 <100ms | 同上 |
| 合规 | `cargo clippy --all-targets --all-features`；`cargo fmt`；`cargo test -p wf-cli` | **当前沙箱不可执行**（工具链下载被墙 + 缺 libluajit） |

> 构建前置达成后，本方案 §4.1–§4.7 的每一项均应有对应单测/集成断言；`draw_session` 删除后需确认无 `dead_code` 告警（`clippy`）。

---

## 七、与既有文档关系

- 取代 `docs/plan/cli/wf-cli-stage7-全屏TUI完整模式-细化实施方案.md`（2026-09-06）中已过时的"E1–E4 未做"断言；保留其 Codex 借鉴映射（§三）与开放点框架，按真实代码更新。
- 补 `wf-cli-剩余问题-TUI深化.md` 的 E1/E2/E3 由"规划"升级为"已落地"（基于源码核验）。
- 遵循 `AGENTS.md`：代码注释仅英文、不引用文档结构标识符；本文为中文规划、仅列最小示意 diff 与精确锚点；模块保持 `lib.rs` 扁平声明、无 `mod.rs`。

> 说明：完整模式的流式正确性（表 holdback / 换行门控 / 引用链接回退 / 定稿兜底）已在 Stage 6 落实于 `markdown.rs`（`:303 TableHoldback`、`:236` 换行门控、`:776` 测试）。E3 直接复用 `markdown.rs`，不重复造轮子——本方案仅在 §4.5 要求 `session.draw` 改用缓存 `Theme`，与流式内核无关。

---

## 八、二轮执行：决策与实施结果（已完成）

### 8.0 环境前置已在本沙箱解决（推翻 §0 与 §六 的"无法编译"记录）

- `libluajit-5.1-dev` 已通过 `apt-get update && apt-get install -y libluajit-5.1-dev` 装上（`pkg-config luajit` 返回 2.1.x）。
- rustup 镜像随命令注入：`RUSTUP_DIST_SERVER=https://rsproxy.cn`、`RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup`（否则 rustup 仍去 `static.rust-lang.org` 同步 channel 而失败）。
- 结果：本沙箱内 `cargo check -p wf-cli`、`cargo test -p wf-cli` 均可执行，"沙箱不可执行"限制**已解除**，下述结论全部经真实编译验证。

### 8.1 开放点决策记录

| 开放点 | 决策 | 理由 |
| :--- | :--- | :--- |
| E5 游标语义 | 按"整段加载 + 后台任务 + Loading/Complete 状态机"落地；**不引入 `Partial`/时间戳游标**，等存储层提供 continuation token 再补 | `replay_scrollack` 无分页 API，强行造游标属伪造；状态机已留好替换点（`ReplayLoaded` 事件整体替换 `scrollback`） |
| E6-b 挂起实现方式 | 完全镜像 `mini.rs` 已验证模式（静态 `SUSPEND_PENDING` + async-signal-safe handler + 事件循环内 restore / `raise(SIGTSTP)` / 重入） | 不自造新模式，复用 mini 模式已跑通的行为 |
| E6-e 退出防 failover | **暂缓**，不改动 `wf-runtime` | `is_shutting_down()` 已在事件循环末尾消费，会话任务在 `run()` 清理段 `abort()+await`；跨 crate 的 `active_shutdown` 需 runtime owner 对齐，单方面加标志位是伪需求 |
| E3 handler 合并 | **不合并** | `TuiApprovalHandler` / `TuiInteractionHandler` 职责已单一，合并反而耦合 |
| `remote.rs` 与完整模式 | **不纳入本轮**，维持 `Embedded` 路径 | 远程会话客户端是独立特性，与本轮硬化无关 |
| 模态主题热更新 | 仅 `session.draw` 接活跃主题；模态仍走内部回退主题（预存行为） | 模态 `draw` 签名不带 theme，改签名波及 8 个模态，收益低，另行评估 |

### 8.2 实施结果（改动文件与要点）

| 文件 | 改动 |
| :--- | :--- |
| `session.rs` | ① `last_frame` → `origin`，`now_ms()` 改为自 `origin` 起算的单调毫秒（修正质量-2：notice 过期、spinner 旋转、双击退出全部恢复正确时序）；② `draw(frame, area)` → `draw(frame, area, theme: &Theme)`，去掉每帧 `fallback_theme()`（E6-d）；③ 新增 `ReplayPhase{LoadingBeginning, Complete}` 与 `SessionEvent::ReplayLoaded(Vec<HistoryLine>)`，`load_replay` 改为同步入队：立即显示 `▦ Loading history for {id}…` 占位行，后台任务拉取完成后整页替换（E5，事件循环不阻塞） |
| `screens.rs` | 删除死代码 `draw_session`（原 433-448）及其 `ScreenKind::Session` 分发臂（改为显式 no-op 注释；Session 屏由 `SessionController::draw` 渲染）（质量-1） |
| `tui.rs` | ① 接入 `FrameRequester`（120FPS 限速）+ `dirty` 标记：空闲屏不再每 100ms 无条件重绘，键盘 / 数据 / notice / 会话流才触发，poll 超时由 `deadline()` 推导，不忙转（E6-a）；② `Event::Resize` 接 `ResizeDebouncer`（75ms 防抖），settle 后才强制一次 reflow（E6-c）；③ SIGUSR2 主题热更新接入事件循环（后台任务 re-probe → channel → `self.theme` → 传入 `session.draw`）（E6-d）；④ 新增 `SUSPEND_PENDING` + `sigtstp_handler` + `check_suspend()`：Ctrl-Z 挂起/恢复（restore → `raise(SIGTSTP)` → 重入 TUI → 重新查询几何 → clear → 强制重绘）（E6-b）；⑤ `drain_data` 返回 `bool` 供 dirty 判定；⑥ 新增 `expire_notice()`，防止 notice 槽位过期后长期占用导致无谓重绘 |

### 8.3 验证结果（真实执行）

| 命令 | 结果 |
| :--- | :--- |
| `cargo check -p wf-cli` | ✅ `Finished dev profile`；wf-cli 自身 0 warning（仅预存 `wf-shell` 的 `private_interfaces` 警告，与本轮无关） |
| `cargo test -p wf-cli` | ✅ **315 passed + 4（mini_pipeline）passed，0 failed**，覆盖 `tui::tests`、`theme::tests`（SIGUSR2）、`framer` / `size`、`replay`、`domain` e2e |

### 8.4 运行时未覆盖项（如实记录）

本沙箱无真实 TTY/PTY，以下仅由编译与单测保证，**无法端到端冒烟**，需在开发板上过一遍：

1. Ctrl-Z 挂起 → shell 可用 → `fg` 恢复 → 画面完整重绘；
2. 拖拽窗口：resize 风暴期间无高频重绘，松手后一次性以新宽度 reflow；
3. SIGUSR2 发一次：主题立即切换（Session 屏 footer 颜色随之变化）；
4. 长 history 会话 replay：先见 `▦ Loading…` 行，完成后整体替换、无阻塞卡顿；
5. 会话屏流式期间 spinner 按 40ms 旋转、notice 3s 后消失（依赖修正后的单调 `now_ms`）。

### 8.5 交付物

- 源码改动：`session.rs` / `screens.rs` / `tui.rs`（详见 §8.2）
- Patch 文件：`/workspace/wf-agent-changes.patch`（含 §8.2 三文件 + 本文档；`git apply` 即可落到本地仓库，仅含源码与文档变更，无构建产物目录）
