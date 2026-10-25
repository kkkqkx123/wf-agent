# 面板文本选择能力差距分析与借鉴（对照 paperboy-tui）

- 分析日期：2026-10-24
- 分析对象：`wf-agent` 的完整 TUI（`wf-tui`）与 TUI 公共层（`tui-core`、`tui-render`、`tui-terminal`、`tui-components`）
- 对照对象：`paperboy-tui` 的 `tui-panel-select` crate
- 关联文档：`docs/plan/tui-panel-select-design.md`

## 摘要

`wf-agent` 已经吸收了 `tui-panel-select` 的主要底层能力：逻辑坐标与流式选择几何、窗口化换行缓存、本地工具优先的剪贴板回退、复制策略与终端守卫。这些能力集中在 `tui-core`、`tui-render`、`tui-terminal`，并通过 `wf-cli-demo` 的双面板示例验证。

当前缺口在壳层集成：`wf-tui` 的会话滚动区仍以视口显示行列保存选择状态，滚动、缩放、重排会使选区失效，拖拽自动滚动还会让锚点内容漂移；命中测试与复制路径对全部 scrollback 行重复执行 reflow，没有复用已有的 `PreparedScrollback` 缓存；键盘选择、文件查看器选区、样式化与 ANSI 内容选区仍未接入。

建议按三个优先级推进：

- P0：把 transcript 选区迁移到 `TextPos` 逻辑坐标，命中与复制复用 `PreparedScrollback`，补齐键盘扩展选择。
- P1：抽出可复用的 `SelectablePanel` 状态封装，接入文件查看器、diff 查看器与历史分页器；为 styled/ANSI 内容建立纯文本投影。
- P2：多区域选择、续行标记、跨面板组合复制。

`tui-rgb-picker` 与当前 TUI 需求无关，不纳入范围。

## 一、范围与判定标准

本次分析覆盖以下交互面：

- 会话滚动区（transcript）的鼠标拖选、双击选词、三击选行、右键选行、拖出边界自动滚动。
- 文件查看器、diff 查看器、历史分页器的文本选择与复制。
- 剪贴板写入链路与复制策略。
- 键盘选择与无障碍路径。
- 样式化文本、ANSI 工具输出、Markdown 渲染内容的选区映射。

判定标准：

- 选区是否在面板矩形内，且不跨边框、不串相邻面板。
- 选区在 resize、重排、滚动后是否仍指向同一批字符。
- 大文本、超长单行、长会话下的事件开销是否与可见窗口相关。
- 未开启鼠标捕获时是否仍有可用的选择与复制路径。
- 复制内容是否与用户看到的文本一致，且不夹带 gutter、徽标等装饰字符。

## 二、结论

1. 底层原语已经齐备，缺口集中在 `wf-tui` 壳层接线。`docs/plan/tui-panel-select-design.md` 的落点设计已大部分实现，方案第五节最后一条“把显示坐标换成逻辑位置”仍未完成。
2. 需要补，且优先级明确。逻辑坐标迁移与键盘选择直接决定功能是否可用；多区域与续行标记属于体验增强。
3. 借鉴重点在状态组织与几何复用，复制链路已经达到并部分超过对照对象，应保留现状。
4. `wf-mini` 采用无鼠标捕获的追加式模型，不纳入本次范围。

## 三、现状盘点

| 能力 | wf-agent 现状 | 位置 | 差距 |
|---|---|---|---|
| 逻辑文本位置 `TextPos` | 已有 | `crates/app/tui/tui-core/src/panel_select.rs` | 无 |
| 流式选择几何 | 已有 | `panel_select.rs:57` `PanelGeometry` | 未用于 transcript |
| 选区文本提取 | 已有 | `panel_select.rs:179` | 未用于 transcript |
| 可见窗口高亮格 | 已有 | `panel_select.rs:213` | 未用于 transcript |
| 装饰字符排除 | API 已有 | `panel_select.rs:368` `strip_positions` | transcript 未使用 |
| 窗口化换行缓存 | 已有 | `crates/app/tui/tui-render/src/panel_wrap.rs:38` | 仅 demo 使用 |
| 按 `Arc` 身份失效 | 已有 | `panel_wrap.rs:38` | 仅 demo 使用 |
| 本地剪贴板工具优先 | 已有 | `crates/app/tui/tui-terminal/src/clipboard.rs:155` | 无 |
| OSC 52 回退与 tmux 包裹 | 已有 | `clipboard.rs:101` | 无 |
| 剪贴板模式固定与测试禁用 | 已有 | `clipboard.rs:56`、`:83` | 无 |
| 复制能力探测与拒绝提示 | 已有 | `crates/app/tui/tui-core/src/copy.rs:47` | 无 |
| 终端模式恢复与 panic hook | 已有 | `crates/app/tui/tui-terminal/src/terminal/guard.rs` | 无 |
| 滚动条显隐与轨道映射 | 已有 | `crates/app/tui/tui-components/src/scrollbar.rs` | 无 |
| 预排缓存与行源索引 | 已有 | `crates/app/tui/tui-render/src/prep_cache/scrollback.rs` | 选区路径未复用 |
| 会话拖选（显示坐标） | 已有 | `crates/app/cli/wf-tui/src/interactive.rs:164` | 坐标不稳定 |
| 单选面板高层封装 | 缺失 | 仅 demo 内手写 `DemoPanel` | 有 |
| 多区域选择 | 缺失 | 无 | 有 |
| 键盘扩展选择 | 缺失 | 无 | 有 |
| styled / ANSI 选区 | 缺失 | 渲染有 ANSI 解析，选区未接 | 有 |
| 续行标记 | 缓存原语支持 | `panel_wrap.rs` 的 `reserve_marker` | transcript 未启用 |
| 文件查看器选区 | 缺失 | `tui-components/src/file_viewer.rs:59` | 只能整篇复制 |
| 壳层选区测试 | 缺失 | 现有测试以几何与快照为主 | 有 |

## 四、已落地能力

### 4.1 逻辑坐标与流式几何

`PanelGeometry` 以每行显示宽度、有效 wrap 宽度、wrap/clip 模式和累计行数为输入，提供双向映射：

- `textpos_to_row_col`：逻辑位置到当前帧行列。
- `row_col_to_textpos`：当前帧行列到逻辑位置。
- `point_to_textpos`：先钳制到面板矩形，再映射。
- `extract_text`：按流式语义提取首行部分、中间整行、末行部分。
- `highlight_cells`：只扫描可见窗口。

这套几何与 `paperboy-tui` 的 `crates/tui-panel-select/src/selection.rs` 同构，已经具备“逻辑位置为唯一真相”的基础。

### 4.2 窗口化换行缓存

`PanelWrapCache` 持有源文本、按行拆分的字符串、每行宽度和 `PanelGeometry`。`set_content` 以文本 `Arc` 指针身份、宽度、模式、标记保留四项判断是否重建；`visible_window` 只物化视口内的行；超长行通过字符窗口跳过前缀，不整行物化。

这意味着每一帧调用 `set_content` 的代价很低，适合直接放进绘制路径。

### 4.3 剪贴板与复制策略

`tui-terminal` 的剪贴板实现已经具备：

- 本地工具优先：`xclip`、`xsel`、`wl-copy`、`pbcopy`、`clip.exe`。
- OSC 52 作为最后回退，含 tmux pass-through。
- 模式固定：`ClipboardMode`、进程内开关、`TUI_PANEL_SELECT_CLIPBOARD` 环境变量。
- 子进程脱离与后台回收，避免僵尸进程。
- 失败可检测：本地工具返回失败时继续回退，OSC 52 放在最后。

`tui-core` 的 `CopyPolicy` 进一步加入终端能力探测、是否 TTY、拒绝原因与用户提示，这部分强于对照对象，应保留。

### 4.4 终端守卫与滚动条

`TerminalGuard` 以可注入的 `TerminalControl` 管理 raw、alt-screen、bracketed paste、focus、mouse capture、keyboard enhancement 等模式，进入与恢复按增量执行，`Drop` 兜底。该设计与 `paperboy-tui` 的 `TerminalGuard` 目标一致，且更适合现有分层。

`tui-components` 的滚动条已提供显隐判断、tail 偏移换算、内容区与 gutter 切分、gutter 命中，`wf-tui` 已在其上实现拇指拖拽与键盘滚动。滚动条部分不需要再做。

### 4.5 演示体系

`crates/app/cli/wf-cli-demo/examples/tui/panel_select_showcase.rs` 用左右双面板验证了换行、裁剪、续行标记、逻辑坐标选区与复制预览。该示例是当前底层能力的唯一真实调用方，说明原语可用，但尚未进入产品路径。

## 五、缺口分析

### 5.1 P0：transcript 选区仍是视口显示坐标

`TranscriptSelection` 保存四个显示空间数值：

```text
anchor_row, anchor_col, head_row, head_col
```

位置：`crates/app/cli/wf-tui/src/interactive.rs:164`。

相关行为：

- `clear_hit_frame` 在 resize 或 overlay 切换时清空选区与命中快照：`interactive.rs:800`。
- `clear_transcript_selection` 的注释明确说明 release、scroll、resize 都会清空：`interactive.rs:846`。
- 滚轮滚动直接清空选区：`crates/app/cli/wf-tui/src/tui/mouse.rs:59`、`:70`。
- 拖拽自动滚动只把 `head_row` 固定到视口边缘，`anchor_row` 不变：`interactive.rs:868` 到 `:891`。

后果：

- 用户在长输出中拖选时一旦滚动，选区丢失。
- 拖出边界自动滚动期间，锚点行号不变但对应内容已经随视口移动，选区起点漂移。
- 缩放或换行宽度变化后，同一显示行列对应不同字符，无法保证“选中同一批字符”。

这是与对照对象核心承诺的直接差距。`paperboy-tui` 的选区只存 `(TextPos, TextPos)`，视口变化只影响绘制时的映射，不影响选择语义。

### 5.2 P0：命中测试与复制按全量 reflow

三个高频路径都会重建全部显示行：

- `seq_at_display_row`：`crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:152`。
- `plain_text_for_display_row`：`mouse_hit.rs:303`。
- `transcript_text_in`：`mouse_hit.rs:259`。

实现方式是对 `self.scrollback` 中每一行调用 `display_lines(width)`，再累加高度定位目标显示行。点击、双击、三击、右键、拖选释放都会触发。

项目已有的 `PreparedScrollback` 已经缓存了：

- 已排版的 `rows: Vec<Line<'static>>`。
- 每个源行对应的 `RowRange` 索引。
- 每源行身份指纹与 plain/highlight 状态。
- 流式尾部单独处理。

位置：`crates/app/tui/tui-render/src/prep_cache/scrollback.rs`。

选区与命中路径绕过了这份缓存，导致长会话、大工具输出、频繁点击时出现不必要的 O(n) 排版。对照对象的做法是用每行字符长度与累计行数前缀和，配合二分定位，代价与可见窗口和选中行数相关。

### 5.3 P0：键盘选择缺失

鼠标捕获默认关闭：

- Full、Reduced、Minimal 三档均设置 `enable_mouse_capture: false`：`crates/app/tui/tui-core/src/perf.rs:149`、`:165`、`:181`。
- 用户需要通过配置显式开启：`crates/app/cli/wf-tui/src/tui/input_classify.rs:232`。

transcript 没有键盘选区扩展：

- `interactive/keys.rs` 只有会话导航、composer 编辑与面板导航。
- 没有 Shift+方向、Shift+Home/End、Shift+PgUp/PgDn、Ctrl+A 的选区语义。
- 没有选区锚点、光标移动与清除的键位映射。

后果：未开启鼠标的用户只能使用 `/copy` 整段复制，无法复制部分输出。这个缺口的用户覆盖面大于多区域选择。

### 5.4 P1：文件查看器与历史分页器只能整篇复制

`FileViewer` 与 diff 查看器的复制接口返回全部内容：

- `crates/app/tui/tui-components/src/file_viewer.rs:59`。
- `file_viewer.rs:165`。

历史分页器同样以整篇复制为主。日志、diff、大文件是选择与复制最自然的场景，接入面板级选区收益直接。

### 5.5 P1：styled / ANSI 内容缺少双视图

渲染侧已有 ANSI 解析：

- `crates/app/tui/tui-render/src/ansi.rs`。

transcript 侧存在两种文本：

- `source_text()`：源文本，保留 Markdown 标记。
- `display_lines(width)`：排版后的带样式行，Markdown 标记已被解释。

视觉选区应当基于用户看到的排版文本，`/copy` 等显式命令可以保留源文本。当前缺少统一的纯文本投影，直接基于 `source_text` 建几何会在 Markdown、ANSI、语法高亮场景下错位。

对照对象的 `set_styled_content` 与 `set_ansi_content` 正是把渲染与选择分离：渲染保留 span 样式，几何与复制操作纯文本。

### 5.6 P2：多区域选择与续行标记

对照对象的 `MultiSelectPanel` 支持：

- 一个活动区域加多个已固定区域。
- Alt+Click 追加区域。
- `Motion` 键盘扩展。
- `selected_parts` 按逻辑位置排序输出。
- `autoscroll_tick` 由面板自持滚动。

`wf-agent` 当前只有单区域，且滚动由会话层掌握。多区域可作为后续增强；续行标记的缓存原语已存在，transcript 尚未启用。

### 5.7 文档滞后

`docs/cli/04-终端交互设计.md` 仍写“无鼠标交互”，与 `crates/app/cli/wf-tui/src/tui/mouse.rs` 的实际实现不一致。`docs/research/tui/01-current-tui-interface-inventory.md` 也把部分组件标为未接线。接线完成后应同步更新。

## 六、值得借鉴的设计

### 6.1 逻辑坐标是唯一真相

机制：

- 选区状态只保存 `(TextPos, TextPos)`。
- 面板矩形与滚动偏移只在事件发生和绘制时参与映射。
- 缩放、重排、滚动不改变选择语义。

适配：

- 把 `TranscriptSelection` 改为逻辑位置。
- 保留 `row_span` / `edge_cols` 之类的显示辅助方法时，改为从逻辑位置按当前几何派生。

参考：`panel_select.rs:57`，`paperboy-tui/crates/tui-panel-select/src/selection.rs`。

### 6.2 两层 API：原语加开箱即用封装

机制：

- 无状态原语负责几何、提取、高亮。
- 有状态封装负责内容缓存、选区、可选滚动与鼠标事件解释。

适配：

- 在 `tui-render` 增加 `SelectablePanel`，组合 `PanelWrapCache` 与 `Option<(TextPos, TextPos)>`。
- `wf-tui` 只负责事件路由与视觉主题。
- 文件查看器如果需要在 `tui-components` 内自持状态，可将 `PanelWrapCache` 下沉到 `tui-core`，避免 `tui-components` 反向依赖 `tui-render`。

参考：`paperboy-tui/crates/tui-panel-select/src/lib.rs`、`panel.rs:130`、`multiselect.rs:87`。

### 6.3 换行缓存的失效条件与窗口化取行

机制：

- 以文本 `Arc` 指针、宽度、模式、标记保留作为失效条件。
- 每行字符长度与累计行数前缀和。
- 逻辑行与 wrapped row 之间二分定位。
- 只物化可见窗口，超长行按字符窗口切片。

适配：

- 直接复用 `PanelWrapCache`。
- 将 `PreparedScrollback` 的 `RowRange` 索引作为“显示行到源行”的快速通道。
- 命中测试先定位源行，再在该行的几何内定位列。

参考：`crates/app/tui/tui-render/src/panel_wrap.rs:38`、`crates/app/tui/tui-render/src/prep_cache/scrollback.rs`。

### 6.4 流式语义与装饰字符排除

机制：

- 首行从点击列到行尾，中间整行，末行从行首到释放列。
- 逆向拖拽先排序端点。
- `exclude: HashSet<TextPos>` 剔除纯视觉字符。
- 纯空白选区返回 `None`。

适配：

- transcript 的 gutter、徽标、状态图标应在复制时排除。
- 复制结果应经过现有 `sanitize_copy_text`。

参考：`panel_select.rs:179`、`panel_select.rs:368`。

### 6.5 键盘 Motion 语义

机制：

- `Left`：左移一个字符，跨到上一行行尾。
- `Right`：右移一个字符，跨到下一行行首。
- `Up` / `Down`：移动一行，尽量保持列。
- 扩展时把光标滚动到可见区域。

适配：

- 在 transcript 增加 `SelectionMotion` 与 `extend_selection(motion)`。
- 键位建议：`Shift+Left/Right/Up/Down`、`Shift+Home/End`、`Shift+PgUp/PgDn`、`Ctrl+A` 全选当前面板、`Esc` 清除。
- 不要求鼠标捕获，直接走键盘事件。

参考：`paperboy-tui/crates/tui-panel-select/src/multiselect.rs:41`、`:467`。

### 6.6 styled / ANSI 双视图

机制：

- 渲染行保留 span 样式。
- 选区、几何、复制基于同一批行的纯文本投影。
- ANSI 文本在构建缓存时剥离控制序列。

适配：

- 为 `HistoryLine` 增加“未换行的渲染纯文本行”接口，供几何构建。
- `SelectablePanel` 增加 `set_styled_content(&[Line])`，内部同时保存 styled rows 与 plain projection。
- 明确复制策略：视觉选区复制渲染纯文本，`/copy` 复制源文本。

参考：`paperboy-tui/crates/tui-panel-select/src/multiselect.rs:147`、`:162`。

### 6.7 滚动所有权与选区解耦

机制：

- 对照对象由面板自持滚动，`autoscroll_tick` 同时推进滚动与选区光标。

适配：

- `wf-agent` 的滚动还服务 tail-follow、SSE 追加、分页器与 gutter 拖拽，建议保留 `InteractiveController` 的滚动所有权。
- 自动滚动 tick 只推进 `view_scroll`，下一帧重新把逻辑光标映射到新视口，锚点保持逻辑位置不变。

这样既得到稳定的选区语义，又不破坏现有流式行为。

### 6.8 剪贴板决策链

机制：

- 本地工具优先，OSC 52 最后。
- X11 工具优先于 Wayland 工具，避免部分合成器弹出可见选区窗口。
- tmux pass-through。
- 模式固定与测试禁用。
- 绘制完成后再刷出转义字节，避免与差分帧交错。

适配：

- 现有实现已经覆盖，保留。
- `CopyPolicy` 的能力探测与拒绝提示可反向贡献给对照对象。

参考：`crates/app/tui/tui-terminal/src/clipboard.rs`、`crates/app/tui/tui-core/src/copy.rs`。

### 6.9 终端守卫

机制：

- 以 RAII 守卫管理鼠标捕获等全局终端状态。
- panic hook 中恢复，正常退出时 `Drop` 恢复。

适配：

- 复用现有 `TerminalGuard` 与 `TerminalModes`，接线时确保 mouse capture 与模式状态同步。

参考：`crates/app/tui/tui-terminal/src/terminal/guard.rs`、`paperboy-tui/crates/tui-panel-select/src/terminal.rs:56`。

### 6.10 测试策略

对照对象的几何单测可以直接扩展为壳层回归：

- 同一逻辑范围在窄宽与宽宽重排后提取结果一致。
- 宽字符不被劈开。
- 纯空白选区返回 `None`。
- 超长单行只高亮可见窗口。
- 逆向拖拽与正向拖拽结果一致。
- 续行标记列不映射为字符。
- 剪贴板测试强制 `ClipboardMode::None`，避免覆盖开发机剪贴板。

## 七、落地方案

### 7.1 crate 落点

| 职责 | 落点 | 说明 |
|---|---|---|
| 逻辑位置与几何 | `tui-core` | 已存在，继续扩展 Motion 与端点排序辅助 |
| 换行缓存与会话选区状态 | `tui-render` | 复用 `PanelWrapCache`，新增 `SelectablePanel` |
| 键盘 Motion 纯逻辑 | `tui-core` | 不依赖渲染，便于单测 |
| 剪贴板与终端模式 | `tui-terminal` | 已存在，不新增后端 |
| 复制策略 | `tui-core` | 已存在，保留 |
| 事件路由与主题 | `wf-tui` | 把显示坐标换成逻辑位置 |
| 文件查看器接线 | `wf-tui` 或 `tui-components` | 视是否允许 `tui-components` 依赖 `tui-render` 决定 |

依赖约束：`tui-components` 当前依赖 `tui-core`、`tui-style`、`tui-markdown`、`tui-clock`，不依赖 `tui-render`；`tui-render` 依赖 `tui-components`。因此 `SelectablePanel` 放在 `tui-render` 最直接。若希望 `FileViewer` 自持选区，需要把 `PanelWrapCache` 下沉到 `tui-core`，或让 `wf-tui` 在外部持有选区状态并驱动 `FileViewer` 绘制。

### 7.2 建议 API

```rust
pub struct SelectablePanel {
    wrap: Option<PanelWrapCache>,
    selection: Option<(TextPos, TextPos)>,
    exclude: HashSet<TextPos>,
    mode: PanelWrapMode,
    marker: bool,
}

impl SelectablePanel {
    pub fn set_plain(&mut self, text: Arc<str>, width: usize);
    pub fn set_styled(&mut self, lines: &[Line<'static>], width: usize);
    pub fn clear(&mut self);
    pub fn begin(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool;
    pub fn extend(&mut self, point: (u16, u16), area: Rect, scroll: u16) -> bool;
    pub fn extend_motion(&mut self, motion: Motion, area: Rect, scroll: u16);
    pub fn selected_text(&self) -> Option<String>;
    pub fn highlight_cells(&self, area: Rect, scroll: u16) -> Vec<(u16, u16, u16)>;
    pub fn visible_rows(&self, scroll: u16, height: u16) -> Vec<String>;
}
```

`Motion` 建议定义在 `tui-core`：

```rust
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    PageUp(u16),
    PageDown(u16),
}
```

### 7.3 事件流

```text
鼠标按下
  -> 钳制到面板矩形
  -> PreparedScrollback / PanelWrapCache 映射为 TextPos
  -> selection = Some((pos, pos))

拖拽
  -> 更新 cursor TextPos
  -> 越界时只推进 view_scroll
  -> 下一帧按新 scroll 重新映射光标

松开
  -> selected_text()
  -> sanitize_copy_text()
  -> 本地剪贴板优先，OSC 52 回退

键盘
  -> Motion 修改 cursor TextPos
  -> 必要时滚动到可见
```

### 7.4 Markdown 与 ANSI 复制策略

建议明确两条复制路径：

- 视觉选区：复制渲染后的纯文本。用户看到什么就复制什么，适合 Markdown、ANSI、语法高亮。
- `/copy` 命令：保留源文本。适合需要原始 Markdown 或命令输出的场景。

两条路径共用同一套剪贴板后端与策略门禁，只在文本来源上区分。

## 八、实施顺序与验收

### 8.1 阶段划分

| 阶段 | 内容 | 验收 |
|---|---|---|
| P0-1 | 命中与复制复用 `PreparedScrollback` 的行索引，删除全量 `display_lines` 重建 | 长会话点击与复制开销与可见窗口相关 |
| P0-2 | `TranscriptSelection` 改为 `TextPos`，滚动、resize 后选区保持 | 滚动中选区不丢，缩放后同一字符仍被选中 |
| P0-3 | 键盘选择：Shift+方向、Shift+Home/End、Shift+PgUp/PgDn、Ctrl+A、Esc | 关闭鼠标捕获时仍可复制部分输出 |
| P1-1 | `SelectablePanel` 封装并接入文件查看器、diff 查看器、历史分页器 | 查看器内可拖选复制 |
| P1-2 | styled / ANSI 纯文本投影 | Markdown、ANSI 选区不错位 |
| P2-1 | 多区域选择与 Alt+Click | 可一次复制多个不相邻片段 |
| P2-2 | 续行标记 | 软换行与硬换行可区分 |
| P2-3 | 文档同步 | `docs/cli/04` 与 `docs/research/tui/01` 更新 |

### 8.2 测试清单

几何与状态：

- 同一逻辑范围在 40 列与 8 列下提取结果一致。
- 宽字符与组合字符不被劈开。
- 纯空白选区返回 `None`。
- 超长单行只产生视口内高亮格。
- 逆向拖拽与正向拖拽结果一致。
- 续行标记列不产生可复制字符。

壳层：

- 滚动后选区仍指向原字符。
- resize 后选区仍指向原字符。
- 拖出边界自动滚动时锚点不漂移。
- 流式追加期间不会把未完成行纳入选区。
- 关闭鼠标捕获时键盘选择可用。
- 文件查看器选区复制内容与渲染一致。

剪贴板：

- 本地工具成功时不写 OSC 52。
- 本地工具失败时回退 OSC 52。
- 非 TTY 时按 `CopyPolicy` 拒绝并给出提示。
- 测试环境强制 `ClipboardMode::None`。

验证命令：

```bash
cargo check -p tui-core -p tui-render -p tui-terminal -p tui-components -p wf-tui
cargo test -p tui-core -p tui-render -p tui-terminal -p wf-tui
```

## 九、风险与取舍

- 直接替换 transcript 选区坐标会影响现有生成号校验与流式尾部处理。建议保留 `seq_no` 与 generation 校验，只把坐标语义换掉。
- Markdown 渲染会改变字符与行结构。几何必须建立在渲染后的纯文本投影上，避免用 `source_text` 直接映射。
- 把 `PanelWrapCache` 下沉到 `tui-core` 会改变分层，收益是 `tui-components` 可以自持选区。若短期只有 `wf-tui` 需要，优先把 `SelectablePanel` 放在 `tui-render`，减少改动面。
- 面板自持滚动会与 tail-follow、SSE 追加、分页器冲突。保留宿主滚动所有权，选区只存逻辑位置。
- 多区域选择会增加复制顺序、跨面板组合、视觉层叠的复杂度，放到 P2。
- 续行标记会占用一列，可能影响现有快照与窄终端布局，默认关闭。

## 十、与既有方案的关系

`docs/plan/tui-panel-select-design.md` 已经完成对照分析并给出落点设计。本文件在其基础上补充：

- 落地进度核对：原语层、剪贴板层、演示层已完成；壳层接线未完成。
- 缺口优先级：把“逻辑坐标迁移”和“键盘选择”提升为 P0，把“多区域”和“续行标记”后置。
- 分层修正建议：明确 `SelectablePanel` 的落点与 `tui-components` 不依赖 `tui-render` 的约束。
- 复制策略：区分视觉选区与 `/copy` 的文本来源。
- 测试与验收：补充壳层滚动、resize、自动滚动、键盘路径的用例。

后续实施应以本文件的 P0 清单作为起点，完成后回填 `docs/plan/tui-panel-select-design.md` 的落地状态，并同步 `docs/cli/04-终端交互设计.md`。

## 附录 A：关键文件索引

wf-agent：

- `crates/app/tui/tui-core/src/panel_select.rs`
- `crates/app/tui/tui-core/src/wrap_map.rs`
- `crates/app/tui/tui-core/src/copy.rs`
- `crates/app/tui/tui-core/src/perf.rs`
- `crates/app/tui/tui-render/src/panel_wrap.rs`
- `crates/app/tui/tui-render/src/prep_cache/scrollback.rs`
- `crates/app/tui/tui-render/src/prep_cache/aux_map.rs`
- `crates/app/tui/tui-render/src/ansi.rs`
- `crates/app/tui/tui-terminal/src/clipboard.rs`
- `crates/app/tui/tui-terminal/src/terminal/guard.rs`
- `crates/app/tui/tui-components/src/scrollbar.rs`
- `crates/app/tui/tui-components/src/file_viewer.rs`
- `crates/app/tui/tui-components/src/transcript.rs`
- `crates/app/cli/wf-tui/src/interactive.rs`
- `crates/app/cli/wf-tui/src/interactive/mouse_hit.rs`
- `crates/app/cli/wf-tui/src/interactive/render.rs`
- `crates/app/cli/wf-tui/src/tui/mouse.rs`
- `crates/app/cli/wf-tui/src/tui/clipboard.rs`
- `crates/app/cli/wf-cli-demo/examples/tui/panel_select_showcase.rs`
- `docs/plan/tui-panel-select-design.md`
- `docs/cli/04-终端交互设计.md`

paperboy-tui：

- `crates/tui-panel-select/src/lib.rs`
- `crates/tui-panel-select/src/panel.rs`
- `crates/tui-panel-select/src/multiselect.rs`
- `crates/tui-panel-select/src/selection.rs`
- `crates/tui-panel-select/src/wrapcache.rs`
- `crates/tui-panel-select/src/clipboard.rs`
- `crates/tui-panel-select/src/terminal.rs`
- `crates/tui-panel-select/src/scrollbar.rs`

## 附录 B：决策记录

| 决策 | 结论 | 理由 |
|---|---|---|
| 是否引入 `tui-rgb-picker` | 不引入 | 当前 TUI 没有取色需求 |
| 是否新建 crate | 不新建 | 现有 `tui-core` / `tui-render` / `tui-terminal` 分层已足够 |
| 是否让面板自持滚动 | 否 | tail-follow、SSE 追加、分页器需要宿主控制 |
| 是否保留 OSC 52 | 保留 | SSH、tmux、无本地工具环境仍需要 |
| 是否保留 `CopyPolicy` | 保留 | 能力探测与拒绝提示优于对照对象 |
| 是否默认开启鼠标捕获 | 保持默认关闭 | 保留终端原生选择，符合现有输入能力策略 |
| 键盘选择优先级 | P0 | 鼠标默认关闭时这是唯一的部分复制路径 |
| 多区域选择优先级 | P2 | 单区域已覆盖主流程，复杂度较高 |
