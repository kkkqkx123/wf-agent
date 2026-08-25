# wf-cli Stage 6 实施方案：mini 模式（形态落地 1）

> 状态：已完成（阶段 6A 已完成 2026-08-20；阶段 6B 已完成 2026-08-22；6C–6E 已完成 2026-08-24；事件架构修订 2026-08-24）
> 上游方案：`docs/plan/cli/wf-cli-分阶段实现方案.md`（Stage 6 任务定义）、`docs/cli/05-opencode-mini模式与无头模式设计.md`（mini 模式设计 §3.1-§3.4/§四）、`docs/cli/03-组件设计方案.md`（组件清单/事件循环/输入）、`docs/cli/04-终端交互设计.md`（Keymap 上下文）
> 对照参考：`/workspace/opencode-mini-tui.md`（架构总览）、`/workspace/opencode-mini-features.md`（功能清单）、`/workspace/opencode-mini-rendering.md`（渲染逻辑）——三份 opencode `--mini` 模式源码分析；`/workspace/docs/analysis/wf-agent-learnings.md`（codex TUI 四模块设计思想对照，§三/§四/§五/§六 为本文档 D14-D20 的出处）
> 范围：`wf --mini` inline split-footer 交互会话的完整落地——渲染底座（ratatui `Terminal` + `Viewport::Inline`）、footer 视图栈（composer/面板/审批/问题 + statusline）、MiniSink 输出模型、串行 turn + 排队、两按退出、审批/追问交互接线、`examples/mini_demo.rs` 管线展示。**Stage 6 是第一个消费 Stage 5 reducer/markdown 产物的 UI 形态**（mini footer 与 Stage 5 无头摘要渲染器消费同一 commit 流）。

> **2026-08-24 事件架构修订**（本文正文中 `UnifiedEvent`/`events.rs`/`--demo`/`demo.rs`/`ExecutionStreamEvent::Agent` 的历史表述以本节为准）：
> 1. **执行流协议是引擎无关的，定义且仅定义在 wf-api**。正确分层：**客户端层**（wf-cli）只消费 wf-api 的 `ExecutionStreamEvent`，自有 UI 状态派生（reducer → `MiniCommit`/`FooterState`），禁止定义或导入事件协议类型；**协议层**（wf-api）定义自包含的执行流协议——`Engine(BaseEvent)`（总线生命周期事件）、`IterationStart/IterationEnd`、`LlmDelta`、`ToolStart/ToolEnd`、`Interrupted`（类型化执行进度，扁平枚举）、`Completed/Failed`（终态），并在 `from_agent_stream` 完成"引擎事件 → 协议事件"适配；**引擎层**（wf-agent ∥ wf-workflow，对等引擎）各自持有引擎内部事件模型（wf-agent 的 `AgentStreamEvent` 是 agent loop 自身的事件模型，引擎内部命名准确，保留），引擎词汇不得出现在协议中。
> 2. **`agent` 概括执行进度是错误命名**。agent 与 workflow 对等；LlmDelta/ToolStart/ToolEnd/迭代边界是**执行级**进度语义，不专属任何引擎。原 `ExecutionStreamEvent::Agent(AgentStreamEvent)` 变体内嵌引擎实现类型，使协议依赖单个引擎 crate、CLI 被迫导入引擎类型做匹配——已删除，进度事件扁平提升为协议变体（`Completed { iterations }` 本就以 iterations 为协议词汇，迭代边界入协议与之自洽）。CLI 侧历史中间层 `src/events.rs`（`UnifiedEvent`/`AgentEvent`，对协议的冗余副本）此前已删除。
> 3. **demo 不属于 src**。`src/demo.rs` 与 `--demo` 参数删除；管线展示迁至 `examples/mini_demo.rs`（`cargo run -p wf-cli --example mini_demo`，仅合成协议事件驱动同一 reducer/footer/markdown/审批/问题管线——客户端无需知晓引擎事件类型），集成断言在 `tests/mini_pipeline.rs`。
> 4. 工具时长是客户端观测（run.rs / mini.rs 各自 `HashMap<tool_call_id, Instant>` 计时，协议不携带 duration）；`MiniCommit::ToolEnd` 因此不再有 `duration_ms` 字段。

***

## 一、现状与缺口

### 1.1 Stage 0-5 已交付的设施（Stage 6 直接消费，不再重复实现）

| 设施                                                                                  | 模块                | 交付点     | Stage 6 消费方式                                      |
| :---------------------------------------------------------------------------------- | :---------------- | :------ | :------------------------------------------------ |
| `UnifiedEvent` 统一事件 + `unified_from_execution_stream`                               | `events.rs`       | Stage 1 | 会话事件流 → reducer 的输入                               |
| `DomainAdapter`（bootstrap/api\_context/shutdown）                                    | `domain.rs`       | Stage 1 | mini 会话持有 adapter，`agent_execution::stream` 驱动    |
| `OutputSink` 体系（`HeadlessFileSink`/`MemorySink`/`TeeSink`）                          | `output.rs`       | Stage 1 | `--log` 落盘经 `TeeSink` 复用；新增 `MiniSink`（§2.1 预留）   |
| `TerminalGuard`/`with_restored`/`TerminalStderrGuard`                               | `terminal.rs`     | Stage 3 | raw mode 进入、`/editor` 外部编辑器窗口、stderr 抑制           |
| `DoublePressTracker`（SIGINT 两按，5s 窗口）                                               | `terminal.rs`     | Stage 3 | 两按中断/退出接线                                         |
| `install_panic_hook`                                                                | `terminal.rs`     | Stage 3 | mini 启动即安装                                        |
| `Theme`（8 角色）+ `probe_theme` + `theme_reload_signals`（SIGUSR2）                      | `theme.rs`        | Stage 3 | footer/scrollback 配色 + 热更新                        |
| `HistoryLine`/`LinesView`/`Role`                                                    | `scrollback.rs`   | Stage 4 | scrollback 数据 + 渲染（`display_lines(width)` reflow） |
| `SelectList<T>`（分组滚动列表）                                                             | `select.rs`       | Stage 4 | 面板列表导航                                            |
| `Keymap`/`CKey`/`KeyAction`/`KeymapContext`                                         | `keymap.rs`       | Stage 4 | 键位解析；需补 mini 上下文绑定                                |
| `FrameRequester`/`FrameRateLimiter`                                                 | `framer.rs`       | Stage 4 | 渲染限帧 + 事件循环 deadline                              |
| `AnsiParser`（SGR 子集）                                                                | `ansi.rs`         | Stage 4 | 外部命令输出捕获入 scrollback                              |
| `ResizeDebouncer`（75ms）                                                             | `size.rs`         | Stage 4 | resize 合并 + reflow 触发                             |
| `SessionReducer`/`MiniCommit`/`FooterState`/`fold`                                  | `reducer.rs`      | Stage 5 | 事件 → commit 流 + footer 状态                         |
| `MarkdownStream`（streaming markdown，`code_lang` 接缝）                                 | `markdown.rs`     | Stage 5 | assistant 增量文本 → 滚动区                              |
| `HeadlessRenderer`（同源验证）                                                            | `render.rs`       | Stage 5 | 无头/mini 同源基准                                      |
| `agent_execution::stream` + `RunAgentLoopParams`（approval\_handler/agent\_loop\_id） | wf-api            | 既有      | mini 会话启动                                         |
| `llm_profile::list` / `SkillLoader` / `wf-api agent_user_interaction::respond`      | wf-api / wf-tools | 既有      | 面板数据源 + 追问回复                                      |

### 1.2 Stage 6 缺口

| #   | 缺口                                  | 说明                                                                                                                                                                                                             |
| :-- | :---------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1  | **mini 渲染底座不存在**                    | `lib.rs::run_interactive` 目前仅打印 `"[wf] Mini mode selected; the interactive renderer lands in a later stage"` 占位文本（lib.rs:134-138）。需要 ratatui `Terminal` + `Viewport::Inline(n)` + split-footer 布局 + tokio 事件循环 |
| G2  | **`MiniSink`** **已实现（6A）**          | 见 1.3；总方案 §2.1 预留的内存型 sink 已落地                                                                                                                                                                                 |
| G3  | **footer.rs 不存在**                   | 视图路由（prompt/permission/question + 面板 route）、`apply_height` 动态高度、statusline 宽度响应式、notice 机制、两按退出提示——opencode `RunFooter` 的对应物                                                                                   |
| G4  | **composer.rs 不存在**                 | P0 单行输入（grapheme 光标 + 水平滚动 + 历史 100 条）；P1 多行（≤6 行）+ `@` mention 区间高亮 + `/` 命令 palette                                                                                                                          |
| G5  | **panels.rs 不存在**                   | 模型/技能/排队面板（`llm_profile`、`SkillLoader`、排队队列数据源）                                                                                                                                                                |
| G6  | **approval.rs / question.rs 不存在**   | 审批视图（y/a/d/n/c 键位）与追问视图（单选/多选/自定义）；headless 侧的 deny 策略不能用于交互形态（05 §3.4 要求交互确认）                                                                                                                                 |
| G7  | **会话驱动未接线**                         | 串行 turn + 排队队列、`MiniApprovalHandler`/`MiniInteractionHandler` 注入 `RunAgentLoopParams`/`register_handler`、退出续跑提示（`wf --mini --session <id>`）                                                                    |
| G8  | **`--demo`** **最小版缺失**              | 合成事件流驱动完整管线冒烟（Stage 8 demo.rs 的 P0 前置）                                                                                                                                                                         |
| G9  | **args 扩展已完成（6A）**                  | `--session`/`--resume`/`--demo`/`-p`/`--agent`/`--model` + 互斥校验（含 `--demo` 要求 `--mini`）已落地                                                                                                                     |
| G10 | **keymap mini 上下文已完成（6A）**          | `KeymapContext::{Composer,Panel,Approval,Question}` + `KeyAction::DenyOnce` 已落地；6B 起事件循环接线消费                                                                                                                   |
| G11 | **事件循环编排不存在**                       | tokio `select!` 合并：crossterm 输入流、FrameRequester deadline、会话事件流 mpsc、SIGINT、SIGUSR2 主题热更新、resize 防抖——对齐 opencode `runtime.ts` 的编排职责                                                                             |
| G12 | **流式渲染正确性缺口（自 codex 分析引入，D14）**     | `MarkdownStream` 缺表格 holdback、严格换行门控、引用式链接定义全量回退；流定稿无"源码驱动全量重渲"兜底（codex `assert_streamed_equals_full` 语义）——mini 是第一个消费方，表格流式输出会列错位/闪烁                                                                          |
| G13 | **resize 流式宽度缺口（自 codex 分析引入，D15）** | `ResizeDebouncer` + `display_lines(width)` 已有，但"流式期间宽度更新 + 流定稿强制重排"缺失——宽度变化后 committed 前缀与 streaming 尾部不连续                                                                                                     |
| G14 | **输入/退出语义缺口（自 codex 分析引入，D16-D18）** | 缺用户文本 sanitize、输入边界丢弃早到输入、退出时"主动停止 ≠ 故障"领域语义、挂起/恢复（SIGTSTP/SIGCONT）                                                                                                                                            |

### 1.3 Stage 6A 已交付（2026-08-20，commit `78f5d51`）

| 设施         | 模块          | 交付内容                                                                                                                                                                                                                                                                         |
| :--------- | :---------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| mini 参数面   | `args.rs`   | `Cli` 增 `--session/--resume/--demo/-p/--prompt/--agent/--model`；`validate()` 增 `--demo` 要求 `--mini`、`--session/--resume` 互斥、交互选项与子命令/`--no-tui` 互斥；单测覆盖解析矩阵                                                                                                                  |
| mini 键位上下文 | `keymap.rs` | `KeymapContext::{Composer,Panel,Approval,Question}` + `KeyAction::DenyOnce`（`Approve/ApproveAll/Deny/Cancel` 沿用 Modal 预置）；composer（Enter=Submit/↑↓=History/Esc=Back/Ctrl+u=Clear）、panel（导航/Select/Back/Delete/Edit）、approval（y/a/n/d/c）、question（1-9/Enter/Esc）绑定表 + 上下文回退单测 |
| `MiniSink` | `sink.rs`   | `MiniOutputEvent { Text{role,content} / Message{role,content} / Chunk{content} }` + `MiniSink` 实现 `OutputSink`（`UnboundedSender` + flush=触发帧请求）+ `MiniSink::tee_log(path, format)`（`TeeSink` 组合）；单测（MemoryReceiver 断言 + Tee 落盘）                                              |

**验收（已达成）**：`cargo check -p wf-cli` 通过；args/keymap/sink 单测全绿；`wf --demo`（无 `--mini`）报参数错误。

***

## 二、opencode-mini 对照（三份分析文档要点 → Stage 6 落点）

### 2.1 总体架构对照

opencode mini 的**双车道模型**（`opencode-mini-tui.md` §3）：不可变 scrollback + 可变 footer，由 `screenMode: "split-footer"` + `footerHeight`（默认 4，动态 4~26+）实现；数据流为 `SDK 事件 → session-data reducer（纯函数）→ StreamCommit[] + FooterOutput → RunFooter.append()/event() → OpenTUI split-footer 渲染`。

wf-cli Stage 6 的对应：**数据管线已由 Stage 5 备齐**（`SessionReducer` + `MarkdownStream` + `MiniCommit`/`FooterState`），本阶段补上"footer 组件 + 渲染底座 + 编排"，完成从 `UnifiedEvent` 到终端画面的最后一公里：

```
UnifiedEvent 流 → SessionReducer（Stage 5，纯函数）→ MiniCommit[] + FooterState
  → MiniApp 事件循环（Stage 6 新增）→ footer 视图 + scrollback 更新
    → ratatui Terminal::draw（Viewport::Inline(n) 双缓冲 diff）
```

### 2.2 关键机制对照表

| opencode-mini 机制（分析文档出处）                         | opencode 实现要点                                                                                                                              | wf-cli Stage 6 对应                                                                                                                        | 现状            |
| :----------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------- | :------------ |
| **split-footer 渲染**（tui §3.1）                    | `CliRenderer` `screenMode: "split-footer"`，主屏不可变 scrollback + footer 动态高度                                                                  | ratatui `Terminal` + `Viewport::Inline(n)`（normal screen 底部 n 行，溢出自然滚入终端 scrollback）                                                     | 缺（G1）         |
| **capture-stdout**（tui §4.1）                     | `externalOutputMode: "capture-stdout"`，外部 stdout 同一帧内渲染                                                                                    | stderr 抑制（Stage 3 `TerminalStderrGuard`）+ 业务输出统一走 MiniSink → scrollback                                                                  | 半（G2）         |
| **footer 动态高度**（rendering §2）                    | `applyHeight()`：permission +12、question +14、面板按行数、prompt 随 textarea 1~6 行伸缩；仅实际变化才写 `footerHeight`                                         | `Footer::apply_height()` 高度表（05 §4.3：base+1 / base+clamp(rows,1,6) / base+16 / base+12 / base+14）；高度变化重建 `Viewport::Inline(n)`           | 缺（G3）         |
| **微任务合并**（tui §5.1 / rendering §3.1）             | `append` 队列合并连续同 part progress，`queueMicrotask` 一次 flush                                                                                   | mpsc 接收端批量 drain + `FrameRequester` 帧调度合并（Stage 4 已有）                                                                                    | 已有帧调度（G11 编排） |
| **视图状态机**（tui §5.2/§6.1）                         | `FooterView: prompt/permission/question` + `FooterPromptRoute: command/skill/model/variant/queued/subagent`；`present()` 切换 + `applyHeight` | `FooterView { Prompt, Permission, Question }` + `FooterRoute { Composer, Command, Model, Skill, Queued }`                                | 缺（G3）         |
| **statusline 宽度响应式**（features §8 / rendering §6） | 断点 80/66/120/150，窄终端隐藏 activity/model/context 区块；spinner 40ms；notice 3s + statusVersion                                                    | 断点 80/120（05 §附）；`[○/▶] [BUILD] wf agent · iter · msgs` + 右侧 ≥120 显示；spinner/notice                                                      | 缺（G3）         |
| **composer**（features §2 / rendering §7）         | 1\~6 行 textarea、环形历史 200（stash）、`@` mention（agents/files+`#行号`/resources）、`/` 命令、extmarks 区间标记                                             | P0 单行自研 Input（历史 100，03 §4.2）；P1 多行 + `@` mention（文件+行号/技能/工作流，区间高亮 `Vec<(Range, MentionKind)>`）+ `/` 命令                                 | 缺（G4）         |
| **命令/选择面板**（features §3 / rendering §5.3）        | 分组命令面板 + 模型/变体/子代理/排队/技能 5 面板；通用菜单交互（↑/↓、PgUp/PgDn、Home/End、Enter、Esc、ctrl+u 清过滤）                                                          | `SelectList<T>`（Stage 4 已有分组滚动）+ panels.rs 组装；键位走 Keymap                                                                                 | 组件已有（G5 组装）   |
| **权限确认**（features §4.1 / rendering §8.1）         | `permission.asked` → 权限视图：允许一次/允许并记住/拒绝（带反馈）；多阶段状态机 + diff 预览；回复经 `permission.replied` 送回                                                  | `ToolApprovalHandler`（wf-agent approval）→ `MiniApprovalHandler` 经 mpsc 投递 UI → 审批视图（y/a/d/n/c 对齐 codex）→ oneshot 回传 `ToolApprovalResult` | 缺（G6）         |
| **问题询问**（features §4.2 / rendering §8.2）         | 单选/多选（数字 1-9 + 自定义）、多问题 tab + Confirm；回复经 `question.replied` 送回                                                                            | `UserInteractionHandler.on_followup_question_requested` → mpsc 投递 UI → 问题视图 → `wf-api agent_user_interaction::respond` 送回                | 缺（G6）         |
| **串行 turn + 排队**（features §6 / tui §4.3）         | `runPromptQueue` 串行 drain；活跃时入队、队列可编辑/删除；`/new` 新会话                                                                                        | `PromptQueue`（Stage 6 新增）：一次一个 turn，活跃入队；排队面板 Enter/ctrl+e 编辑回填、Delete/ctrl+d 删除                                                         | 缺（G7）         |
| **两按中断/退出**（features §7 / tui §5.3）              | 首次提示、5s 内二次确认；退出写 exit splash 含 `opencode --mini -s <id>` 续跑提示                                                                             | `DoublePressTracker`（Stage 3 已有）接线 + exit 提示 `wf --mini --session <id>`（05 §4.5）                                                         | 状态机已有（G7 接线）  |
| **turn summary**（features §6）                    | 每轮结束追加 `▣ agent · model · duration`                                                                                                        | `▣ exec_id · type · iterations · duration`（05 §3.3/§5.2）；reducer `Completed{iterations}` + 墙钟计时                                          | 缺（G7）         |
| **demo 模式**（features §10 / tui §9）               | `/permission <kind>`、`/question <kind>`、`/fmt <kind>` 合成事件；`--demo` 要求 `--mini`                                                            | `demo.rs` 最小版：合成 `UnifiedEvent` 序列 + `/fmt`、`/permission`、`/question` 演示命令                                                               | 缺（G8）         |
| **主题热更新**（tui §8.5 / rendering §9）               | 调色板推导 + `PALETTE`/`THEME_MODE`/OSC/SIGUSR2；最后已知良好主题                                                                                        | `theme.rs`（Stage 3 已有 probe/reload/缓存）+ SIGUSR2 事件接入事件循环                                                                                 | 已有（G11 接线）    |
| **entry 分组/分隔/spacer**（rendering §3.2）           | 按 partID 分组、同组不分隔、inline→inline 不分隔、其余插空行                                                                                                  | `CommitGroup`（`execution_id + iteration + tool_call_id`，reducer.rs 已有）映射为 scrollback 分隔                                                  | 部分（G3 呈现）     |
| **流式 markdown 增量提交**（rendering §3.4）             | `MarkdownRenderable streaming:true` 按稳定 block 提交，未完结 block 不固化                                                                             | `MarkdownStream`（Stage 5 已有：committed/streaming 分界 + 围栏/空行判定）                                                                            | 已有            |
| **尾部行保留**（rendering §3.4）                        | 流式过程 `height-1` 尾巴行不提交，`done` 才完整提交防换行闪烁                                                                                                   | mini scrollback 渲染时 `LineState::Streaming` 行保留末行（Stage 4 HistoryLine 支持）                                                                 | 呈现层（G3）       |

### 2.3 必须对齐的行为契约（实现期不可偏离）

1. **业务输出不直接 println**：mini 模式下 LLM 文本/工具行/系统消息一律走 MiniSink 追加 scrollback，由渲染器统一绘制（总方案 §2.1 边界 + §七决策"mini 直接 println"）。
2. **无头与 mini 同源**：同一 `SessionReducer` + `MarkdownStream` 产物（Stage 5 已用 `HeadlessRenderer` 同源测试验证）；mini 滚动区渲染 `MiniCommit` 而非重新解析事件。
3. **会话保存 = 原始消息序列**：不得把 `Viewport::Inline` 画面导出文件；`--log` 经 `TeeSink` 落干净 text/json（总方案 §2.1）。
4. **`--demo`** **必须要求** **`--mini`**（对齐 opencode：`--demo`、`--replay-limit` 必须搭配 `--mini`）。
5. **退出后终端状态干净**：raw mode 关闭、inline 视口禁用、对话留在终端 scrollback 可回溯。
6. **用户文本入库前 sanitize（自 codex 分析引入，D16）**：粘贴/外部来源进入 scrollback 与消息库的文本一律剥离 CSI 与除 `\n`/`\t` 外控制字符（codex `sanitize_user_text` 语义），headless 与 mini 共用同一清洗函数，防止终端转义内容入库。
7. **主动停止 ≠ 故障（自 codex 分析引入，D17）**：两按退出/`/quit`/SIGINT 中断触发的执行关闭事件不得被 wf-runtime 记为 `Failed` 派发给 UI；`--session <id>` 续跑路径按"被主动停止"而非"失败"呈现。

***

## 三、关键设计决策

| #   | 决策                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | 理由                                                                                            |
| :-- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------- |
| D1  | **渲染底座 =** **`Terminal`** **+** **`Viewport::Inline(n)`** **+ 独立事件循环**：`MiniApp` 持有 `ratatui::Terminal<CrosstermBackend<Stdout>>`，经 `Terminal::with_options(TerminalOptions { viewport: Viewport::Inline(n), .. })` 创建（ratatui 0.30 `Viewport::Inline(u16)`：底部 n 行、宽度随终端、绘于光标下方）；footer 高度变化时**重建 Terminal 的 viewport**（`with_viewport` 或重建实例）并触发一次 `draw`                                                                                                                                                                           | 对齐 05 §3.1 与 opencode split-footer；`Viewport::Inline` 把历史自然写入终端 scrollback（轻量体验核心）            |
| D2  | **事件循环单一** **`select!`**：tokio 循环合并 ① crossterm `EventStream`（KeyEvent/Resize，`KeyCode` → `keymap::Key` 归一，对齐 Stage 4 `Key` 自研结构）② `FrameRequester::deadline()` 定时分支（渲染限帧）③ 会话事件 mpsc（`UnifiedEvent` 批量 drain → `SessionReducer::push_batch` → commits → scrollback/footer）④ `theme_reload_signals`（SIGUSR2 主题热更新）⑤ crossterm SIGINT（经 `DoublePressTracker`）。**crossterm 输入与 tokio 事件流通过 mpsc 桥接**（crossterm `EventStream` 本身就是 stream，可直接 `select!`）                                                                                | 对齐 opencode `runtime.ts` 编排；`select!` 单一循环避免多线程竞态；FrameRequester（Stage 4）的 deadline 语义在此接线    |
| D3  | **`MiniSink`** **= mpsc 内存型 sink**（总方案 §2.1 落地）：实现 `OutputSink` trait，`write_text/write_message/write_chunk` 把内容编码为 `MiniOutputEvent`（`Text{role}` / `Message` / `Chunk`）经 `tokio::sync::mpsc::UnboundedSender` 发给 UI 单消费者；UI 侧批量 drain → 追加 `HistoryLine`（`Role` 映射）→ `request_frame()`。`--log <file>` 时包装 `TeeSink { MiniSink, HeadlessFileSink::file }`                                                                                                                                                                           | 不引入 `Arc<Mutex<AppState>>`（总方案 §2.1 所有权红线）；业务层零 IO，渲染完全交给 ratatui                             |
| D4  | **`Footer`** **视图模型 =** **`FooterView`** **+** **`FooterRoute`** **双层**：`FooterView { Prompt, Permission, Question }`（由 reducer/领域事件驱动，对齐 opencode `pickBlockerView`）；Prompt 视图下有 `FooterRoute { Composer, Command, Model, Skill, Queued }`（面板路由）；`present(view)` 切换并 `apply_height()`。**审批/问题视图是阻塞态**（阶段感知：Streaming 期间 keyboard 归 composer/缓冲，Approval/Question 期间归对应视图，05 §4.4.3）                                                                                                                                                 | 对齐 opencode `FooterView`/`FooterPromptRoute` 双态；面板用 route 而非视图保证"面板从 composer 上下文打开、Esc 逐层关闭" |
| D5  | **动态高度表 + 仅变化重建**：`apply_height()` 按 `FooterView × FooterRoute × composer 行数` 计算高度（base=4 含 statusline+装饰；composer base+clamp(rows,1,6)；面板 base+16；审批 base+12；问题 base+14——对齐 05 §4.3 与 opencode 常量）；高度变化才重建 `Viewport::Inline(n)` 并重绘（记录当前 `last_height`）                                                                                                                                                                                                                                                                            | 对齐 opencode `applyHeight` "仅实际变化时写 footerHeight"；减少重绘                                         |
| D6  | **composer P0 自研单行（不引入 ratatui-textarea）**：`Composer { buf: String, cursor: usize (grapheme), scroll_x, history: VecDeque<String> (100), stash: Option<String>, placeholder }`；P1（本阶段可延）`ratatui-textarea` 多行 + `@` mention（`Vec<(Range, MentionKind)>` 区间高亮，MentionKind ∈ File{path,lines}/Skill/Workflow）+ `/` 命令 palette                                                                                                                                                                                                          | 03 §4.2 决策"P0 单行自研、P1 外购 ratatui-textarea"；extmark 无 ratatui 对应，用区间高亮（05 §3.2 简化）             |
| D7  | **审批 =** **`MiniApprovalHandler`（ToolApprovalHandler）+ footer 审批视图**：handler 的 `request_approval` 把 `ToolApprovalRequest` 经 mpsc 投递 UI，`await` 一个 oneshot `Receiver`；UI 切 `FooterView::Permission` 渲染描述 + 参数 + 键位提示（**y** 允许本次 / **a** 允许并记住（会话内） / **d** 拒绝 / **n** 拒绝本次 / **c** 取消，对齐 codex 键位 05 §3.4）；按键结果经 oneshot 回传 `ToolApprovalResult`。视图关闭后 `present(Prompt)`                                                                                                                                                              | 交互形态必须人工确认（05 §3.4）；与 Stage 2 无头 deny 策略分属两形态，互不影响                                            |
| D8  | **追问 =** **`MiniInteractionHandler`（UserInteractionHandler）+ footer 问题视图**：`on_followup_question_requested` 把请求经 mpsc 投递 UI → `FooterView::Question` 渲染问题（单选/多选/自定义输入，数字 1-9 快捷键）→ 答案经 `wf-api agent_user_interaction::respond` 送回领域层；`on_tool_approval_requested` no-op（审批走 D7 handler 通道）                                                                                                                                                                                                                                          | 对齐 opencode `footer.question.tsx`；respond 走既有 API 而非自建回复通道                                    |
| D9  | **串行 turn +** **`PromptQueue`**：`MiniQueue { queue: Vec<QueuedPrompt>, active: Option<TurnState> }`；composer 提交 → 无活跃 turn 直接 `spawn_turn`，有则入队；`/new` 清空会话状态；每轮结束追加 `▣ exec · iterations · duration` turn summary 行；排队面板（route `Queued`）`Enter`/`ctrl+e` 回填 composer 并移除、`Delete`/`ctrl+d` 删除，队列空自动关面板                                                                                                                                                                                                                              | 对齐 opencode `runPromptQueue` + queued 面板语义（features §3.6/§6）                                  |
| D10 | **两按退出接线 + exit splash**：`DoublePressTracker`（Stage 3）——第一次按下清空 composer/显示 `interrupt`（Streaming）或 `Press ctrl+c again to exit`（Idle）；5s 内第二次中断 turn / 退出。退出流程严格对齐 opencode：`footer.close() → idle() → destroy() → Terminal 恢复（切回 main-screen 语义=禁用 inline 视口）`，期间打印 exit splash（含 `wf --mini --session <execution_id>` 续跑提示），最后 `adapter.shutdown()`                                                                                                                                                                               | 对齐 05 §4.5 与 opencode 关闭顺序；TerminalStderrGuard/raw mode 恢复由 Stage 3 Guard 保证                  |
| D11 | **`--demo`** **最小版 = 合成事件源**：`demo.rs` 提供 `DemoSource`（实现与真实会话事件流相同的 mpsc 生产方）：`/fmt <kind>`（markdown/text/tool/question 等合成 `UnifiedEvent`）、`/permission`、`/question`、`/help`；demo 会话不 bootstrap 真实 runtime（或 bootstrap 但 mock LLM），事件经同一 reducer/footer 管线                                                                                                                                                                                                                                                                         | 对齐 opencode `demo.ts`（features §10）；Stage 8 完整版在此扩展                                           |
| D12 | **keymap 扩展 mini 上下文**：`KeymapContext` 新增 `Composer`/`Panel`/`Approval`/`Question`（保留既有 Global/List/Detail/Chat/Input/Modal）；`KeyAction` 仅需补 `DenyOnce`（Approve/ApproveAll/Deny/Cancel 已在 Stage 4 预置，且 Modal 上下文已绑定 y=Approve/a=ApproveAll/n=Deny/Esc=Cancel）；绑定表：composer（Enter=Submit、↑/↓=HistoryPrev/Next、`@` 触发 mention、`/` 触发 palette）、panel（MovePrev/Next/Select/Back、ctrl+u=Clear）、approval（复用 Modal 既有 y/a/n/Esc，补 d=DenyOnce、c=Cancel）、question（1-9/Enter/Esc）；审批/问题视图**捕获全部键**（`captures_all_keys`，对齐 03 §5.1 Modal trait） | 03 §3.1 红线 8（禁止裸 match 键位）；04 §七 上下文回退在此具体化；复用 Stage 4 Modal 既有批准键位                           |
| D13 | **scrollback 呈现层**：mini 滚动区 = `Vec<HistoryLine>`（Stage 4）+ 流式行（`LineState::Streaming`）；`MiniCommit` → `HistoryLine` 映射（User→`› ` 前缀+Accent、AssistantText→`MarkdownStream` 产物 plain text、ToolStart/End→`▲/✓/✗` 行、Completed→turn summary、Failed/Interrupted→Error 角色）；`LinesView`（Stage 4）带 scroll\_offset 渲染；宽度变化（ResizeDebouncer 75ms）触发全量 `display_lines(width)` reflow                                                                                                                                                             | 组件纯数据化（Stage 4 红线）；同源契约（2.3-2）                                                                |
| D14 | **流式渲染正确性补齐（自 codex** **`ai-output.md`** **分析引入）**：`MarkdownStream` 增：① **表格 holdback**——`TableHoldbackScanner`（`FenceTracker` 区分 markdown fence 与代码块）识别"表头 + 分隔行"后整表留在 streaming 侧直到定稿，防列宽错位；② **换行门控**——提交点不超过最后一个换行（`rfind('\n')` 门控，与块边界取更保守者），半行不提交；③ **引用式链接定义全量回退**——`push` 检测 `reference_definitions()` 非空即跳过增量切分、整段 streaming（或触发一次全量重渲）；④ **定稿兜底**——流定稿时以完整源码渲染为准做增量比对/替换（对齐 codex `assert_streamed_equals_full` 不变量，单测锁定"流式过程 == 完整结果"）                                                                                   | mini 是 MarkdownStream 第一个 UI 消费方；表格/半行/引用链接三类闪烁在 mini 首次可见（分析 §4.1-4.3）；定稿兜底是正确性硬约束（分析 §4.2）  |
| D15 | **resize 流式宽度 + 定稿强制重排（自 codex** **`resize-and-exit.md`** **分析引入）**：resize 事件（ResizeDebouncer 75ms）→ ① 立即更新流式渲染宽度（streaming 预留 2 列，对齐 codex `StreamController::set_width`）；② 触发 scrollback 中 streaming 区 + 最近 committed 区按新宽度重渲（`HistoryLine` 持源文本，内存模型允许）；③ 流定稿时若发生过流内 resize，**强制一次全量重排**（对齐 `maybe_finish_stream_reflow`），否则 Inline 视口内残留旧宽度行。边界声明：已滚入终端 scrollback 的行无法重排，本机制只覆盖视口内 + 保留窗口                                                                                                                                       | Inline 视口特有约束（分析 §6.1）；已滚入 scrollback 的历史行重排不可达，须文档明示此边界                                      |
| D16 | **输入边界与 sanitize（自 codex** **`input.md`** **分析引入）**：① TUI 初始化/`with_restored` 恢复后**丢弃早到输入**（`discard_pending_terminal_input`：排空 stdin 残留，最长 1s）——防快速键入灌进输入框/误触首屏操作；② `sanitize_user_text`（剥离 CSI、保留 `\n`/`\t`）在**消息聚合层**统一做，headless 与 mini 共用（契约 2.3-6）                                                                                                                                                                                                                                                                             | 分析 §3.5/§3.4：低成本防误触；控制字符清洗是 P0 安全项                                                            |
| D17 | **退出防 failover（自 codex** **`resize-and-exit.md`** **分析引入）**：`MiniApp` 维护"主动停止"标记（两按退出/`/quit`/SIGINT 中断），执行关闭回调携带该标记；领域层（wf-runtime shutdown 路径）不得把主动关闭当成 `Failed` 派发（防退出瞬间闪现错误行/后台重试）；`--session <id>` 续跑路径按"被主动停止"呈现                                                                                                                                                                                                                                                                                                               | 分析 §6.3；三形态共用语义（总方案风险表已补）                                                                     |
| D18 | **挂起/恢复（SIGTSTP/SIGCONT，自 codex** **`resize-and-exit.md`** **§7.2 引入）**：`select!` 增 SIGTSTP 分支——恢复终端（`TerminalGuard` restore）→ `SIGSTOP`；SIGCONT 后**重应用 raw mode（不假设未变）** + **强制重查终端几何**（挂起期间尺寸变化无 resize 事件）+ **清输入残留** + 全量重绘                                                                                                                                                                                                                                                                                                      | 03 文档 2.1 已有设计但 stage6 未列；对齐 codex `SuspendContext` Resume 语义（分析 §6.5）                        |
| D19 | **滚动区尾部局部替换（自 codex** **`history.md`** **§3.2 引入）**：streaming 行每次 commit tick 与已渲染行做公共前缀 diff，只重渲差异部分；流定稿走"上移视口 → clear → 写新尾部"的 Inline 视口替换，避免整段重画                                                                                                                                                                                                                                                                                                                                                                                  | 分析 §5.3；配合 D14-④ 的"全量重渲兜底"以增量形态落地                                                             |
| D20 | **会话快照与富元素区间约束（自 codex** **`input.md`** **§5.3/§2.1 引入）**：① 会话/executions 切换时保存 `Composer` 草稿与 `PromptQueue` 队列快照（防切屏丢输入）；② P1 mention 区间 `Vec<(Range, MentionKind)>` 若落地在 `ratatui-textarea` 之上，必须做**编辑 diff 后区间偏移同步**（外购库无富元素区间模型）；评估外购库 API 不足以支撑时，切自研字节坐标 TextArea 核心子集                                                                                                                                                                                                                                                          | 分析 §3.2/§3.1；富元素区间维护是外购库否决项，P1 评估时先行验证                                                        |
| D21 | **`args.rs`** **扩展 + 互斥校验**：`--session <id>`（resume）、`--resume`、`--demo`、`-p/--prompt`、`--agent`、`--model` 仅交互形态可见；`Cli::validate()` 增：`--demo` 要求 `--mini`、`-p` 仅 mini（run 形态已有 prompt 位置参数）、`--session/--resume` 要求交互形态（**6A 已落地**）                                                                                                                                                                                                                                                                                                | 对齐 opencode CLI 选项矩阵（features §1）；避免 `--demo` 误入 headless                                     |

***

## 四、模块落点

```
crates/wf-cli/src/
├── mini.rs         ← 新增：MiniApp（渲染底座 Terminal+Viewport::Inline、事件循环 select!、
│                     会话驱动 spawn_turn、生命周期/退出流程、splash）
├── footer.rs       ← 新增：FooterView/FooterRoute/FooterState 视图模型、Footer 组件
│                     （apply_height 动态高度 + statusline + notice + 两按提示渲染）
├── composer.rs     ← 新增：Composer（P0 单行：grapheme 光标/水平滚动/历史100/placeholder；
│                     P1 多行 + @mention 区间高亮 + / 命令 palette）
├── panels.rs       ← 新增：ModelPanel（llm_profile::list）/SkillPanel（SkillLoader）
│                     /QueuedPanel（PromptQueue）——SelectList 组装 + 过滤输入
├── approval.rs     ← 新增：MiniApprovalHandler（ToolApprovalHandler + oneshot）
│                     + ApprovalView（y/a/d/n/c 状态机 + 描述/参数渲染）
├── question.rs     ← 新增：QuestionView（单选/多选/自定义状态机）+ 领域接线（respond）
├── queue.rs        ← 新增：PromptQueue + QueuedPrompt（串行 turn + 排队 + 编辑/删除）
├── demo.rs         ← 新增：DemoSource（合成 UnifiedEvent 序列）+ /fmt /permission /question
├── sink.rs         ← ✅ 6A 已交付：MiniSink（mpsc 内存型 OutputSink）+ MiniOutputEvent
├── args.rs         ← ✅ 6A 已交付：--session/--resume/--demo/-p/--agent/--model + validate 扩展
├── keymap.rs       ← ✅ 6A 已交付：KeymapContext::Composer/Panel/Approval/Question + mini 键位绑定
├── lib.rs          ← 接线：run_interactive 分发到 mini（CliMode::Mini → MiniApp::run）
├── output.rs       ← （不改动，TeeSink 复用）
└── scrollback.rs   ← ✅ 6B 已补：HistoryLine::raw_lines() 三通道（stage4 方案 §七 I1，D19 窗口快照消费）
```

依赖关系：`mini.rs`（编排）→ `footer.rs`/`composer.rs`/`panels.rs`/`approval.rs`/`question.rs`/`queue.rs`/`sink.rs` → 既有 Stage 0-5 组件（reducer/markdown/scrollback/select/keymap/framer/theme/terminal/events/domain/output）。

无新增第三方依赖（P0 阶段）：composer 自研单行（D6），mention 用自研区间高亮，均不引入 ratatui-textarea / fuzzy matcher（fuzzysort 对应物 P1 评估）。

***

## 五、分阶段任务与验收

### 阶段 6A：args/keymap/MiniSink 前置（G9/G10/G2）✅ 已完成（2026-08-20）

* [x] `args.rs`：`Cli` 增 `--session`/`--resume`/`--demo`/`-p/--prompt`/`--agent`/`--model`；`validate()` 增 `--demo` 要求 `--mini`、`--session/--resume` 互斥、`--session/--resume` 要求交互形态、`--no-tui` 与交互选项互斥。单测：各选项解析 + 非法组合报错。

* [x] `keymap.rs`：`KeymapContext` 增 `Composer/Panel/Approval/Question`；`KeyAction` 补 `DenyOnce`（Approve/ApproveAll/Deny/Cancel 已有，Modal 已绑 y/a/n/Esc）；内置绑定表：composer（Enter=Submit、Esc=Back、↑/↓=HistoryPrev/Next、Ctrl+u=Clear）、panel（MovePrev/MoveNext/Select/Back/Delete/Edit、Ctrl+u=Clear）、approval（复用 Modal 的 y=Approve/a=ApproveAll/n=Deny/Esc=Cancel，补 d=DenyOnce、c=Cancel）、question（1-9 数字、Enter=Select、Esc=Cancel）。单测：上下文回退顺序 + mini 各上下文查表断言。

* [x] `sink.rs`：`MiniOutputEvent { Text { role, content } / Message { role, content } / Chunk { content } }`；`MiniSink` 实现 `OutputSink`（`UnboundedSender<MiniOutputEvent>` + `flush` 语义 = 触发一次 frame 请求标记）；`MiniSink::tee_log(path, format)` 返回 `(MiniSink, HeadlessFileSink)` 组合（复用 `TeeSink`）。单测（MemoryReceiver 断言）：write\_text/write\_message/write\_chunk 编码 + Tee 落盘内容。

**完成记录（2026-08-20）**

* 三件套落地（1.3 节）；`cargo test -p wf-cli` 全绿，Stage 2-5 无回归。

**验收**：`cargo check -p wf-cli` 通过；args/keymap/sink 单测全绿；`wf --demo`（无 `--mini`）报参数错误。✅

### 阶段 6B：渲染底座 + footer 骨架 + 事件循环（G1/G3 主体）✅ 已完成（2026-08-22）

* [x] `mini.rs`：`MiniApp::new(adapter, cli)`；`Terminal::with_options(Options { viewport: Viewport::Inline(BASE_HEIGHT), .. })`（ratatui 0.30）；`run() -> CliResult<ExitOutcome>` 事件循环 `select!`（D2）；`install_panic_hook` + `TerminalGuard`（MINI 模式集：raw + bracketed paste + hide cursor）进入，Drop/退出恢复。**签名偏差：本阶段** **`MiniApp::new()`** **无** **`(adapter, cli)`** **参数（会话驱动留 6D，见偏差表 P6）。**

* [x] 事件循环扩展（自 codex 分析引入）：**SIGTSTP/SIGCONT 分支**（D18：restore → SIGSTOP；SIGCONT 重应用 raw mode + 强制重查几何 + 清输入残留 + 全量重绘）；**输入边界**（D16：TUI 启动与 `with_restored` 恢复后丢弃早到输入，最多 1s 排空 stdin 残留）。

* [x] `footer.rs`：`FooterView { Prompt, Permission, Question }`、`FooterRoute { Composer, Command, Model, Skill, Queued }`、`FooterState { phase, iteration, active_tools, message_count, last_error, model, duration, notice }`（reducer `FooterState` 扩展 UI 侧字段）；`present()`/`apply_height()` 高度表（D5）；`draw(footer_area)`：主区（composer/面板/审批/问题）+ statusline。

* [x] statusline：模式标签（`BUILD`/`EXIT` 着色）+ spinner（40ms blocks，busy 时）+ 状态文本（interrupt/again to interrupt/退出提示）+ 左中右布局；宽度响应式（<80 隐藏右侧模型/摘要区块，80/120 断点对齐 05 §4.4.4）。notice 机制（3s + statusVersion 防覆盖）。

* [x] scrollback 呈现：`MiniCommit` → `HistoryLine` 映射（D13）；流式行保留末行；`LinesView` 渲染；宽度变化 reflow（ResizeDebouncer）。**运行时映射随 6D 会话驱动接线（render.rs 同源测试已锁定契约 2.3-2；见偏差表 P6）。**

* [x] **流式渲染正确性补齐（D14，markdown.rs）**：表格 holdback（`TableHoldbackScanner` + `FenceTracker`）、换行门控（提交点 ≤ 最后换行）、引用式链接定义全量回退、定稿源码驱动全量重渲兜底（`final_plain_text()`）+ `assert_streamed_equals_full` 单测（LCG 伪随机 char 边界切分 → 流式提交 → 与整段渲染比对，覆盖表格/fence/引用链接/空行边界/混合文档）。

* [x] **resize 流式宽度（D15）**：resize 事件 → 更新流式渲染宽度（streaming 预留 2 列，`STREAMING_WIDTH_MARGIN`）→ streaming 区 + 最近 committed 区按新宽度重渲；流定稿时若发生过流内 resize 强制一次全量重排（`reflow_scrollback`）。

* [x] **滚动区尾部局部替换（D19）**：streaming 行 commit tick 公共前缀 diff 只重渲差异（footer 视口内由 ratatui 双缓冲 diff 承担；滚动区窗口重绘按 `common_prefix_rows` + `window_rows` 快照只重写差异行）；定稿走 Inline 视口"上移 → clear → 写新尾部"替换。

* [x] composer P0 单行（D6）：绘制 + 输入处理（Char/Backspace/Left/Right/Home/End/Enter/Esc）+ 历史导航；提交把内容回传事件循环（Submit → queue/turn 逻辑，6D 接线，本阶段先打点）。

* [x] 两按退出最小接线：SIGINT → `DoublePressTracker` → 第一次提示、第二次退出；退出流程（D10）恢复终端 + 打印 exit 提示。

**完成记录（2026-08-22）**

* 渲染底座/事件循环/footer/statusline/composer/两按退出/D18/D16：随 `eb33bec`、`cbffeb8` 落地（约 1600 行）；D14-①②③ + `sanitize.rs`（D16-②）+ `run_session` 接线随 `cbffeb8` 落地。

* 本次补缺（D14-④/D15/D19/stage4 I1）：`markdown.rs` 增 `final_plain_text()` + `assert_streamed_equals_full`（LCG 随机切分 × 5 场景 × 8 seeds）；`scrollback.rs` 增 `HistoryLine::raw_lines()`（三通道补全）；`footer.rs` 增 `STREAMING_WIDTH_MARGIN = 2`（streaming 渲染 `width - 2`）；`mini.rs` 增 `stream_resized`/`window_rows`/`common_prefix_rows`/`update_window_snapshot`/`reflow_scrollback`（定稿强制重排 + 窗口公共前缀 diff）。

* 文档同步：本文件 6B 勾选；stage4 §七 I1、stage5 §七 I4 标记完成；总方案 Stage 6 状态更新。

**验收**：`wf --mini`（无模型调用）可进入：footer 显示 composer + statusline，输入可编辑、Enter 提交打点、两按退出干净；`script` PTY 冒烟：raw mode 无残留、退出后提示正确；非 TTY 走既有报错（exit 2）。**验证状态：静态核对完成；`cargo test -p wf-cli`** **全绿结果待后台编译任务确认（2026-08-22）。**

### 阶段 6C：面板 + composer 完善（G4/G5）✅ 已完成（2026-08-24）

* [x] `panels.rs`：`ModelPanel`（`llm_profile::list` → 分组列表，当前模型定位，Enter 切换 → 更新 statusline + 后续 turn 生效）、`SkillPanel`（`SkillLoader` 枚举技能，Enter 即执行 `/技能名`）、`QueuedPanel`（`PromptQueue` 数据；Enter/ctrl+e 编辑回填 composer、Delete/ctrl+d 删除，队列空自动关）。

* [x] `/` 命令 palette（route `Command`）：`/new`（新会话）、`/model`（模型面板）、`/skills`（技能面板）、`/queued`（排队面板，有排队时）、`/quit`（退出）；内置 + 面板入口经 keymap（`Palette` action）。

* [x] composer 完善：`/editor` 外部编辑器（`TerminalGuard::with_restored` 窗口，返回后重绘）；历史去重 + stash 语义（↑ 存草稿进历史、↓ 越末尾恢复）。

* [ ] P1（如进度允许，否则留 Stage 8 排期）：多行（≤6 行 word wrap）+ `@` mention（文件 find\_files + `#行号`、技能、工作流；区间高亮渲染 + 提交时把 parts 随 prompt 传递）。**区间簿记约束（D20-②）**：若落地在 `ratatui-textarea` 之上，每次编辑须做区间偏移同步（监听编辑 diff 重算 `Vec<(Range, MentionKind)>`）；外购库 API 不足以支撑则切自研字节坐标 TextArea 核心子集。**→ 留 Stage 8 排期。**

* [x] **sanitize（D16-②）**：`sanitize_user_text`（剥离 CSI、保留 `\n`/`\t`）在消息聚合层落地，composer 提交与 headless 路径共用；单测覆盖粘贴/外部来源文本。

**验收**：`wf --mini --demo`（6E 前可用 stub 事件源）逐面板切换无残留；`/model` 切换后 statusline 更新；排队面板编辑/删除行为正确；keymap 回退单测全绿。

### 阶段 6D：审批/问题视图 + 会话驱动（G6/G7）✅ 已完成（2026-08-24）

* [x] `approval.rs`：`MiniApprovalHandler { tx: UnboundedSender<ApprovalEvent> }` 实现 `ToolApprovalHandler`——`request_approval` 投递 + `oneshot::Receiver` await（带超时保护）；`ApprovalView` 状态机（permission → 按键 → 结果回传）；键位 y/a/d/n/c；视图渲染（工具名 + 参数 + 键位提示）。

* [x] `question.rs`：`MiniInteractionHandler` 实现 `UserInteractionHandler`——`on_followup_question_requested` 投递 UI；`QuestionView`（单选/多选/自定义 + 数字快捷键）；答案经 `wf-api agent_user_interaction::respond` 送回。

* [x] `queue.rs` + `mini.rs` 会话驱动：`spawn_turn(prompt)` → `RunAgentLoopParams { agent_loop_id, approval_handler: MiniApprovalHandler, config/input }` → `agent_execution::stream` → 事件流批量 drain → `SessionReducer::push_batch` → commits → scrollback/footer；turn 结束（Completed/Failed/Interrupted）→ turn summary + phase 回落 + drain 队列下一项；`/new` 清空。

* [x] **会话快照（D20-①）**：composer 草稿与 `PromptQueue` 队列在会话/executions 切换时保存恢复（防切屏丢输入）。

* [x] **退出防 failover（D17）**：`MiniApp` 维护"主动停止"标记（两按退出/`/quit`/SIGINT 中断），执行关闭回调携带标记；领域层不得把主动关闭记为 `Failed` 派发；`--session <id>` 续跑路径按"被主动停止"呈现。

* [x] 阶段感知：`FooterState.phase` 驱动 keyboard 路由（Idle/Streaming → composer；Approval → ApprovalView；Question → QuestionView；Streaming 期间 composer type-ahead 缓冲）。

* [x] 退出续跑提示（D10）完整化：记录当前 `execution_id`，exit splash 打印 `wf --mini --session <id>`；`--session <id>` resume 走存储重放（P0：打点 + 不重建历史，对齐 opencode 惰性预热；完整重放留 Stage 8 replay）。

**验收**：mock LLM e2e：`wf --mini` 会话——提交 prompt → scrollback 流式输出 + 工具行 → 敏感工具触发审批视图（y 放行 / n 拒绝均正确回传）→ turn summary → 退出提示带 session id；`--demo` 的 `/permission`、`/question` 冒烟；退出后终端干净。

### 阶段 6E：demo + 收尾（G8）✅ 已完成（2026-08-24；同日按事件架构修订调整交付形态）

* [x] 管线展示（修订后）：`examples/mini_demo.rs` 合成 `ExecutionStreamEvent` 序列（markdown 流、工具 start/end、completed），驱动与真实会话完全相同的 reducer → footer / markdown / 审批 / 问题视图管线；不进 `src/`、不新增 CLI 参数（原 `src/demo.rs` + `--demo` 方案作废，见文档头修订记录）。
* [x] 勾选总方案 Stage 6 任务项；补完成记录；生成 patch（排除 target/，Cargo.lock 不动）。
* [x] 集成测试：`tests/mini_pipeline.rs` 合成 `ExecutionStreamEvent` 脚本断言 reducer/footer/审批/问题视图接线；`cargo test -p wf-cli` 全绿（Stage 2-5 无回归）。**验证状态：256 单测 + 4 集成全绿；examples 编译通过（2026-08-24，事件架构修订后）。**

**验收**：`cargo run -p wf-cli --example mini_demo` 管线展示完整（视图状态、scrollback、审批/问题渲染）；`cargo test -p wf-cli` 全绿；patch 校验 `grep -c 'target/'` = 0。

***

## 五·一、与方案的偏差（实施期预期，实施后按实际更新）

| #  | 预期偏差                                                                                                                                                                                                                     | 原因                                                                               |
| :- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------- |
| P1 | `Viewport::Inline` 高度变化采用"重建 Terminal viewport"而非 in-place 修改（ratatui 0.30 `Terminal` 的 viewport 在 `TerminalOptions` 构造期固定；高度变化路径待实施期验证 `Terminal::with_viewport` 是否可用，不可用则重建实例）                                         | ratatui 0.30 构造 API 限制；重建成本可接受（首帧 diff 全量，之后增量）                                  |
| P2 | `@` mention 的 fuzzysort 模糊匹配降级为 P0 子串匹配（对齐 Stage 4 `SelectList` filter 的 P0 语义），fuzzy 算法 P1 再评估                                                                                                                          | 03 §4.1 SelectList filter P0 子串匹配的既有决策                                           |
| P3 | `--session/--resume` 在本阶段只做参数接收 + 打点（`execution_id` 沿用），不做完整会话重放                                                                                                                                                         | 完整 replay 属 Stage 8（`replay.rs`）；opencode 也是 resume 时惰性预热                        |
| P4 | 面板数据源可能改走 `ApiContext` 查询而非直接持有 gateway/SkillLoader 引用（实施期按借用关系定）                                                                                                                                                        | 组件纯数据化红线与借用简化                                                                    |
| P5 | demo 模式可能 bootstrap 一个 mock-LLM 的 mini runtime 而非完全无 runtime（复用 Stage 2 mock e2e 基建）                                                                                                                                     | 保证 demo 与真实会话走同一领域调用路径                                                           |
| P6 | `MiniApp::new()` 本阶段无 `(adapter, cli)` 参数；`MiniCommit` → `HistoryLine` 运行时映射（D13 的 ToolStart/End、turn summary 等）随 6D 会话驱动（`spawn_turn` → `UnifiedEvent` → `SessionReducer::push_batch`）一起接线，mini 呈现层当前直接消费 `MiniSink` 事件 | 会话驱动属 6D（G7）；契约 2.3-2（无头与 mini 同源）已由 render.rs 同源测试锁定，运行时接线前不重复实现 reducer 路径     |
| P7 | D19"尾部局部替换"的落地形态：footer 视口内由 ratatui `Terminal::draw` 双缓冲 diff 承担（每帧仅刷变化 cell）；滚动区窗口重绘按"公共前缀 diff 只重写差异行"实现（`common_prefix_rows` + `window_rows` 快照，`reflow_scrollback` 内使用）                                             | ratatui 渲染模型天然具备 cell 级 diff；`insert_before` 本身即尾部增量，D19 的显式 diff 在重排（D15-③）路径落地 |
| P8 | D15 重排边界：仅重排**可见窗口**（`rows - viewport` 行）；已滚入终端自身 scrollback 的行无法 re-wrap                                                                                                                                                | `Viewport::Inline` 布局下滚动区顶部行进入终端 scrollback 后不可达（D15 边界声明）                       |

***

## 六、风险与边界

| 风险                                                           | 缓解                                                                                                               |
| :----------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------- |
| `Viewport::Inline` 与 raw mode/光标交互（业务 println 竞争、外部命令输出打花画面） | 业务输出统一走 MiniSink（D3）；stderr 抑制（Stage 3 `TerminalStderrGuard`）；外部命令经 `with_restored`（D6 `/editor`）                |
| inline 视口高度频繁重建导致闪烁                                          | `apply_height` 仅高度实际变化才重建（D5）；帧 diff 由 ratatui 双缓冲承担                                                             |
| 审批 await 与 turn 取消的竞态（用户拒绝后引擎是否继续）                           | `MiniApprovalHandler` 回传即返回结果；oneshot 带超时（对齐 wf-api 审批 timeout 语义）；取消路径（c 键）映射为 reject                           |
| 事件流与输入流优先级（流式渲染 vs 按键响应）                                     | 单 `select!` 公平轮询；FrameRequester 限帧（Stage 4）保证 30-60FPS；输入事件即时唤醒                                                  |
| resize 期间 reflow 与流式行竞争                                      | `ResizeDebouncer` 75ms 合并（Stage 4）；`HistoryLine` 持有源文本按新宽度重算（红线 9）                                               |
| 无 TTY / CI 冒烟                                                | 非 TTY 走既有 `run_interactive` 报错路径（exit 2）；`--demo` 冒烟经 `script` PTY 或合成输入注入（Stage 2/3 已验证该范式）                     |
| 审批/问题 handler 与 Stage 2 headless handler 的互斥                 | 两形态各自注册（headless 在 `run_session` 内、mini 在 `MiniApp` 内），`register_handler` 替换语义（wf-api `user_interaction.rs`）天然隔离 |
| composer 历史/mention 的 grapheme 边界                            | 复用 `unicode-segmentation`（Stage 4 已依赖）+ `unicode-width`；P1 多行评估 `ratatui-textarea`（03 §4.2 决策）                   |
| 退出时 inline 视口残留                                              | 退出流程严格对齐 opencode 关闭顺序（D10）；`TerminalGuard` Drop 兜底 + `install_panic_hook` 双保险（Stage 3）                          |

***

## 附：opencode-mini 关键常量 → wf-cli Stage 6 对照

| opencode-mini 常量（rendering §13 / features 附录） | 值                         | wf-cli Stage 6                                |
| :-------------------------------------------- | :------------------------ | :-------------------------------------------- |
| `FOOTER_HEIGHT`（初始）                           | 4                         | `BASE_HEIGHT = 4`                             |
| `TEXTAREA_MIN/MAX_ROWS`                       | 1 / 6                     | P0 单行；P1 1\~6                                 |
| `PANEL_LIST_ROWS` / `PANEL_FRAME_ROWS`        | 10 / 6                    | 面板高度 base+16（05 §4.3）                         |
| `PERMISSION_ROWS` / `QUESTION_ROWS`           | 12 / 14                   | base+12 / base+14（05 §4.3）                    |
| 宽度断点                                          | 80 / 66 / 120 / 150       | 80 / 120（05 §附）                               |
| 历史上限                                          | 200                       | 100（03 §4.2 决策）                               |
| 两按确认窗口                                        | 5s                        | 5s（Stage 3 `DoublePressTracker`）              |
| resize 防抖                                     | 250ms                     | 75ms（02 文档五·3 既有决策）                           |
| targetFps / maxFps                            | 30 / 60                   | `FrameRateLimiter` 8.3ms（120FPS 上限，实际帧 30-60） |
| spinner 间隔                                    | 40ms                      | 40ms                                          |
| notice 超时                                     | 3s                        | 3s                                            |
| 退出提示                                          | `opencode --mini -s <id>` | `wf --mini --session <id>`（05 §4.5）           |

