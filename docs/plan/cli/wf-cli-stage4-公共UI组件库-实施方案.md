# wf-cli Stage 4 实施方案：公共 UI 组件库（通用 F10）

> 状态：已完成（阶段 4A–4E，2026-08-18）；遗留改进见 §七（I1 `raw_lines` 三通道已随 Stage 6B 落地；I2 宽度键控缓存按性能触发）
> 上游方案：`docs/plan/cli/wf-cli-分阶段实现方案.md`（Stage 4 任务定义）、`docs/cli/03-组件设计方案.md`（组件清单 / Keymap / 绘制调度 / 内容渲染 / resize reflow）、`docs/cli/04-终端交互设计.md` §七（KeymapContext 上下文回退）
> 范围：mini 与 TUI 共享的组件层——`scrollback.rs`（HistoryLine / reflow）、`select.rs`（SelectList 分组滚动列表）、`keymap.rs`（Keymap 上下文回退）、`framer.rs`（FrameRequester 限帧 + 批量 drain）、`ansi.rs`（ANSI → ratatui Text 解析管线）、`size.rs`（终端尺寸变更 75ms 防抖）。**stage 4 是 ratatui 0.30 的正式引入点**（Stage 2/3 的"无 UI 依赖"约束在此解除）。

## 一、现状与缺口

Stage 3 已交付终端安全设施（`terminal.rs` TerminalGuard / with_restored / stderr 抑制 / SIGINT 两按）与 `theme.rs`（OSC 10/11 探测 + 8 角色色 + 缓存 / SIGUSR2），仍**不引入 ratatui**。Stage 4 缺口：

| # | 缺口 | 说明 |
| :- | :--- | :--- |
| G1 | `ratatui` 依赖未引入 | Stage 3 文档已固定 workspace `crossterm = "0.29"`（对齐 ratatui 0.30 系），ratatui 0.30 首次进入依赖图，需由组件库先行落地 |
| G2 | `scrollback.rs` 不存在 | mini 滚动区与 TUI 滚动区都依赖 `HistoryLine`：**持有源文本 + `display_lines(width)` 按宽度重算换行 + `desired_height(width)` 真实视口行数**；提交型行与在途 streaming 行并存（03 §七、05 §4.2、02 文档 resize reflow） |
| G3 | `select.rs` 不存在 | 列表屏唯一导航组件（03 §4.1）：分组滚动列表、`→` 选中标记 + label + description 两列、`(N/M)` 滚动指示、↑/↓/j/k wrap 导航 |
| G4 | `keymap.rs` 不存在 | 键位解析必须收敛到 Keymap（03 §3.1 红线 8：禁止屏幕/组件裸 match）：上下文回退 `modal > 当前屏幕上下文 > global > built-in defaults`（04 §七、05 §4.6） |
| G5 | `framer.rs` 不存在 | 绘制调度合并：`FrameRequester` 以 mpsc actor 合并绘制请求 + `FrameRateLimiter` 限帧（03 §3.2：120FPS 上限 / 8.3ms 最小间隔）+ 批量 drain |
| G6 | `ansi.rs` 缺失 | 工具/命令输出需 ANSI 解析管线 → ratatui `Text`（03 §2.3 ansi-to-tui），tab 先展开 4 空格 |
| G7 | resize reflow 无承载 | 连续 resize 75ms 防抖合并、仅处理最终尺寸（02 文档五·3、03 §2.2），与 `display_lines(width)` 衔接 |
| G8 | 组件不可独立测试 | 组件应纯数据化（不直接依赖领域类型），`HistoryLine` 只持有 `Text`/样式；缺少 golden 输出基建（lib 测试须无副作用，不写文件） |

## 二、外部最佳实践参考（context7 检索结论）

1. **ratatui 0.30（/websites/rs_ratatui_0_30_0）**：
   - `Frame::render_widget(widget, area)` 渲染到当前 Buffer；`Text/Line/Span` 构成文本模型，`Paragraph::new(Text).wrap(Wrap{trim}).scroll((y,x))` 是换行/滚动渲染的标准路径；
   - 自绘组件通过实现 `Widget`（消费）并借助 ratatui 为 `&T` 提供的 blanket impl（`impl Widget for &T where T: WidgetRef`）同时支持 `render_widget(&widget, area)`；样式经 `Style/Stylize`（`.fg/.bg/.add_modifier/.dim/.bold/.cyan`）链式表达；
   - **ratatui 不做换行缓存**：wrap/reflow 每次按当前宽度重算（正合 03 §2.2/§七"禁止固定宽度渲染缓存"红线 9）。
2. **crossterm（Stage 3 既有结论）**：`event::poll(Duration)` + `read()` 带超时读事件；`Event::Resize(u16,u16)` 是 resize 事件来源——组件层只消费尺寸，与实际事件循环解耦。
3. **仓内既有范式**：`run.rs` 的"DiagWriter 可注入缓冲"、`theme.rs` 的"注入时钟/环境做纯函数单测"——本阶段各组件以**可注入时钟 / 纯数据**实现，无 TTY 单测（FrameRequester 注入毫秒时钟、ANSI 解析器接受字节切片、Keymap 纯查表、HistoryLine 注入宽度）。

## 三、关键设计决策

| # | 决策 | 理由 |
| :- | :--- | :--- |
| D1 | **`ratatui = "0.30"` 全 feature 引入**（含 crossterm backend），与 workspace `crossterm = "0.29"` 配套；同时新增 `unicode-width`（文本宽度/grapheme 截断，组件换行统一用它）。**不引入 insta**：golden 输出改由专用 `example` 生成到 `wf-cli/outputs/` | 对齐 AGENTS"依赖集中在根 Cargo.toml"；宽度语义与 ratatui 一致（03 §六 文本工具）；lib 测试保持无副作用 |
| D2 | **`HistoryLine` 纯数据化**：持有 `Kind`（Commit/Streaming）+ `Text`（ratatui，源文本）+ `Role` 轻标记（来自 `Theme` 角色的引用，映射到 `Style`）；`display_lines(width) -> Vec<Line>` 将源文本按宽度（`unicode-width` 总宽 + 逐图素换行）重算换行（reflow 地基）；`desired_height(width) -> u16` 为该宽度下总行数（滚动区/80x24 裁剪依赖） | 对 03 §七"提交型行与在途 streaming 行并存，commit tick 沉淀"；换行缓存在此不做（红线 9），宽度变化即重算 |
| D3 | **`SelectList` = 分组 + 单一选中游标 + 视口滚动**：`groups: Vec<Group>`（每组 item 列表）、选中游标以**扁平序号**统一（跨组递增，`selected` 组内相对序号派生）；`navigatable`→`select_next/prev(wrap)`、`visible(window)` 计算视口窗口与 `(N/M)` 滚动指示；可选 `filter` 模糊过滤（P0 子串匹配） | 分组滚动（03 §4.1）不与具体领域绑定；视口窗口纯计算可测；item 的 `data` 用泛型 `T` 携带领域数据或 `()` |
| D4 | **`Keymap` = 三表上下文回退**：`global_bindings`（不可覆盖基础）+ `context_bindings: HashMap<Ctx, HashMap<Key, A>>` + `builtin_defaults`；`resolve(ctx, key)` 顺序 `context → global → builtin`，内置默认表各上下文 P0 子集（对齐 04 §八 List/Detail/Chat/Input/Modal + Global）；`Key` 用 `KeyEvent { code, modifiers }` 轻量结构（避免把 crossterm 泄漏进所有权） | 04 §七 优先级 + 红线 8；纯查表可无 TTY 断言回退顺序 |
| D5 | **`FrameRequester` actor + 注入时钟**：`request_frame()` / `request_frame_in(dur)` 合并请求为一次 `next_frame_deadline`；`FrameRateLimiter` 保证两次*done*间 `MIN_FRAME_INTERVAL(8.3ms)`；`deadline(now)` 返回 `Some(Instant)` 供事件循环 `select!` 定时分支；时钟可注入（毫秒）做限帧单测 | 03 §3.2；不绑定 tokio，`deadline()` 纯计算，调用方（Stage 6/7 事件循环）自行 select |
| D6 | **`AnsiParser` 自研最小 SGR 解析（不引入 ansi-to-tui 外部 crate）**：字节流状态机识别 `\x1b[...m`（SGR 支持 16/256/truecolor）、其余 CSI/OSC 序列剥离；产出 `Vec<Line>`（现色 `Style`）；tab 先展开 4 空格 | 外部 ansi-to-tui 未收录于 context7 且版本耦合高（03 提到但 P2）；最小自研覆盖 CLI 输出的 SGR 子集，纯字节→Text 可测；全量 ANSI（超链接/文档模式）留 P2 |
| D7 | **`ResizeDebouncer`：75ms 防抖合并**：`push(size, now)` 记录待定尺寸与最后变更时刻；`settle_if_elapsed(now)` 在 75ms 内返回 None、超时后返回最终尺寸并清空（02 文档五·3 / 03 §3.1）；`reflow` 触发职责归调用方（拿到尺寸后对 scrollback 调 `display_lines(width)`） | 事件循环合并 resize 的纯组件；时钟注入可测防抖窗口 |
| D8 | **渲染仅承载 Lines → Buffer**：`LinesView` 轻量 Widget（`lines: Vec<Line>` + `scroll_offset`），实现 `Widget`；不是每个组件都自己拼 Buffer，统一由它把 `display_lines`/`SelectList::render_lines` 产物绘入 Buffer | 组件 = 纯数据 + 幂等渲染（03 核心决策 4）；Buffer diff 交给 `ratatui::Terminal`（Stage 6/7 引入） |
| D9 | **组件不直接依赖领域类型**：`HistoryLine`/`SelectList`/`Keymap` 均无 `UnifiedEvent`/`ExecutionEvent` 引用；数据流在 Stage 6（reducer）接缝注入 | 对齐"组件纯数据化"验收；独立可测 |
| D10 | **golden 输出由专用 `example` 生成，lib 测试保持无副作用**：`component_output` example 把确定性渲染结果写入 `wf-cli/outputs/*.txt`；lib 测试只在内存里做精确断言比对，不写文件（如确需写盘则用 `tempfile` dev-dep） | src 不应被生成文件污染；lib 测试须可纯重放；golden 文件落 `outputs/` 便于审阅与 diff |

## 四、模块落点

```
crates/wf-cli/src/
├── scrollback.rs   ← 新增：HistoryLine{Kind,Text,role} + display_lines/desired_height + LineWrap（宽图素换行）+ LinesView Widget
├── select.rs       ← 新增：SelectList<T>{groups,flat_index,selected,filter,wrap} + Group/GroupItem + 导航/视口 + 渲染
├── keymap.rs       ← 新增：Key{CKey,Mods} + KeyAction + Keymap{global,context,builtin} + resolve 上下文回退
├── framer.rs       ← 新增：FrameRequester(actor/注入时钟) + FrameRateLimiter + deadline/next_deadline
├── ansi.rs         ← 新增：AnsiParser + SgrState(现色 Style) + parse_to_text + tab 展开
├── size.rs         ← 新增：ResizeDebouncer(75ms 防抖) + SettledSize{width,height}
├── lib.rs          ← 接线：pub mod scrollback/select/keymap/framer/ansi/size + pub use 核心类型
│                    + 内存断言辅助（render_lines -> 字符串，无副作用）
crates/wf-cli/examples/
├── component_output.rs ← 新增：golden 生成 example —— 调 scrollback/ansi/select 纯 API 把渲染结果写入 outputs/*
└── outputs/            ← 生成的确定性 golden 文件（reflow_w12/w40、ansi_mixed、select_wide/narrow），可审阅/diff
Cargo.toml（root）  ← workspace.dependencies 新增 ratatui="0.30"、unicode-width（不引入 insta）
crates/wf-cli/Cargo.toml ← 依赖 ratatui、unicode-width；dev-deps 仅 tempfile（不听凭文件 IO 写盘）
```

## 五、分阶段任务与验收

### 阶段 4A：依赖引入与模块接线（G1/G8 基础）

- [x] root `Cargo.toml` workspace 依赖新增 `ratatui = "0.30"`、`unicode-width`（**不引入 insta**）；校验 ratatui 0.30 与现有 `crossterm = "0.29"` 版本配套。
- [x] wf-cli 增 `ratatui`、`unicode-width`；dev-deps 仅 `tempfile`（lib 测试如需临时文件 IO 使用之，黄金输出统一走 example）。
- [x] `lib.rs` 声明 `pub mod scrollback / select / keymap / framer / ansi / size` 并 `pub use` 核心类型；各模块先落空结构 + `#[cfg(test)]` 最小编制测试占位（保证 `cargo check -p wf-cli` 立即通过）。

**验收**：`cargo check -p wf-cli` 通过（无未用告警）；`wf-cli` 依赖图包含 ratatui。

### 阶段 4B：scrollback.rs HistoryLine（G2 主体）

- [x] `HistoryLine::new0`（提交型）/ `streaming(Text)`（在途）：`kind`、`text`（源 `Text`）、`role: Role`（`Default/Muted/Accent/Error` 等轻量枚举，Stage 6 映射到所属主题 `Style`）。
- [x] 换行内核 `wrap_grapeheme(line: &Line, width) -> Vec<Line>`：`unicode-width` 累计单行总宽，超宽断行（整个 `Span` 作为不可断 token，内部按图素截断）；`display_lines(width)` 对源 `Text` 每行换行后扁平化为 `Vec<Line>`。
- [x] `desired_height(width) -> u16`：`display_lines(width).len()` 的 `u16` 化（供滚动区/80x24 裁剪）。
- [x] `LinesView<'a>` 轻量 Widget：`lines + scroll_offset`，`Widget` 逐行写入 Buffer（越界行裁剪，`set_line` / 空格铺底）。
- [x] 单测：宽度变化 reflow（同一源文本在不同 width 产出不同行数）、`display_lines` 在宽图素（中/emoji）处不回退、`desired_height` 一致性、`LinesView` 渲染行数与 scroll 偏移快照。

**验收**：`cargo test -p wf-cli`（scrollback 模块）全绿；lib 测试无文件写副作用，黄金渲染由 `component_output` example 生成覆盖至少 3 种宽度/行类型。

### 阶段 4C：select.rs + keymap.rs（G3/G4）

- [x] `SelectList<T>`：`Group { title: String, items: Vec<GroupItem<T>> }`；`flat_index`（跨组扁平游标）+ `selected`（组内相对），`len()`/`navigate(dir, wrap)`（dir ∈ Prev/Next，wrap 收尾回绕）；`current()`/`selected_item_data()`；`filter`（P0 子串匹配，命中项过滤出候选集并可导航）。
- [x] 视口计算：`visible(window_height) -> (usize, usize, Vec<usize>)` 返回滚动窗口内可见扁平索引 + `(N/M)` 计数；`scroll_to_selected(window_height)` 保选中可见（对齐 03 §4.1）。
- [x] `SelectList` 渲染：`render_lines(window_height) -> Vec<Line>` 或轻量 `select_lines`（不提 `Block`/边框，红线 1）：`→` 选中标记 + label（+ description 两列，宽 > 40 才显示）单行截断；`(N/M)` 滚动指示写入状态行由调用方组合。
- [x] `Keymap`：`Key { code, modifiers }`（`CKey::Char/Enter/Esc/Up/Down/Page/Home/End/Ctrl…`）；`KeyAction::Quit/Redraw/Help/Palette/Submit/NavigatePrev/Next/Select/Back/Refresh/Delete/New/…`；`KeymapBuilder`（`global()`/`context(ctx, …)`/`with_builtin_defaults()`）；`resolve(ctx, key)` 回退顺序断言。
- [x] golden 输出：`component_output` example 覆盖 SelectList 分组选中/滚动位置渲染；Keymap 各上下文查表结果用内存断言。

**验收**：组件导航与过滤单测全绿；resolve 回退顺序单测；黄金输出覆盖分组、滚动、过滤。

### 阶段 4D：framer.rs + ansi.rs + size.rs（G5/G6/G7）

- [x] `FrameRateLimiter`：`MIN_FRAME_INTERVAL = 8.3ms`；`next_deadline(now, last_done) -> Instant` 保证最小帧间隔（120FPS 上限）。
- [x] `FrameRequester`：`request_frame()` / `request_frame_in(dur)`（多请求合并若干次 `next_deadline` = min）；`deadline(now, last_frame)` 返回 `Option<Instant>`（无请求 → None）；毫秒时钟注入（`Clock: Fn() -> u64` 或存注入值）。
- [x] `AnsiParser`：`parse(&[u8]) -> Vec<Line>`；`SgrState` 现色（fg/bg/Modifier）；支持 `\x1b[38;5;Nm` / `\x1b[48;5;Nm`（256 色）、`\x1b[38;2;R;G;Bm`（truecolor）、基础 `\x1b[0m`/`\x1b[1m`/`\x1b[30-37m`；其余 CSI/OSC 序列剥离；tab(`\t`)展开 4 空格；文本按 `\n` 分行为 `Line`。单测：纯文本、混色 256、truecolor、未知序列剥离、tab 展开。
- [x] `ResizeDebouncer`：`new(window: Duration)`；`push(size, now)`；`settle_if_elapsed(now) -> Option<Size>`（window 内未超 → None；超时第一次调用返回最终尺寸并清空）。

**验收**：framer/ansi/size 单测全绿；黄金输出覆盖 ANSI 解析结果；限帧 deadline 走内存断言；`wf-cli` 全量 `cargo test` 通过。

### 阶段 4E：勾选与收尾

- [x] 勾选总方案 Stage 4 任务项；补完成记录。
- [x] 生成 patch（排除构建产物：target/、Cargo.lock 不动；新增 docs/plan/cli 方案 + src 源码 + examples/component_output.rs + outputs/*）。

**验收**：本方案全部任务勾选；`cargo test -p wf-cli` 全绿；patch 校验 `grep -c 'target/'` = 0。

## 五·一、与方案的偏差（实施期决策）

| # | 偏差 | 原因 |
| :- | :--- | :--- |
| P1 | ANSI 解析自研最小 SGR 子集，**不引入 `ansi-to-tui` 外部 crate**（03 §2.3 原引 ansi-to-tui） | context7 索引未收录该库、版本与 ratatui 0.30 耦合度高；CLI 输出的 SGR 子集自研足够，留 P2 扩容 |
| P2 | SelectList 泛型数据 `T`（可 `T=()`），不预绑定领域 item 结构 | 组件纯数据化红线；Stage 6/7 各自映射领域对象 |
| P3 | `Key` 自研轻量结构，不直接用 crossterm `KeyEvent` | 组件层不依赖 crossterm 事件类型；Stage 6 事件归一后作适配 |
| P4 | Lines 渲染用 `LinesView` 统一承载，而非每个组件拼 Buffer | 减少重复渲染样板，Buffer diff 统一交给 ratatui Terminal |
| P5 | golden 基建：lib 测试用 `render_lines_to_string` 做**内存断言**（无副作用）；确定性黄金文件由 `component_output` example 写入 `wf-cli/outputs/` | lib 测试须无副作用、可重复；黄金输出移到 `outputs/` 不污染 `src/` |

## 六、风险与边界

| 风险 | 缓解 |
| :--- | :--- |
| ratatui 0.30 与 crossterm 0.29 版本匹配 | workspace 已固定 crossterm 0.29；ratatui 0.30 默认 crossterm backend 同版本，`cargo check` 即校验 |
| 宽字符（CJK/emoji）换行宽度 | 换行内核统一走 `unicode-width`，与 ratatui 宽度语义一致；红线 9 禁止宽度缓存 |
| 大量 ANSI（文档模式/超链接） | 本阶段最小 SGR 子集解析+剥离其余；P2 再扩容（syntect/超链接在 Stage 5 之后） |
| 组件被渲染前 resize | HistoryLine 持有源文本，宽度变化即重算；ResizeDebouncer 合并连续变更 |
| 黄金输出文件膨胀 | 只用少量核心输出（reflow/选中/ANSI）；大断言走内存单测 |
| 组件不接领域/不渲染终端 | 本阶段只交付可测组件与纯数据管道；ratatui `Terminal`、事件循环、reducer 接入均在 Stage 5/6 |

## 七、已知改进项（codex 对照分析定位）

对照 `docs/analysis/` 下 codex 历史分析（`history.md` §2.1）与 `wf-agent-learnings.md` §5/§7.6，本阶段交付的 `HistoryLine` 存在两项后续改进：

| # | 改进项 | 说明 | 排期 |
| :- | :--- | :--- | :--- |
| I1 | **`HistoryLine` 三通道（display/raw/height）** | codex `HistoryCell` trait 提供 `display_lines`（显示）/ `raw_lines`（复制友好的纯文本视图）/ `desired_height` 三通道；本阶段只有 display/height。Stage 6 起补 `raw_lines()` 供复制模式 / 转录浮层 / 会话导出共用（`Role` 之外的单元格语义枚举化） | Stage 6B 呈现层（`wf-cli-stage6-mini模式-实施方案.md` D13）✅ 已完成 2026-08-22（`scrollback.rs::raw_lines`，消费方：mini `window_rows` 快照/D19 公共前缀 diff；复制/转录/导出待 6C+ 消费） |
| I2 | **宽度键控缓存澄清（红线 9 补充说明）** | 红线 9"禁止固定宽度缓存"语义是"宽度变化必须重算"，**宽度键控**缓存（键含 width，resize 即失效）与 committed 区域行缓存不违反红线（codex `MarkdownRenderCache`/`StablePrefixLenCache` 均以宽度为键）；流式高频重渲性能需要时允许引入 | 总方案风险表已同步；性能触发时落地 |