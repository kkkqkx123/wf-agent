# 当前 Screen 与参考项目视角对比

## A. Dashboard

当前：
- `screen_draw.rs::draw_dashboard` 渲染计数 + 全部 screen 列表 + 最近执行。
- `Screens::new()` 默认启动 Interactive，Dashboard 是工作区 overview / screen catalog，不是品牌首页。
- 视觉密度中等；`>` 选中 marker；无图标/卡片；最近执行最多 5 条；digit 0-6 只对应前七个 screen。
- Interactive 空态 `draw_welcome` 单独提供品牌 centered welcome，弥补 Dashboard 作为功能目录的弱引导。

可对应参考：
- opencode `routes/home.tsx`：Home 以 logo + prompt 为中心，screen navigation 更少。
- atomcode welcome/mascot/tips：品牌态与 onboarding。
- jcode session picker：将导航放在 picker/loader，而非 dashboard。
- codex chat 空态：`Type a prompt and press Enter...` 风格。

差距：
- 当前 Dashboard 是功能目录，不是品牌/任务起始面；但空态欢迎已进入 Interactive。
- 参考项目更强调“立即开始会话”，当前通过 dig 4 进入 Chat，且默认就是 Interactive。

建议方向：
- Dashboard 可转为“workspace overview + quick actions”，保持 screen catalog 作为导航 overlay。
- Interactive welcome 可继续吸收 atomcode/opencode 的 tips/footer。

## B. Workflow

当前：
- master-detail；views 为文本 tab；字段文本；detail pane。
- list + 50/50 detail，`/` filter，`X` clear，`d` delete，views `[ ]` cycle。
- Summary 从 cached row property list；Nodes/Runs 显示 fetched detail lines。

可对应参考：
- opencode dialog list；jcode session picker list/filter；codex selection view。

差距：
- Workflow view 仍以文本 property/detail 为主； lacks status visualization、node graph preview、run timeline。
- Workflow detail drawer 可借鉴 opencode diff-viewer tree drilldown。

建议：
- 列表可借鉴 jcode session picker 的 filter + detail side panel（当前已部分具备）。
- detail 可扩展 node graph/run timeline；不要把 50/50 文本分栏当成最终主视图。

## C. Executions

当前：
- execution rows + status color；subviews Overview/Messages/Tools/Timeline/Variables。
- `f` status filter；`p` toggle；Overview Enter 回到 Interactive replay。
- detail cache，`Enter` 对非 Overview 打开 FileViewer 全文。

可对应参考：
- codex transcript/history cell；opencode session timeline dialog；jcode session facts。

差距：
- execution cell 语义仍弱；current cell is text row，不是 display/raw/copy 的 HistoryCell。
- Timeline/Variables 应对应 codex execution cell 与 opencode timeline/context。

建议：
- 与 codex `history_cell/exec.rs` 对齐：每个 execution 为 cell，包含 command、output、status。
- Timeline 对齐 opencode `dialog-timeline.tsx`；Variables 对齐 context sidebar。

## D. Insights

当前：
- `data_table::format_table` 渲染多 table 对齐文本，header row 为第一行。
- 无边框内单元格； densities 高。
- Screen data 为 `InsightsData.tables`.

可对应参考：
- codex table render/markdown_render/table_key_value；opencode feature plugins。

建议：
- 对齐 codex markdown table renderer，支持 key/value、header、border。
- 图表/usage 可借鉴 jcode usage overlay。

## E. Interactive / Chat

当前：
- scrollback + footer + mirrored prompt input + spinner + streaming tail + scrollbar + jump badge。
- 空态 centered welcome；footer 是 inline bottom pane，含 command/model/skill/workflow/mention/queued panels。
- approval/question 嵌入 footer Permission/Question 视图，不 disrupting scrollback。
- redraw grading：Full / BottomOnly / AnimationOnly / None。
- anchors：prompt/todo/failed/subagent/checkpoint/decision，Ctrl shortcuts。
- retained preparation cache + HistoryLine role color。

可对应参考：
- codex `chatwidget.rs`, `bottom_pane/*`：history cell + bottom panel + approval overlay。
- opencode session route + sidebar + prompt。
- atomcode retained screen + modal。
- jcode split view + todos view。

差距：
- `HistoryLine` 是通用行；参考将语义 cell 作为一等单元（display/raw/copy）。
- footer 已接近 codex bottom pane，但 panel family 还需 document/persist consistency。
- 侧栏上下文仍缺 opencode/jcode 风格 context panel。

建议：
- HarvestCell model: message/tool/plan/approval/patch，带 raw/display/copy。
- composer/approval/question 状态堆栈继续对齐 codex bottom pane。
- context sidebar（files/todo/context/mcp）对齐 opencode sidebar，或 rename current navigation sidebar.

## F. Checkpoints

当前：
- 不再是全屏 status list：list + detail 50/50。
- title 说明 refresh/restore/detail/chain/delete。
- detail/chain FileViewer；restore/delete ConfirmModal。
- 时间线/chain 可查看。

可对应参考：
- jcode session picker、atomcode rewind modal。

建议：
- Checkpoint 可作为 panel/overlay，而非主 screen；chain view 可 dates/timeline 更可视化。
- 交互（restore new commit）可与 jcode rewind flow 对齐。

## G. Settings

当前：
- Theme、LLM profiles、local config toggle rows。
- 鼠标/scrollbar/tool output/preview/appearance theme/motion/notification 等都有控制。
- `m` 打开 ModelPicker；local config save-on-restart（scrollbar instant）。

可对应参考：
- atomcode config panel/modal
- codex thread_settings/settings popups/theme_picker
- opencode dialogs theme/model/provider/status

建议：
- UI 形态对齐 opencode dialog family（分 tab dialog）。
- toggle/motion/appearance 对齐 codex theme_picker。
- data/profile 对齐 atomcode/codex settings popups。

## H. Templates / Resources / Triggers / Events

当前：
- list + detail master-detail；`/` filter（X clear）；`r` refresh；Enter detail FileViewer。
- Templates detail 预渲染；其他 detail 默认 property/detail。
- Git 也遵循相同模式。

可对应参考：
- opencode command palette / dialog list
- codex selection view
- jcode session picker filter

建议：
- 合并为统一 searchable resource catalog，对齐 opencode command palette 输入+结果模式。
- per-item detail drawer 对齐 atomcode modals。
- 可考虑把 `/` palette 打开为 command route（当前 footer 已有 Command route）。

## I. Git

当前：
- status list + detail 50/50；status coloring。
- `r refresh`、`Enter detail`；detail FileViewer。
- 没有 diff viewer、branch tree、commit graph。
- Interactive tool output 可以使用 DiffViewer `from_unified`。

可对应参考：
- atomcode git diff/diff viewer
- opencode diff-viewer feature plugin
- codex diff_render
- jcode info_widget_git

设计决策：
- Git screen 保持只读 status summary + detail；不实现 file tree、hunk viewer 或交互式 git 操作，外部 `git`/`lazygit` 承担完整 Git 工作流。
- `DiffViewer` 仅用于 Interactive tool output 的 unified diff，不接入 Git screen。
- status list 保持 Navigation/sidebar/search palette 中的低频访问入口，不分配 digit shortcut。

## J. History Overlay

当前：
- full screen reflow transcript，tail anchored。
- `HistoryOverlayView` reflow + role color + scrollbar + jump badge + status (`loading…`/error/has_more/PageUp)。
- `pager.rs` 分页。
- Ctrl+T 关闭，PageUp/PageDown/End。

可对应参考：
- codex history UI / transcript
- opencode timeline dialog
- jcode session picker/render

建议：
- History should be pager/virtualized with roles preserved；对齐 codex HistoryCell raw_lines。
- 避免每帧 reflow，借鉴 retained/virtualized rendering。
