# TUI 界面与视觉效果对比研究

本目录汇总 `wf-agent` 全屏 TUI 与 `ref/` 下参考项目的界面、交互视角对比结果。研究范围以 `crates/app/cli/wf-tui`（Full TUI）为主；`wf-mini` 的轻量终端交互单独在极简模式章节说明。

研究对象：`atomcode`、`opencode`、`codex`、`jcode`。

## 目录

- `01-current-tui-interface-inventory.md`：当前项目 TUI 界面、覆盖层、视觉系统与代码位置。
- `02-reference-visual-effects-and-files.md`：参考项目 TUI 的视觉体系与关键文件索引。
- `03-screen-perspective-comparison.md`：当前各个 screen 与参考项目的对应界面、视角差异。
- `04-transcripts-overlays-settings-comparison.md`：Interactive、 transcript/history overlay、settings、modal/search palette、git/checkpoint 等界面对比。

## 方法与约定

1. 文中“视角效果”指用户观察到的布局、密度、色彩、强调、動效、信息组织与操作反馈，不只指配色。
2. 参考项目条目均标注涉及文件名；为可追溯起见优先使用相对 `ref/<project>` 的路径。
3. 对比基于代码结构与组件实现，而非运行截图；运行时差异需通过 snapshot/visual capture 另行确认。
4. 当前项目与参考项目的功能颗粒度不同：当前 Full TUI 是“管理控制台型”TUI，参考项目普遍是“会话中心型”TUI。因此对齐关系不是一对一，而是 screen-to-overlay 或 feature-to-panel。

## 当前 TUI 界面总览

当前 Full TUI 共有 12 个 screen：Dashboard、Workflow、Executions、Insights、Interactive (Chat)、Checkpoints、Settings、Templates、Resources、Triggers、Events、Git。`Screens::new()` 默认从 Interactive screen 启动；Dashboard 只是导航与计数 overview，digit 0-6 对应前七个 screen，其余 screen 通过 Dashboard、sidebar overlay 或 search palette 进入。所有 screen 的数据模型定义在 `crates/app/tui/tui-core/src/screen_data.rs`，绘制实现在 `crates/app/tui/tui-render/src/screen_draw.rs`，路由/状态位于 `crates/app/cli/wf-tui/src/screens.rs` 与 `crates/app/cli/wf-tui/src/state.rs`。

Interactive screen 不走 `ScreenData` 渲染，它由 `InteractiveController::draw`（`crates/app/cli/wf-tui/src/interactive/render.rs`）直接绘制，拥有自己的 scrollback、footer 与流式 tail。空态使用居中的 `draw_welcome`：标题 `wf-agent`、副标题 `Modular agent framework`，以及输入提示和 digit/退出提示。

Interactive screen 的 footer 是一个可增长的 inline bottom pane（`crates/app/tui/tui-components/src/footer.rs`）： composer、`/` 命令面板、Model/Skill/Workflow/Mention/Queued 面板、permission/question 视图、状态行、notice（3s）。空输入态时审批/问题视图嵌在 footer 中，而不是居中 modal。

交互层额外包含：footer + composer 路由面板，scrollback preparation cache，streaming tail + spinner，scrollbar，jump badge，search palette overlay，sidebar overlay，history transcript overlay，approval/question footer views，ModelPicker/FileViewer/DiffViewer modal，帮助 modal，checkpoint chain/detail viewer，空态 welcome。

覆盖层与 modal 入口速查：
- Search palette：Ctrl+K，`draw_search_palette`，`modal::centered_rect(70, 60)`。
- Sidebar：Ctrl+B，左侧约 30% 宽度 navigation overlay。
- History transcript：Ctrl+T/overlay，全屏 `draw_transcript_overlay`。
- Modal 家族：`confirm_modal.rs`、`password_modal.rs`、`prompt_modal.rs`、`help_modal.rs`、`model_picker.rs`、`file_viewer.rs`（FileViewer + DiffViewer）。
- Approval/question：`approval_overlay.rs` / `question_overlay.rs`，嵌入 footer Permission/Question 视图。

## 视角差异总览

| 视角 | 当前项目 | atomcode | opencode | codex | jcode |
|---|---|---|---|---|---|
| 总体结构 | Interactive 默认首屏 + 12 screen 管理控制台 | 会话中心 + modal/overlay | Home + Session route + sidebar | 聊天历史 + 底部面板 | 会话 + 侧栏 + workspace/map |
| 首屏 | 空态 centered welcome；Dashboard 计数 + screen catalog | 欢迎/logo + prompt | Home logo + prompt + footer slots | 聊天空态提示 | 会话/orientation 面板 |
| 色彩 | Theme role + 彩色 role | 终端主题 + 保留屏 cell diff | 主题 JSON + semantic color | 终端感知 + 明暗检测 + ANSI 调色板 | 主题 + 动画 |
| 密度 | 管理 screen 中高，list/detail 50/50；聊天流低密度 | 聊天流，低密度 | 信息密度高但组件化 | 历史 cell + 底部面板 | 侧栏上下文 |
| 动效 | spinner/shimmer 光标，redraw grading，jump badge | retained cell diff + 动画 | bg-pulse、spinner | shimmer、status indicator | anim crate |
| 弹出交互 | modal family、footer panel、sidebar | modal 系列 | dialog 系列 | overlay/selection view | usage/session picker |
| Git | 只读 status list + detail；Interactive tool output unified diff 可用 DiffViewer | git diff/diff viewer modal | feature-plugin diff viewer | patch/diff render | info_widget_git |
