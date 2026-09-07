# wf-cli mini 模式与 opencode TUI 对比分析及改进建议

> 对比基准：`ref/opencode` TUI 会话界面（见 `docs/ref/opencode-tui-实现位置与隔离机制分析.md`）。
> 分析对象：`crates/app/wf-cli/src/mini.rs`（2386 行 MiniApp）+ 关联模块。
> 核心问题：**mini 模式下输入框（composer）与输出完全无法隔离**——流式输出会把输入框冲走、混排，光标与输入文本错位。

---

## 一、wf-cli mini 当前实现机制

### 1.1 渲染底座：inline viewport + 直写终端 scrollback

`mini.rs` 的 `MiniApp::new`（L275-280）：

```rust
let terminal = Terminal::with_options(
    backend,
    TerminalOptions {
        viewport: Viewport::Inline(height),   // 底部 N 行受管视口
    },
)?;
```

- 整个 mini 界面 = **ratatui `Viewport::Inline(height)`**：只有底部若干行（footer）是 ratatui 每帧重绘的受管视口，其余屏幕区域是终端原生 scrollback。
- 输入框（`Footer` → `Composer`，`footer.rs`）渲染在 footer 内；footer 高度 = `FOOTER_BASE_HEIGHT(3) + main`（composer 1 / 面板 16 / 权限 12 / 提问 14）。

### 1.2 输出机制：`insert_before` 直写终端

`mini.rs` `settle_scrollback`（L573-602）：

```rust
self.terminal.insert_before(total_height, move |buf| { ... });
```

- 输出行不经过 ratatui 主 buffer 参与布局，而是通过 `terminal.insert_before` **以 ANSI 序列直接插入到视口上方的终端原生 scrollback**。
- 流式路径（`handle_turn_event` L1374-1400 / `handle_output` L2165-2180）：每个 `LlmDelta`/`Chunk` 把 streaming tail 作为新的 `HistoryLine(LineState::Streaming)` push 进 `pending_scroll`，随即 settle 插入。
- 兜底复杂度：`reflow_scrollback`（L631）、`window_rows` 快照 + common-prefix diff（L613-623）——都是为了在"绕过 ratatui 直写 scrollback"模型下做 resize 重排打的补丁。

### 1.3 输入机制：footer 内单行 composer

- `composer.rs` `Composer::render`（L262-297）：单行输入，grapheme 级光标移动、mention 区间反色高亮。
- 光标定位（`mini.rs` redraw L546-556）：`frame.set_cursor_position((cursor_col, 1))`——**硬编码 y=1**，假定输入行永远在视口第 2 行。

---

## 二、根因分析：为什么输入输出无法隔离

### 根因 1：两套渲染路径在同一个屏幕上拼合（坐标系冲突）

| | 输出 | 输入 |
| :--- | :--- | :--- |
| 渲染路径 | `insert_before` 直写终端原生 scrollback（ANSI 序列） | ratatui buffer 每帧重绘（inline viewport） |
| 空间来源 | 终端自身的滚动历史 | 视口底部 N 行 |
| 布局约束 | 无（行数与 footer 高度、终端 rows 无统一关系） | flex 无关，视口固定 |

两条路径共享同一块物理屏幕，却**没有共享的布局引擎**：输出行数（含换行后行数）与 footer 高度之间没有任何约束关系。ratatui 无法感知"视口上方插入了多少行"，终端也无法感知"视口内部是什么"。

### 根因 2：流式 tail 重复插入，视口被顶出屏幕

- 每次 `LlmDelta` 都把**完整的当前 streaming tail**（`streaming_text()` 返回未提交的全部字节）作为一条新 `HistoryLine` push 并立即 `insert_before`。流式过程中"越来越长的 tail 行"反复插入 scrollback，输出行数飞速增长。
- `insert_before` 插入的行在视口上方 → 终端原生滚动把视口连同输入框一起向上推。输出行与输入行在屏幕上**连续堆叠**，视口被顶出可视区，输入框视觉上"被输出淹没/冲走"。
- 每次重绘后 footer 又回到底部——于是用户看到输入框在输出流中反复跳动、闪烁、混排。

### 根因 3：光标坐标硬编码

`frame.set_cursor_position((cursor_col, 1))` 假定输入行恒在视口第 2 行。但视口高度随视图变化（4/16/12/14 行），且流式插入行后终端滚动位置不定，光标与输入文本实际所在行错位——表现为"光标不在输入框里 / 输入框里没有光标"。

### 根因 4：滚动由终端整屏承担，输入区无独立边界

- 输出滚动不是 scrollbox 内部滚动，而是**终端整屏滚动**（连 footer 一起滚）。
- 输入框只有一行文本，无边框、无背景、无元信息行，与上方输出行视觉上无任何分隔（对比 opencode Prompt 的左边框 + 背景 + agent/model 行）。
- 没有焦点分区：键盘事件全部经 crossterm 收进 `handle_key`，按 keymap 路由——输入态与输出态在状态机层面耦合，界面层面没有体现。

### 根因 5：架构性代价

`settle_scrollback`/`reflow_scrollback`/`window_rows`/`stream_resized` 等机制全部是为"绕过 ratatui 直写终端 scrollback"这个模型打补丁；越补越复杂，隔离问题却越来越难收敛——这正是 opencode 用"单一渲染树 + 局部滚动"从结构上消灭掉的那类复杂度。

---

## 三、opencode 方案与本实现的逐项对照

| 维度 | opencode TUI（参考基准） | wf-cli mini（现状） | 差距 |
| :--- | :--- | :--- | :--- |
| 渲染所有权 | 全屏单一渲染器（OpenTUI），整棵树一个布局引擎 | `Viewport::Inline` 只拥有底部 N 行，输出直写终端 scrollback | 无共享布局，双坐标系拼合 |
| 输出区 | `scrollbox` `flexGrow=1`，滚动发生在内部，sticky bottom | 无滚动容器，依赖终端整屏滚动 | 滚动触及输入区 |
| 输入区 | `flexShrink=0` 独立盒子 + textarea + 边框/背景/元信息行 | footer 内 1 行 composer，无边界 | 视觉与输出无分隔 |
| 光标 | textarea 焦点态（`TextareaRenderable`），光标在输入区内 | `set_cursor_position` 硬编码 y=1 | 光标错位 |
| 流式渲染 | 输出增量渲染进 scrollbox 内部 | streaming tail 反复插入终端 scrollback | 视口被顶出屏幕 |
| 消息/工具行 | scrollbox 内只读块（User/Assistant/ToolPart...） | `HistoryLine` 队列 + insert_before | 见上 |
| resize 处理 | 布局引擎按新尺寸重排，天然正确 | `reflow_scrollback` + common-prefix diff 打补丁 | 复杂且脆弱 |

结论：opencode 的隔离是**结构性的**（渲染树分区 + 滚动局部化 + 输入区自治），wf-cli mini 的混排是**机制性的**（双渲染路径 + 整屏滚动 + 硬编码光标），需要从渲染架构上对齐，而不是继续在现状上加补丁。

---

## 四、改进建议

### 方案 A（推荐）：单一全屏渲染树，输出区/输入区分区

对齐 opencode 的结构：

1. **切到 `Viewport::Fullscreen`**（alt-screen）：整个屏幕归 ratatui 所有，一个 buffer、一个布局。
2. **布局分区**：`Layout::vertical([Min(输出区), Length(输入区)])`：
   - 输出区：自研或复用 `Paragraph`/`List` 做滚动窗口（维护 scroll offset，`sticky bottom` 语义），只渲染最后 `输出区高度` 行；
   - 输入区：固定高度（composer 1 行 + 元信息行 + 分隔），`flexShrink` 等价于 `Constraint::Length`。
3. **流式输出只进输出区内部**：markdown stream 增量渲染进滚动窗口底部，不再 `insert_before` 终端。
4. **光标由布局计算**：`cursor_col` 经 footer 布局换算为绝对坐标，不再硬编码 y=1。
5. **输入区视觉自治**：给 composer 加边框/背景/元信息行（对齐 opencode Prompt），与输出区明确分隔。
6. 滚动/重排全部交给 ratatui 布局，删除 `reflow_scrollback`、`window_rows`、common-prefix diff 等补丁机制。

代价：失去"退出后对话保留在终端 scrollback 可回看"的 inline 特性（可改为退出时把 scrollback 批量打印到 stdout 补偿，openheadless `run` 已有类似输出）。

### 方案 B（保守）：保留 inline，但输出不再直写 scrollback

如果必须保留 `Viewport::Inline`（退出可回看），最低限度改造：

1. **流式 tail 渲染进 footer 内部**：`streaming_text()` 只画在 footer 的一个"流式行"区域（footer 高度动态 = 基础 + tail 行数），不再每条 delta 往 scrollback 塞行。
2. **scrollback 只在 flush/迭代边界批量落定**（一次 `insert_before` 一批完整行，而非每条 delta 一次）。
3. **光标坐标**改为由 footer 当前布局（view × route）计算真实行，不再硬编码。
4. **输入框加视觉边界**（分隔行/背景），与流式行分隔。

### 方案 C（阶段性）：先做 A 的最小闭环

考虑到 wf-cli 已有"全屏 TUI 完整模式"规划（`docs/plan/cli/wf-cli-stage7-*`），建议：

- mini 模式直接复用 fullscreen 渲染底座的分区布局（输出 scrollbox + 底部 prompt），仅关闭完整模式的多面板/侧栏/对话框，形成"精简版全屏会话"——即"类似 opencode --mini"的形态；
- inline 形态的 scrollback 保留能力通过退出时打印补偿。

### 建议的验收标准

1. 流式输出过程中，输入框位置、光标位置、输入文本**零抖动**；
2. 输出滚动只发生在输出区内部，输入区永不被卷入；
3. 输入区有明确的视觉边界，与输出区可一眼区分；
4. resize 后布局自动正确，无需 reflow 补丁。

---

## 五、相关代码索引（供改造时查阅）

| 模块 | 位置 | 说明 |
| :--- | :--- | :--- |
| MiniApp 事件循环 / 渲染 | `crates/app/wf-cli/src/mini.rs` | `run` L334、`redraw` L543、`settle_scrollback` L573、`reflow_scrollback` L631 |
| 流式事件处理 | `crates/app/wf-cli/src/mini.rs` | `handle_turn_event` L1345、`handle_output` L2151 |
| footer / 输入区 | `crates/app/wf-cli/src/footer.rs` | `Footer::draw` L292、`draw_main` L311、`apply_height_with_width` L247 |
| composer 输入框 | `crates/app/wf-cli/src/composer.rs` | `render` L262、`cursor_col` L220 |
| scrollback 行模型 | `crates/app/wf-cli/src/scrollback.rs` | `HistoryLine` / `LineState::Streaming` |
| 流式 markdown 内核 | `crates/app/wf-cli/src/markdown.rs` | `push` L76、`streaming_text` L132 |
| opencode 参考实现 | `ref/opencode/packages/tui/src/routes/session/index.tsx` | Session 布局 L1177-1335 |
| opencode 输入框 | `ref/opencode/packages/tui/src/component/prompt/index.tsx` | Prompt 布局 L1348-1443 |
