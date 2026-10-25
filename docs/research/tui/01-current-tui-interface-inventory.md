# 当前 TUI 界面与视觉系统清单

## 1. 整体壳层

- Shell/状态：`crates/app/cli/wf-tui/src/screens.rs`、`crates/app/cli/wf-tui/src/state.rs`
- Navigation 默认从 `ScreenKind::Interactive` 启动；management screen 通过 sidebar overlay / search palette / digit 0-6 进入。
- Render model：`crates/app/tui/tui-core/src/render_model/*`、`crates/app/tui/tui-render/src/screen_draw.rs`
- 所有非 Interactive screen 都从 `ScreenData` 渲染，详见 `crates/app/tui/tui-core/src/screen_data.rs`
- 管理 screen 共享视觉语言：`titled_block` 边框、`selected_style` 黑底青色/强调色高亮、status role 颜色、scrollbar；已升级为 list + detail master-detail（`split_list_detail`, 50/50）。

视觉特点：
- 界面以 bordered box 为单位；标题写在边框内，部分 title 包含当前操作提示。
- 列表项为一行文本，字段用 `·` 分隔。
- 状态用 ANSI color role：Add/AddText=绿、Error=红、Warning=黄、Highlight=强调、Muted=灰；`status_role()` 覆盖 completed/running/paused/modified/deleted 等标签。
- 选中行使用强调色背景，非选中按 status 上色。
- 大多数 screen 提供 `list_suffix`（数量、server 截断、active filter）。

## 2. Dashboard

位置：
- `screen_data.rs::DashboardData`
- `screen_draw.rs::draw_dashboard`

界面内容：
- 工作流/执行（running）/检查点计数。
- 全部 12 个 screen 列表，`>` 标记选中；digit 对前七个显示 0-6，其余显示 `-`。
- 最近执行最多 5 条。
- selected anchor 滚动：`draw_dashboard` 按 selected 计算窗口，避免短终端裁掉选中项。

可视化实现细节：
- `ScreenKind::all().len()` 行可选，Dashboard row_count 等于 screen 数。
- 文本 Paragraph + block，没有 card/图标。
- Enter 进入 screen；digit 快速切换前七个。

## 3. Workflow

位置：`draw_workflow`, `WorkflowRow`, `WorkflowView`

界面：
- 顶部 Views tab：Summary / Nodes / Runs（`tab_bar::format_tabs` 文本 tab，不是 graphical tab）。
- header 提示：`[ ] views`、`/ find`、`X clear`、`d delete`、`Enter detail`、`Esc back`。
- split management：header area + 50/50 list/detail。
- Summary view 从 cached row 渲染 id/name/description/nodes property list；Nodes/Runs 渲染 fetched detail lines。
- list client-side `/` filter，`X` clear。

## 4. Executions

位置：`draw_executions`, `ExecRow`, `ExecView`

界面：
- status filter（`f`：All/Running/Paused/Completed/Failed/Cancelled）。
- 执行列表带 status coloring。
- detail pane：Overview / Messages / Tools / Timeline / Variables。
- header 提示：`f status`、`/ find`、`X clear`、`Enter open session`、views cycle `[ ]`。
- `p` 可 toggle selected execution（key route）；Overview 的 Enter 回到 Interactive replay。

## 5. Insights

位置：`draw_insights`, `InsightsData`, `InsightTable`

界面：
- 多个预聚合 table，每个 `InsightTable` 有 title 和 rows；header row 为第一行。
- 通过 `tui_components::data_table::format_table` 渲染对齐文本，不是手写 SQL 布局。
- 无边框内单元格；块整体 titled block。

## 6. Interactive / Chat

位置：
- `crates/app/cli/wf-tui/src/interactive.rs`
- `crates/app/cli/wf-tui/src/interactive/render.rs`
- `crates/app/cli/wf-tui/src/interactive/anchors.rs`
- `crates/app/tui/tui-components/src/footer.rs`, `composer.rs`, `scrollbar.rs`, `transcript.rs`

界面结构：
- scrollback 历史区域 + preparation cache（`prep_cache`）。
- footer（状态行、route panels、key hints）；composer 不在 footer 框内，footer 主区只承载面板与阻塞视图。
- 底部 composer input 行（`> ` 前缀列，文本、placeholder、mention 高亮、ghost 后缀同面渲染）；composer 过高时 windowed。
- streaming tail line（2 column margin）+ spinner（Braille 10 frames / 40ms）。
- scrollbar（estimated/accurate two-pass width）。
- jump badge 与 anchors（prompt/todo/failed/subagent/checkpoint/decision），Ctrl shortcuts。
- 空态 `draw_welcome`：居中 `wf-agent` title、`Modular agent framework` subtitle 和输入/退出提示。

Footer/bottom-pane 能力：
- `FooterView`: Prompt / Permission / Question。
- `FooterRoute`: Composer / Command / Model / Skill / Queued / Workflow / Mention。
- 输入归属：Composer 与 Mention 拥有 composer 输入（Mention 是组合态，面板是 composer 查询的派生视图）；其余面板独占 filter，composer 暂停。
- 面板：`CommandPalette`, `ModelPanel`, `SkillPanel`, `QueuedPanel`, `WorkflowPanel`, `MentionPanel`；面板均由 `wf-tui` 的 `interactive/panels.rs` 接入事件循环。
- `FooterState` 路由/状态；notice TTL 3s，宽度断点 80/120。
- approval/question 嵌入 footer Permission/Question 视图，不是居中 overlay。

Redraw：
- Redraw grading：Full / BottomOnly / AnimationOnly / None。
- Full frame refresh preparation cache key；Bottom/Animation 复用 cached preparation。
- 预估 text width + scrollbar 两步修正，减少闪烁。

历史/上下文：
- 使用 `transcript.rs::HistoryLine` 与 role color，而非 HistoryCell trait。
- `interactive/anchors.rs` 维护 jump anchor table，trimmed rows drop bookkeeping on next jump。

## 7. Checkpoints

位置：`draw_checkpoints`

界面：
- status-colored row list：id/entity/entity type/checkpoint type/status/timestamp。
- master-detail：左侧 list，右侧 property-list detail。
- title 提示：`r refresh`、`R restore`、`Enter detail`、`t chain`、`d delete`、`Esc back`。
- detail modal 入口：checkpoint detail、chain viewer；restore/delete 用 ConfirmModal。

## 8. Settings

位置：`draw_settings`

界面：
- 工作 Theme：`Theme: ...`。
- LLM profiles，用 `*` 标默认；`m` 通过 ModelPicker 更换 default。
- local config 文本/ toggle rows：
  - `[o] mouse capture`
  - `[s] session scrollbar`（即时）
  - `[c] copy_target`
  - `[e] tool output` + `[l] level`
  - `[+/-] preview lines · chars`
  - `appearance theme` 与 `motion performance`
  - `notifications`: on/off/auto
- scrollbar。
- title：`j/k scroll, o/c/e/l/s local keys, +/- preview, m model, Esc back`。
- 本地变更保存 config 文件，重启生效（session scrollbar 即时）。

不是传统文本行； toggle 通过 `property_list::format_toggle` 渲染。

## 9. Templates / Resources / Triggers / Events / Git

位置：`draw_templates`, `draw_resources`, `draw_triggers`, `draw_events`, `draw_git`

界面：
- 统一 list + detail（`split_list_detail`）。
- 行式列表：Templates `name · kind · id`；Resources `name · status · id`；Triggers `name · status · id`；Events `kind · summary · id`；Git `status · path`。
- Templates detail 展示预渲染 `detail`（`Enter` FileViewer）。
- Resources/Triggers/Events/Git detail 默认 property list 或 detail 文本。
- 标题包含 `r refresh`、`Enter detail`、`Esc back`；Git status title 显示 `r refresh, Enter detail, Esc back`。
- Git 这里是 status list + detail，没有 branch tree / commit graph / hunk diff；Interactive 中 tool output 可使用 `DiffViewer::from_unified`。
- 设计决策：Git 保持只读状态摘要 + detail，外部 `git`/`lazygit` 承担 diff/staging/branch/history 等完整能力；不在未来接入 Git diff viewer。
## 10. 覆盖层与 modal

- Search palette：`draw_search_palette`，`modal::centered_rect(70, 60)`；input + result list；score 显示；truncated/running/error 状态。
- Sidebar：`draw_sidebar_overlay`，左侧 30% screen list + `Clear`；导航 overlay，不是上下文 sidebar。
- History overlay：`draw_transcript_overlay`，全屏 transcript，tail anchored，reflow + scrollbar + jump badge + loading/has_more/error status；拖选复制与 `Shift` 键盘扩展选择接入 `SelectablePanel`。
- Help modal / approval / question / model picker：`help_modal.rs`, `approval_overlay.rs`, `question_overlay.rs`, `model_picker.rs`。
- Confirm/password/prompt modal：`confirm_modal.rs`, `password_modal.rs`, `prompt_modal.rs`。
- FileViewer/DiffViewer：`file_viewer.rs`，支持 scroll、copy（Ctrl+Y）；DiffViewer 有 Add/Remove/Context/Hunk coloring；拖选复制、`Alt+Click` 多区域、`Esc` 先清选区，均基于 `SelectablePanel` 的逻辑坐标。
- Checkpoint chain/detail 通过 `FileViewer` 展示 text。

## 11. 全局视觉系统

- Theme 解析/缓存：`crates/app/tui/tui-style/src/theme/*`
- 颜色角色与主题匹配；`osc.rs` OSC 探测终端背景。
- 终端能力：`tui-terminal`（modes, probe, sigint, stderr guard, image emit）。
- scrollbar：`tui-components/src/scrollbar.rs`（visible/clamp/split/render）。
- Markdown 与流式渲染：`tui-markdown/*` canonical dialect，stream pacer/tail。
- 高亮/动画：`tui-style/src/motion.rs`, `animation.rs`, `anim_core.rs`。
- History line role color mapping -> `ColorRole`.

## 12. wf-mini

位置：`crates/app/cli/wf-mini/`

轻量 CLI，不复用 full screen Draw 家族，强调单会话/transcript；视觉目标低 footprint，不是管理控制台。共享 `tui-style`/theme/component 子集可作为一致性边界；`tui-markdown` canonical dialect 与 full TUI 共用。

## 13. 设计决策与延期项

- Git screen 保持只读 status summary + detail，代号 read-only；`DiffViewer` 仅 Interactive tool output 使用。
- `HistoryLine`、Unified Catalog、Context Sidebar、Settings dialog family 仍按研究建议延期，直到出现 copy/search/raw、catalog 聚合或真实上下文数据的强需求。
- `tui-components` 中未接线的 `bottom_pane`、`context_sidebar`、`file_tree`、`file_selection` 等组件不视为 Full TUI 已交付能力，仅作为可复用库资产。
