# wf-cli 分阶段实现方案（通用功能主线）

> 状态：方案设计
> 范围：`crates/wf-cli` 从零创建，覆盖三种交互形态（无头会话 run、mini 会话、全屏 TUI）与无头管理命令面**共享的通用功能**，按阶段推进实现
> 关联文档：`docs/cli/01-功能清单.md`（CLI-1~4 阶段）、`docs/cli/05-opencode-mini模式与无头模式设计.md`（三种形态设计）、`docs/plan/rust迁移-分阶段方案.md`（rust 迁移总纲）、`docs/cli/02~04`（UI/组件/终端交互设计）

## 一、背景与目标

### 1.1 现状

- `crates/wf-cli` 尚未创建（workspace members 未包含），CLI 仅有文档设计（`docs/cli/01-05`）。
- 01 文档的 CLI-1~4 以"功能域"划分（命令组 → TUI 引擎 → 执行管理 → 生态命令），未显式区分**三形态共享的通用层**与**各形态专用层**。
- 05 文档定义了四种形态：无头管理命令面（既有）、无头会话 run、mini 会话、全屏 TUI，并给出技术映射。

### 1.2 本方案目标

以**通用功能为主线**给出分阶段实现方案：先交付三形态共享的基础设施（模式判定、输出路由、领域调用、终端设施、UI 组件、流式渲染内核），再分别落地 run / mini / TUI 形态，保证：

1. **依赖有序**：每个阶段产出可编译、可测试的完整模块，不依赖尚未实现的形态。
2. **先闭环后美化**：run 无头会话是第一个端到端闭环（不依赖任何 TUI 设施），为后续形态提供领域调用与输出契约。
3. **组件共享**：mini 与 TUI 复用同一公共层，避免两套 UI 底座。
4. **与既有规划对齐**：本方案取代 01 文档 CLI-1~4 的粗粒度划分（阶段内容并入本文档 Stage 0-8），CLI 相关验收以本文档为准。

### 1.3 通用功能定义

通用功能 = 至少被两种形态使用、且与具体形态无关的基础设施。具体清单见第二节。

---

## 二、通用功能清单

| # | 通用功能 | run 无头 | mini | 全屏 TUI | 设计依据 | 现状依赖 |
| :- | :--- | :-: | :-: | :-: | :--- | :--- |
| F1 | 模式判定与入口路由（--tui/--mini/子命令/TTY 检测/stdin 管道/config 默认） | ● | ● | ● | 05 §2.2 | wf-config |
| F2 | 输出路由（Format 格式层 + OutputSink 目标层 + Tee 分发 + CLIError 退出码） | ● | ● | ● | 01 §4.4、05 §5.2、本方案 §2.1 | 无（自研） |
| F3 | 领域调用层（Runtime::bootstrap + ApiContext + 会话/执行启动） | ● | ● | ● | 05 §3.4、03 §1 | wf-runtime、wf-api、wf-agent |
| F4 | 事件订阅与统一事件流（ExecutionEvent / AgentStreamEvent 归一） | ● | ● | ● | 05 §3.4 | wf-types、wf-core EventBus |
| F5 | 事件折叠 reducer（事件 → commit 流 + footer 状态，纯函数） | ● | ● | ● | 05 §3.3 | 无（自研） |
| F6 | 流式渲染内核（streaming markdown，top-level block 增量提交） | ● | ● | ● | 05 §3.2 | pulldown-cmark |
| F7 | 审批/追问策略适配（deny / 视图 / 白名单 / --approve-prefix） | ● | ● | ● | 05 §3.4/§5.3 | wf-runtime approval_tool、wf-api UserInteractionHandler |
| F8 | 终端交互设施（TerminalGuard RAII、with_restored、stderr 抑制、SIGINT 两按） | — | ● | ● | 03 §2、05 §4.5 | crossterm |
| F9 | 主题探测与热更新（OSC 10/11、调色板降级、SIGUSR2） | — | ● | ● | 03 §6、05 §3.3 | crossterm |
| F10 | 公共 UI 组件（HistoryLine/scrollback、SelectList、Keymap、FrameRequester） | — | ● | ● | 03 §3-§7 | ratatui 0.30 |

> ● = 形态使用该功能；— = 不使用。F1-F7 为无 UI 前置（run 闭环的依赖），F8-F10 为 UI 形态前置。

### 2.1 输出流抽象（F2 细化：格式层 × 目标层）

输出路由拆为**两个正交维度**，提前抽象目标层（Sink），后续控制输出位置（终端/文件/管道/内存）不需改业务逻辑：

- **维度一：Format（格式层）**——text / json / jsonl / silent。对应 01 §4.4 信封与 05 §5.2 输出形态，由 `OutputRouter` 承担。
- **维度二：OutputSink（目标层）**——"往哪里写"与业务逻辑解耦，业务只调用 `write_text / write_message / write_chunk / flush`。

```rust
// 目标层 trait（规划形态，避免完整代码，仅示意接口）
pub trait OutputSink {
    fn write_text(&mut self, text: &str) -> Result<()>;
    fn write_message(&mut self, msg: &ChatMessage) -> Result<()>;   // 结构化消息（text/json）
    fn write_chunk(&mut self, chunk: &str) -> Result<()>;           // LLM 流式增量
    fn flush(&mut self) -> Result<()>;
}
```

**Sink 实现矩阵（按目标 × 模式）**：

| Sink | 写入行为 | 适用模式 | 说明 |
| :--- | :--- | :--- | :--- |
| `HeadlessFileSink` | 真实 IO（stdout / 文件 / Vec\<u8\>），writer 为 TTY 时开 ANSI、否则纯文本 | run 无头 | 流式 chunk 直接 write + 适时 flush（管道实时性） |
| `MiniSink`（内存型） | **不直接写 IO**，追加 HistoryLine 到 scrollback + 标记重绘 | mini | 与 TuiSink 同构，差异只在渲染器（inline vs alt-screen） |
| `TuiSink`（内存型） | 不直接写 IO，更新 AppState.messages + need_redraw | 全屏 TUI | 渲染完全交给 ratatui `terminal.draw()` |
| `MemorySink` | 写入 Vec\<u8\> / Vec\<Message\> | 测试 | 断言输出内容，不依赖真实终端 |
| `TeeSink` | 多路分发到子 sink 列表 | 任意 | 交互同时落盘：`[MiniSink, HeadlessFileSink]` |

**两个独立 IO 通道（禁止混淆）**：
1. **业务输出通道（OutputSink）**：对话消息/LLM 内容/工具返回，可到终端、文件、管道、内存。
2. **TUI 终端渲染通道（ratatui Terminal）**：仅 mini / 全屏 TUI 启用；headless 完全不初始化 ratatui。

**关键边界**：
- ratatui `Terminal<CrosstermBackend>` 只能面向交互终端；**不得**将 TUI 渲染（ANSI 画面）直接导出文件。会话保存 = 导出原始消息序列（ChatMessage），经 `HeadlessFileSink` 输出干净 text/json。
- 非 TTY 自动降级：mini / TUI 遇 stdout 非 TTY → 切换 `HeadlessFileSink`（对齐 05 §2.2 判定）。
- mini 模式业务输出**不直接 println 写屏**：println 与 ratatui `Viewport::Inline` 的光标操作竞争，会破坏底部视口。统一走"内存型 sink 追加 scrollback → 渲染器绘制"（capture-stdout + ansi-to-tui，对齐 05 §3.1）。
- 所有权：内存型 sink 经 tokio mpsc 到 UI 单消费者（03 §七 事件驱动架构），**不引入 `Arc<Mutex<AppState>>`**，避免锁竞争。

---

## 三、crate 结构与模块划分

```
crates/wf-cli/src/
├── lib.rs            ← pub mod 声明 + re-export（核心逻辑均在 lib，可单测）
├── main.rs           ← 薄入口（bin target，仅调用 lib::run）
├── args.rs           ← clap 4 derive 命令树（含 mode 互斥校验）            [Stage 0]
├── mode.rs           ← ModeResolver：判定顺序 + TTY 检测 + stdin 管道读取    [Stage 0]
├── error.rs          ← CLIError 定义与退出码映射（复用 wf-common Error）     [Stage 0]
├── output.rs         ← OutputRouter：text/json/silent 信封渲染             [Stage 1]
├── domain.rs         ← DomainAdapter：Runtime bootstrap + ApiContext 封装   [Stage 1]
├── events.rs         ← 事件适配：ExecutionEvent / AgentStreamEvent → 统一流 [Stage 1]
├── run.rs            ← 无头会话驱动（流式 stdout + 摘要行 + 退出码）        [Stage 2]
├── terminal.rs       ← TerminalGuard / with_restored / stderr 抑制 / SIGINT [Stage 3]
├── theme.rs          ← 主题探测（OSC 10/11 + 调色板 + 热更新）             [Stage 3]
├── scrollback.rs     ← HistoryLine（display_lines / desired_height / reflow）[Stage 4]
├── select.rs         ← SelectList（分组滚动列表）                          [Stage 4]
├── keymap.rs         ← Keymap（上下文回退、阶段感知）                      [Stage 4]
├── framer.rs         ← FrameRequester（限帧 + 批量 drain）                 [Stage 4]
├── reducer.rs        ← 事件 → MiniCommit[] + FooterState 归约（纯函数）    [Stage 5]
├── markdown.rs       ← streaming markdown 渲染（top-level block 增量）    [Stage 5]
├── footer.rs         ← mini footer：主区视图路由 + 动态高度 + statusline   [Stage 6]
├── composer.rs       ← 输入（P0 单行 / P1 ratatui-textarea + @mention）    [Stage 6]
├── panels.rs         ← 模型/技能/排队面板                                 [Stage 6]
├── approval.rs       ← 审批交互视图（y/a/d/n/c 键位）                      [Stage 6]
├── question.rs       ← 追问交互视图（单选/多选/自定义）                    [Stage 6]
├── screens.rs        ← 全屏 TUI 8 屏 + 屏幕栈                             [Stage 7]
├── modal.rs          ← 模态框栈（Modal trait + oneshot）                  [Stage 7]
├── replay.rs         ← 会话重放（从存储重建 scrollback）                   [Stage 8]
└── demo.rs           ← 合成事件演示（--demo 驱动完整管线）                 [Stage 8]
```

依赖 DAG（wf-cli 处于最底层，只被应用形态消费）：

```
wf-types/wf-common/wf-config/wf-storage → wf-runtime/wf-api/wf-agent → wf-cli
外部：clap 4 (derive)、tokio、serde/serde_json、tracing、crossterm、ratatui 0.30、
      pulldown-cmark、（P1）ratatui-textarea、（P2）syntect、（测试）insta
```

---

## 四、分阶段实施

### Stage 0：crate 骨架与模式判定（通用 F1）

**目标**：`wf-cli` 加入 workspace，命令树可解析，模式路由可判定；无 UI、无领域调用。

**任务**

- [ ] 创建 `crates/wf-cli`（lib + bin），注册 workspace member；workspace 依赖新增 `clap 4`（derive）+ `clap_complete`。
- [ ] `args.rs`：clap 命令树——`wf`（无参交互入口）+ 子命令组（`agent` / `workflow` / `executions` / `session` / `config` 等，命令面与 01 文档 §4 对齐）；`--tui` / `--mini` / `--no-tui` / `--output` 选项；互斥校验（--tui 与 --mini 互斥、--output 与交互形态互斥）。
- [ ] `mode.rs`：`ModeResolver` 实现 05 §2.2 判定顺序（--tui > --mini > 子命令 > stdout 非 TTY → run > TTY → config 默认形态）；stdin 非 TTY 时读取全文为 prompt；TTY 判定用 crossterm `is_tty()`（libc 或 std::io::IsTerminal）。
- [ ] `error.rs`：`CLIError`（对齐 01 §4.4：0 成功 / 1 业务失败 / 2 参数 / 3 配置 / 4 中断）+ `From<wf_common::Error>` 转换；`ExitCode` 映射。
- [ ] `lib.rs`：`pub fn run() -> CLIResult` 入口，`main.rs` 薄调用。

**交付与验收**

- `cargo check -p wf-cli` 通过；`wf --help` / `wf run --help` / `wf --mini --help` 输出正确。
- 模式判定单测 ≥12 例：各优先级组合、TTY/非 TTY、stdin 管道、config 默认、互斥报错（用 `IsTerminal` 注入模拟）。

### Stage 1：输出路由与领域调用层（通用 F2/F3/F4）

**目标**：确立三形态共享的 IO 契约与领域调用封装。

**任务**

- [x] `output.rs`：两层输出抽象——① Format 层（text / json / jsonl / silent 渲染，01 §4.4 信封 `{success, type, entity, data, message, timestamp}`）；② OutputSink 目标层（§2.1：trait + `HeadlessFileSink` / `MemorySink` / `TeeSink` 落地，内存型 MiniSink/TuiSink 延后到 Stage 5/6 引入）。CLI 参数语义：`--output <format>` 保持 05 §5.1（格式），新增 `--log <file>` 为落盘路径（任意模式可与主输出 Tee 分发），两者互不冲突。
- [x] `output.rs` 细节：writer 为 TTY 时开 ANSI、否则纯文本（HeadlessFileSink 内自动开关）；流式 chunk 写入后适时 `flush()`（管道实时性）；文件输出低频 flush（性能）。
- [x] `domain.rs`：`DomainAdapter`——`Runtime::bootstrap(RuntimeConfig)`（复用 wf-runtime bootstrap，含 storage/llm_gateway/tool_registry/mcp_manager 组装）、`api_context()` 访问、`shutdown()` 清理、配置加载（wf-config，含 `cli.default_mode`）；统一的"启动会话/执行"入口（agent 会话与 workflow 前台统一为 `ExecutionType`，对齐 05 §7）。
- [x] `events.rs`：事件适配层——订阅 `ExecutionEvent`（EventBus）与 `AgentStreamEvent`（agent 流），归一为单一枚举流（含 `message` 文本增量、`tool_start/end`、`iteration`、`completed/failed/interrupted`），供 F5 reducer 消费；无 UI 阶段以 mpsc 通道交付。
- [x] `lib.rs` 接线：`run`（无头）子命令走 DomainAdapter + OutputRouter 最小闭环（仅打点，不流式）。

**完成记录（2026-08-17）**

- 新 crate `crates/wf-cli` 落地：`error.rs`（CliError + 退出码 2/3/1/4 映射）、`output.rs`（OutputFormat/OutputMessage/OutputEnvelope/OutputSink/HeadlessFileSink/MemorySink/TeeSink + `Arc<Mutex<T>>` blanket impl）、`args.rs`（Cli/Command/validate）、`mode.rs`（ModeResolver 按 05 §2.2 判定顺序）、`events.rs`（UnifiedEvent + `From<AgentStreamEvent>` + subscribe helper）、`domain.rs`（DomainAdapter + `runtime_config_for_cli`）、`lib.rs`（run/build_sink/run_headless/debug_mode）、`main.rs`。
- 单测 29 个全部通过（output 10 / args 6 / mode 8 / events 2 / domain 2 / error 1）；`domain` 测试真实 bootstrap Runtime 默认配置（memory storage + 空 LLM profiles）并 clean shutdown。
- 环境前置：安装 rustup + stable 1.97 工具链；安装系统 `libluajit-5.1-dev`（wf-sandbox 默认 lua-mlua-sandbox feature 的编译前置）。
- 两个 IO 通道边界：`--log <file>` Tee 落盘任意格式；headless 信封输出走 stdout 主 sink；`--output jsonl` 的 clap 值别名 `jsonlines` 兼容。
- `ExecutionEvent` 无 `Eq`，`UnifiedEvent` 仅 derive `PartialEq`（测试断言足够）。

**交付与验收**

- Output 路由 snapshot 测试（text/json/silent 三种信封形态）；`MemorySink` 断言测试（业务输出 → sink → 断言内容，不依赖真实终端）；`--log` 与 `--output` 组合矩阵（Tee 分发到文件与 stdout）。
- DomainAdapter 集成测试（临时 SQLite storage + mock llm）：bootstrap 成功、api_context 可取、shutdown 幂等。

### Stage 2：无头会话 run（第一个闭环）✅ 已完成（2026-08-18，详见 `wf-cli-stage2-无头会话run-实施方案.md`）

**目标**：`wf run "<prompt>"` 端到端可用——流式输出、审批降级、退出码。不依赖任何 UI 设施。

**任务**

- [x] `run.rs`：会话驱动——启动 agent 会话 → 订阅事件流 → 流式渲染（LlmDelta 按换行/固定缓冲合并后打印 stdout；工具摘要行 `▲/✓/✗` 走 stderr；结束打印 `▣ exec_id · iterations · duration` 摘要行，对齐 05 §5.2）；无输出时打印 `▣` 空会话提示。
- [x] 审批降级策略（对齐 05 §5.3）：敏感工具（approve_changes 等）默认 deny 并打印拒绝原因；低危白名单放行；`--approve-prefix` 预授权（优先级最高：显式同意覆盖敏感判定）；追问（UserInteractionHandler）无 TTY 时拒绝并报错退出（exit 1）。
- [x] 退出码映射（对齐 05 §5.4）+ SIGINT 中断 → exit 4；stdout/stderr 分离纪律（日志一律 stderr，主输出 stdout）。
- [x] stdin 管道路径：`echo "prompt" | wf run`（stdin 非 TTY 读全文）。
- [x] （实施新增）wf-api `RunAgentLoopParams` 扩展（预置 `agent_loop_id` + 注入 `approval_handler`）；修复 wf-agent 两处引擎缺陷——`with_visibility_store` 丢失审批配置、流式工具路径绕过审批管道（见 Stage 2 方案"五·二"）。

**交付与验收**

- 端到端：`wf run "hello"`（mock llm provider）→ stdout 出现文本、stderr 出现工具摘要；`--output json` 信封字段完整；审批 deny 用例（敏感工具被拒 + 原因打印）；退出码矩阵（0/1/2/3/4）。✅（`cargo test -p wf-cli` 50 项全绿，含 8 项 mock LLM e2e）
- 此阶段不引入 ratatui/crossterm 依赖（保持无 UI 依赖，保证领域层可独立验证）。✅

### Stage 3：终端交互设施（通用 F8/F9）

**目标**：UI 形态前置的终端安全设施，与渲染解耦、可独立验证。

**任务**

- [ ] `terminal.rs`：
  - `TerminalGuard`（RAII：进入 raw mode / alternate screen 时登记，Drop 恢复终端状态，对齐 03 §2.1）；
  - `with_restored`（暂停视口 → 恢复终端 → 运行外部命令（$EDITOR 等）→ 重绘，对齐 03 §2.1）；
  - stderr 抑制（`TerminalStderrGuard`，对齐 03 §2.1）；
  - SIGINT 两按计数（5s 窗口，对齐 05 §3.3）。
- [ ] `theme.rs`：主题探测——OSC 10/11 颜色查询（带超时降级）、调色板推导、最后已知良好主题缓存、SIGUSR2 热更新事件；输出 `Theme { bg, fg, accent, ... }` 纯数据（供后续组件消费）。

**交付与验收**

- 单测：TerminalGuard 双次进入/退出状态一致性；SIGINT 两按状态机（合成信号）。
- 冒烟：`wf --mini --demo` 之外的探针命令验证 `with_restored`（启动 $EDITOR 后终端无残留状态）；主题探测在无 OSC 响应时回退默认主题不 panic。

### Stage 4：公共 UI 组件库（通用 F10）

**目标**：mini 与 TUI 共享的组件层（ratatui 0.30 引入点）。

**任务**

- [ ] workspace 依赖新增 `ratatui 0.30` + `crossterm`（正式引入 UI 依赖，Stage 2 的"无 UI 依赖"约束解除）。
- [ ] `scrollback.rs`：`HistoryLine`（提交型行 + 在途 streaming 行，`display_lines(width)` / `desired_height(width)`，宽度变化 reflow，对齐 03 §7、05 §4.2）。
- [ ] `select.rs`：`SelectList`（分组滚动列表，对齐 03 §4.1）。
- [ ] `keymap.rs`：`Keymap`（上下文回退：global → mini/footer → 面板/模态框，对齐 04 §九、05 §4.6）。
- [ ] `framer.rs`：`FrameRequester`（限帧：渲染帧 30-60、输入事件即时唤醒；批量 drain 合并，对齐 03 §3.2、05 §附）。
- [ ] 公共绘制：ANSI 解析管线（ansi-to-tui，对齐 03 §2.3）、终端尺寸变更事件（75ms 防抖，对齐 02 §五 3）。

**交付与验收**

- 各组件 insta snapshot 测试（不同宽度/高度、分组选中、滚动位置）；`display_lines` reflow 单测。
- 组件纯数据化：不直接依赖领域类型（`HistoryLine` 只持有 `Text`/样式），保证可独立测试。

### Stage 5：流式渲染内核（通用 F5/F6）

**目标**：三形态共享的"事件 → 可视内容"数据管线（headless 摘要、mini footer、TUI 屏幕均消费）。

**任务**

- [ ] `reducer.rs`：纯函数归约——`Vec<UnifiedEvent> → (Vec<MiniCommit>, FooterState)`；commit 分组键 `execution_id + iteration + tool_call_id`；连续 LlmDelta 合并（帧内批量）；事件折叠对齐 opencode `footer.append` 微任务合并语义（05 §3.3）；`ids` 幂等去重。
- [ ] `markdown.rs`：streaming markdown——pulldown-cmark 分词 + 按 top-level block（段落/列表/代码块）增量提交，未完结 block 不固化（对齐 05 §3.2）；代码块 streaming 用临时占位行，settle 后替换为语法高亮块（P2 syntect，超限保护对齐 03 §2.3）。
- [ ] `lib.rs`：reducer + markdown 组合出"无头摘要渲染器"（headless 用同一内核，但只输出文本行）——验证数据管线独立于 UI。

**交付与验收**

- reducer 纯函数单测：合成事件序列（含乱序/重复 part）→ commit 序列快照；markdown 增量提交快照（未完结 block 不出现、完结后固化）。
- 无头路径冒烟：`wf run` 文本输出与 mini 滚动区内容同源（同一 reducer 产物），保证形态间一致性。

### Stage 6：mini 模式（形态落地 1）

**目标**：`wf --mini` 可用——inline split-footer 交互会话。

**任务**

- [ ] `footer.rs`：ratatui `Terminal` + `Viewport::Inline(n)`（normal screen 底部视口，对齐 05 §3.1/§4.3）；动态高度（composer/面板/审批/问题/statusline 各视图高度表，对齐 05 §4.3）；statusline 宽度响应式（80/120 断点，对齐 05 §4.4.4）。
- [ ] 输出模型对齐：引入 `MiniSink`（内存型，§2.1）——业务输出追加 scrollback + 标记重绘，由渲染器统一绘制（capture-stdout + ansi-to-tui），业务层不直接 println 写屏；可选 `--log <file>` 时用 `TeeSink` 同时落盘。
- [ ] `composer.rs`：P0 自研单行 Input（历史 ↑/↓ 100 条）；P1 `ratatui-textarea` 多行（≤6 行）+ `@` mention（文件带行号 / 技能 / 工作流，区间高亮对齐 05 §3.2）；`/` 命令 palette。
- [ ] `panels.rs`：模型/技能/排队面板（数据源 llm-profile、SkillLoader、排队队列；Enter 编辑 / Delete 删除）。
- [ ] `approval.rs` / `question.rs`：审批视图（y/a/d/n/c 键位对齐 codex）与追问视图（单选/多选/自定义），接入 05 §3.4 领域映射。
- [ ] 会话语义：串行 turn + 排队；两按退出；退出打印 `wf --mini --session <id>` 续跑提示（对齐 05 §4.5）。
- [ ] `--demo`（前置：Stage 8 的 demo.rs 可先落最小版）：合成事件流驱动完整管线冒烟。

**交付与验收**

- `wf --mini --demo` 冒烟：footer 渲染、composer 输入、@mention 面板、审批/问题视图逐一切换无残留。
- keymap 单测（上下文回退）；statusline 宽度断点快照（<80 / ≥80 / ≥120 列）。
- 退出后终端状态干净（无 raw mode 残留、对话留在 scrollback）。

### Stage 7：完整模式（全屏 TUI，形态落地 2）

**目标**：`wf --tui` 可用——alt-screen 8 屏（复用 Stage 3-5 全部设施）。

**任务**

- [ ] `screens.rs`：8 屏幕（dashboard/workflow/executions/session/checkpoints/search/settings，对齐 02 §二）+ 屏幕栈导航；`Viewport::Fullscreen`（alt-screen）。
- [ ] `modal.rs`：模态框栈（Modal trait + oneshot 结果通道，对齐 03 §4.1）；审批/问题在 TUI 中走模态框（与 mini 的 footer 视图并存，共享 handler 层）。
- [ ] Session 屏幕：前台会话（日志流 + 底部输入 + 状态行 + 阶段感知输入 Idle/Streaming/Approval，对齐 01 §4.4.5）；执行跟踪（六类 ExecutionEvent 过滤，对齐 03 §6.1）。
- [ ] 管理屏幕接入 F2 输出路由（列表/详情数据同一查询域）；CLI-2~4 的 TUI 部分并入本阶段（对齐 01 §六）。

**交付与验收**

- `wf --tui --demo` 冒烟：8 屏导航、模态框开合、会话前台输入输出、resize 正常。
- 与 mini 共享组件复测（同一 HistoryLine/Keymap/主题）；alt-screen 退出恢复原屏。

### Stage 8：收尾（replay / demo / 集成 / 性能）

**任务**

- [ ] `replay.rs`：`--resume`/`--replay` 从存储重放执行事件与消息重建 scrollback（对齐 05 §4.5）；resize 后按新宽度 reflow。
- [ ] `demo.rs`：完整版合成 `AgentStreamEvent` 序列（含权限/问题/工具/错误场景），支撑三形态自动化冒烟。
- [ ] 集成测试套：`wf run` 端到端矩阵（text/json/silent × 审批 × 退出码）；`--mini --demo` 与 `--tui --demo` 冒烟脚本；CI 挂接（对齐 rust 迁移方案验收风格）。
- [ ] 性能基准：帧渲染耗时不随 scrollback 行数线性退化（reflow/截断基准）；reducer 万级事件耗时上界。
- [ ] 文档同步：更新 `docs/cli/01`（CLI-1~4 引用本方案）、AGENTS.md（crate 列表）。

**交付与验收**

- 重放一致性：同一 session 重放产出与原始会话相同 commit 序列（快照对比）。
- 集成测试全绿 + 性能基准达标（渲染 <8ms/帧、reducer <100ms/万事件，超限记录在案）。

---

## 五、阶段依赖与关键路径

```
Stage 0（骨架/模式）──► Stage 1（IO/领域）──► Stage 2（run 闭环）──────────┐
        │                        │                                     │
        │                        └──► Stage 3（终端设施）──► Stage 4（UI 组件）
        │                                              │            │
        │                                              ▼            ▼
        └──────────────────────────────────► Stage 5（流式内核）◄───┘
                                                     │
                                        ┌────────────┴────────────┐
                                        ▼                         ▼
                                 Stage 6（mini）            Stage 7（TUI）
                                        └────────────┬────────────┘
                                                     ▼
                                              Stage 8（收尾）
```

- **关键路径**：Stage 0 → 1 → 2（run 闭环）是主线，先于一切 UI 工作交付，为后续形态锁定领域调用与输出契约。
- **并行可行**：Stage 3（终端设施）与 Stage 1/2 无依赖，可与 Stage 2 并行；Stage 4 依赖 Stage 3。
- **合并建议**：Stage 5 可在 Stage 4 完成 reducer 单测后提前，与 Stage 6 并行开发（mini 的 footer 渲染与 reducer 可分别推进，接口先行约定）。
- **里程碑**：M1 = Stage 2（run 可用，首个可交付版本）；M2 = Stage 6（mini 可用）；M3 = Stage 7（TUI 可用）；M4 = Stage 8（全量收尾）。

---

## 六、测试与验收策略

| 层 | 手段 | 覆盖 |
| :--- | :--- | :--- |
| 纯函数 | 单元测试（同文件 `#[cfg(test)]`） | ModeResolver 判定组合；Output 信封；reducer 归约；markdown 增量；keymap 回退；SIGINT 状态机 |
| 组件渲染 | insta snapshot | scrollback 各宽度 reflow；SelectList 选中/分组；statusline 断点；composer 状态 |
| 领域集成 | 集成测试（`tests/`，临时 SQLite + mock llm） | DomainAdapter bootstrap/shutdown；`wf run` 端到端矩阵；审批 deny/白名单/预授权；退出码映射；重放一致性 |
| 形态冒烟 | `--demo` 合成事件 + 断言脚本 | mini 与 TUI 的视图切换、退出终端状态、resize |
| 性能 | `cargo bench`（`benches/`） | 渲染耗时/scrollback 行数曲线；reducer 万级事件耗时 |

## 七、风险与决策记录

| 项 | 决策 | 理由 |
| :--- | :--- | :--- |
| UI 依赖引入时机 | Stage 2 之前保持无 UI 依赖（clap/tokio 即可跑 run 闭环） | 保证领域层可独立验证，TUI 栈不阻塞主线 |
| 版本固定 | workspace 依赖集中声明 clap 4 / ratatui 0.30 / crossterm / pulldown-cmark / ratatui-textarea / syntect，统一版本 | 对齐 AGENTS.md"Rust deps 集中在根 Cargo.toml" |
| 模块结构 | lib.rs 扁平声明 + main.rs 薄入口（无 mod.rs） | 对齐 AGENTS.md 模块约定，核心逻辑进 lib 保证可测 |
| 日志纪律 | 日志/诊断一律 stderr；stdout 只承载业务输出 | 保证 `wf run | jq` 类管道语义正确（对齐 05 §5.2） |
| 无头与 TUI 同源 | 文本输出与 UI 滚动区消费同一 reducer 产物 | 保证三形态行为一致（对齐 05 §七"事件粒度"决策） |
| `--output` 语义 | 保持 05：`--output <format>` 为格式；落盘用 `--log <file>` | 参考设计中的 `--output ./file`（路径）与 05 `--output text`（格式）命名冲突，Stage 1 定死语义 |
| mini 直接 println | mini 业务输出走内存型 sink + 统一渲染，不直接写 stdout | println 与 ratatui `Viewport::Inline` 光标操作竞争，会破坏底部视口 |
| sink 所有权 | 内存型 sink 经 mpsc 到 UI 单消费者，不引入 `Arc<Mutex<AppState>>` | 现有事件驱动架构（03 §七）已解决跨线程，避免锁竞争 |
| 形态默认值 | config `cli.default_mode` 收敛（方案 A 推荐：--mini 显式） | 对齐 05 §2.3，避免实现期反复 |
| wf-server 关系 | wf-cli 与 wf-server 并列依赖 wf-api，不共享 UI 层 | 避免 TUI 代码泄漏进 server 面 |
