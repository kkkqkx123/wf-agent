# TUI 界面参考对齐与改进设计

## 1. 设计原则

1. 当前 Full TUI 定位应从“管理控制台”拆分为“会话中心 + 管理 overlay”，避免每个功能占用一个 screen。
2. 保留 `ScreenKind` 的数据获取与 reducer 能力，但视觉主入口应以 Interactive 为中心。
3. 公共视觉 token 和组件应统一到 `tui-style` / `tui-components`；screen 不再直接硬编码颜色。
4. 不新增并行 TUI 模块；所有改进在现有模块内完成。
5. `wf-mini` 只复用 style/token，不复制 full screen 语义。

## 2. 目标信息架构

建议一级视图：
- Session Workspace（主视图，替代当前 Interactive screen + management screens）：
  - 中央 transcript
  - composer / bottom pane
  - context sidebar
  - optional detail drawer
- Navigation Palette：替代 sidebar overlay，负责 screen/feature 跳转。
- Management overlays：Workflow、Executions、Checkpoints、Templates、Resources、Triggers、Events、Git、Settings、Insights。
- History overlay：保留全屏 transcript。

过渡策略：
- 保留 digit 0-6 短期直接打开 overlay；
- 新的 Navigation Palette 成为主入口；
- management screen 逐步转为 overlay/drawer。

## 3. 与参考项目的对齐矩阵

| 当前界面 | 主要对齐项目 | 对应参考能力 | 调整 |
|---|---|---|---|
| Dashboard | opencode Home, atomcode welcome | Home prompt + tips | 将 dashboard 转为 welcome/overview，不再展示所有 screen 目录 |
| Workflow | jcode session picker, opencode dialog list | filter + detail drawer | 用 searchable picker + inspect drawer 替代 50/50 文本分栏 |
| Executions | codex HistoryCell, opencode timeline | execution cell, timeline dialog | 历史单元与 timeline 对齐 |
| Insights | codex markdown table | bordered table, key/value | 升级手工 table 文本渲染 |
| Interactive | codex chatwidget + bottom pane; opencode session; jcode split view | transcript + bottom pane + sidebar | 作为主 Session Workspace |
| Checkpoints | atomcode rewind; jcode session picker | rewind timeline modal | 作为 overlay/chain view |
| Templates/Resources/Triggers/Events | opencode command palette | unified catalog | 合并为可搜索 catalog |
| Git | opencode diff-viewer + atomcode diff viewer + codex diff_render | file tree + hunk | 升级 Git status 为 diff viewer |
| Settings | opencode dialogs + codex settings popups | dialog family | 从文本块转为 dialog + toggles |
| History overlay | codex history UI | cell-based history | 虚拟化 + raw/display |
| Search palette | opencode command palette | registry + preview | 加 action/context |
| Sidebar overlay | opencode context sidebar | context panel | 重命名 navigation palette；新增 context sidebar |

## 4. 各界面具体设计

### 4.1 Dashboard → Welcome / Overview

借鉴：
- opencode `routes/home.tsx` 的居中 logo + prompt。
- atomcode `render/mascot.rs`, `render/welcome_tips.rs`。
改进：
- Default route 进入 Interactive；Dashboard 改为 `/overview` 或 Welcome screen。
- 展示 counts、recent runs、quick actions（new session, open workflow, git status）。
- 保持 digit 兼容，但主入口在 prompt。

### 4.2 Workflow

借鉴：
- jcode session picker filtering；
- opencode dialog list + diff-viewer tree；
- atomcode modal family。
改进：
- 列表顶部 filter bar；Enter 打开 detail drawer；
- Summary 显示 metadata；Nodes/Runs 使用 structured rows；
- 右侧 detail 使用 bordered property list，而非拼接字符串。

### 4.3 Executions

借鉴：
- codex `history_cell/exec.rs`；
- opencode timeline dialog；
- jcode session facts。
改进：
- 每 execution 为 cell，显示 status badge、duration、tool count、last error。
- Timeline 为事件流，Variables 为 key/value panel。
- Messages 复用 transcript 渲染，不重复文本。

### 4.4 Insights

借鉴：
- codex `markdown_render/table_key_value.rs`。
改进：
- 使用 bordered table 渲染；
- 支持 header、alignment、empty cell；
- 为空状态展示引导。

### 4.5 Interactive / Session Workspace

借鉴：
- codex `chatwidget.rs` + `bottom_pane/*`；
- opencode session route + sidebar；
- jcode split view + todos view。
改进：
- HistoryCell 模型替换 `HistoryLine` 通用行；
- Bottom pane 堆栈：composer、approval、question、status line；
- sidebar context 面板显示 files/todo/context/mcp/git；
- composer 高度与 footer 已有逻辑可保留。

### 4.6 Checkpoints

借鉴：
- atomcode rewind modal；
- jcode session picker。
改进：
- 以 chain/timeline overlay 呈现；
- 支持 selected checkpoint 的 restore/delete/detail；
- 状态以 badge 渲染。

### 4.7 Templates / Resources / Triggers / Events

借鉴：
- opencode command palette；
- jcode session picker。
改进：
- 合并成 Unified Catalog overlay，按 kind 分 tab；
- filter 输入 + results + detail drawer；
- 减少 4 个并列 screen。

### 4.8 Git

借鉴：
- opencode diff-viewer file tree；
- atomcode diff viewer；
- codex diff render。
改进：
- 左侧 changed file tree，右侧 hunk viewer；
- status list 嵌入 sidebar/overview；
- 支持 staged/unstaged 分组。

### 4.9 Settings

借鉴：
- opencode theme/model/provider/status dialogs；
- codex settings popups/theme picker；
- atomcode config panel。
改进：
- 用 dialog family 替代文本设置块；
- theme/model/provider 分 dialog；
- 本地布尔配置使用 toggle + key hints；
- preview 参数可视化。

### 4.10 History overlay

借鉴：
- codex HistoryCell raw/display；
- opencode timeline；
- diff retained renderer。
改进：
- 增量 reflow 与 viewport virtualization；
- cell 级 copy；
- 保留 role color。

### 4.11 Search palette

借鉴 opencode command palette + atomcode modal family。
改进：
- command registry、contextual actions、shortcut preview；
- results 分组；async search indicator 更可视化。

### 4.12 Navigation 与 Context Sidebar

借鉴 opencode routes/session/sidebar。
改进：
- 将现有 sidebar overlay 重命名为 Navigation Palette；
- 新增 Context Sidebar，仅在 session workspace 显示；
- 避免 “sidebar” 二义性。

## 5. 颜色、动效与主题对齐

借鉴：
- codex `color.rs` / `style.rs` / `terminal_palette.rs` / `terminal_probe.rs`；
- atomcode retained diff + terminal_bg；
- jcode anim；
- opencode theme assets。

改进：
- 终端色彩探测从 env 升级为 OSC 10/11 + 缓存；
- 明/暗背景自适应 style；
- TrueColor/Ansi256/Ansi16 降级；
- shimmer/status indicator standardized；
- theme assets 支持导入外部主题（对齐 opencode）。

## 6. 组件库扩展建议

现有组件：`tui-components`。
新增/重构组件：
- `history_cell` (new component module, not separate crate unless needed)
- `data_table`
- `property_list`
- `file_tree`
- `diff_viewer`
- `toggle_row`
- `tab_bar`
- `command_palette`
- `context_sidebar`

## 7. 实施阶段（概要）

阶段 1：导航重构
- 默认进入 Interactive；Navigation Palette；Context Sidebar 骨架。
阶段 2：Management overlay 化
- Workflow/Executions/Templates/Resources/Triggers/Events/Git 先转 overlay，保留 screen data。
阶段 3：Transcript/HistoryCell
- HistoryLine → HistoryCell；raw/display；virtualized history。
阶段 4：Settings/Git/Diff/Tables
- dialog family、diff viewer、table rendering。
阶段 5：颜色/主题/动效
- terminal palette、adaptive style、shimmer、theme assets。

## 8. 验证

- Snapshot tests in `crates/app/cli/wf-tui/tests/`。
- Headless replay fixtures for TUI layout.
- ANSI diff records for render performance.
- 必须验证的 interface：
  - Session workspace default route；
  - Navigation palette；
  - Unified catalog；
  - Git diff viewer；
  - Settings dialog；
  - History virtualized mode.
## 9. 当前实现进度（代码）

状态标记：`[x]` 已完成基础实现；`[~]` 部分实现；`[ ]` 待开始。

### 已完成

- [x] Full TUI 默认进入 `Interactive`，管理界面以 screen/overlay 方式访问。
- [x] 共享组件：`data_table`、`property_list`、`tab_bar`，含单元测试。
- [x] 共享组件扩展：`history_cell`、`file_tree`、`toggle_row`、`command_palette`、`context_sidebar`，含单元测试。
- [x] Terminal palette 探测、明暗背景与 adaptive style 基础能力。
- [x] Git diff viewer 与 file tree 基础组件。
- [x] Insights 使用共享 aligned table 渲染。
- [x] Workflow 与 Executions 的 detail 摘要使用 `property_list`。
- [x] Git、Checkpoints、Templates、Resources、Triggers、Events 统一为 master-detail 两栏布局，并从选中行展示 detail。
- [x] 子视图 tab 文本统一经 `tab_bar::format_tabs` 生成。
- [x] Settings 布尔配置采用 checkbox 风格显示。
- [x] Navigation overlay 标题与术语对齐为 “Navigation”。

### 部分完成

- [~] Navigation / sidebar：术语与 overlay 已调整，context sidebar 基础组件已落地，完整 drawer 仍待落地。
- [~] Settings：文本块内 toggle 已改进，dialog 家族与 theme/model/provider 对齐仍待落地。
- [~] Git：master-detail 与 detail pane 已落地，file tree/diff viewer 基础组件已落地，完整 tree/hunk 仍待落地。

### 待开始

- [ ] Unified catalog（Templates/Resources/Triggers/Events 合并）。
- [ ] Context sidebar（files/todo/context/mcp/git）。
- [ ] HistoryCell 模型与 transcript virtualization。
- [ ] Terminal palette / 明暗背景 / adaptive style。
- [ ] Git diff viewer tree 与 hunk 渲染。
- [ ] dialog family 的 endpoint 与交互改造。

### 已执行验证

- `cargo test -p tui-components --lib`
- `cargo test -p tui-render`
- `cargo build -p wf-tui`
- `cargo build -p wf-cli-demo --example tui_screens`
- `cargo clippy -p tui-components -p tui-render --all-targets`
