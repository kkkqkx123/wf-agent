# 当前 TUI 界面与视觉系统清单

## 1. 整体壳层

- Shell/状态：`crates/app/cli/wf-tui/src/screens.rs`、`crates/app/cli/wf-tui/src/state.rs`
- Render model：`crates/app/tui/tui-core/src/render_model/*`、`crates/app/tui/tui-render/src/screen_draw.rs`
- 每 screen 都从 `ScreenData` 渲染，详见 `crates/app/tui/tui-core/src/screen_data.rs`
- 管理 screen 使用同一套视觉语言：`titled_block` 边框、`selected_style` 黑底青色高亮、status role 颜色、scrollbar。

视觉特点：
- 界面以 bordered box 为单位；标题写在边框内。
- 列表项为一行文本，字段用 `·` 分隔。
- 状态用 ANSI color role：Add=绿、Error=红、Warning=黄、Highlight=强调、Muted=灰。
- 选中行使用强调色背景，非选中按 status 上色。
- 一些 screen 提供 filter/find suffix，例如 `list_suffix`。

## 2. Dashboard

位置：
- `crates/app/tui/tui-core/src/screen_data.rs::DashboardData`
- `crates/app/tui/tui-render/src/screen_draw.rs::draw_dashboard`

界面内容：
- 工作流/执行/检查点计数。
- 全部 screen 列表，`>` 标记选中，digit 0-6。
- 最近执行列表。

视觉特点：
- 文本块 Paragraph，没有独立卡片。
- 选中 screen 锚点滚动，候选列表跳跃。
- 按 Enter 进入 screen，digit 快速切换。

## 3. Workflow

位置：
- `screen_draw.rs::draw_workflow`
- `screen_data.rs::WorkflowRow`, `WorkflowView`

界面内容：
- 顶部 Views tab：Summary / Nodes / Runs。
- filter/find/clear/delete 入口。
- 上下分栏：工作流列表 + detail pane。
- detail 在 Summary 模式展示 id/name/description/nodes；Nodes/Runs 展示 fetched detail lines。

视觉特点：管理屏典型 master-detail；tab 用 `[label]` 文本，不是 graphical tab。

## 4. Executions

位置：
- `draw_executions`, `ExecRow`, `ExecView`

界面：status filter、执行列表、detail view：Overview / Messages / Tools / Timeline / Variables。
视觉特点与 Workflow 同源；status coloring 让执行状态更直观。

## 5. Insights

位置：`draw_insights`

界面：SQL-like table 手工对齐文本。无边框内单元格，自适应列宽。密度高。

## 6. Interactive / Chat

位置：
- `crates/app/cli/wf-tui/src/interactive.rs`
- `crates/app/cli/wf-tui/src/interactive/render.rs`
- `crates/app/tui/tui-components/src/footer.rs`, `composer.rs`, `scrollbar.rs`, `transcript.rs`

界面结构：
- scrollback 历史区域
- footer（状态、composer、快捷提示）
- input/prompt 行
- streaming tail line + spinner
- scrollbar
- jump badge
- approval/question overlays

视觉特点：
- 全屏聊天场景；scrollback rows 用 history line。
- footer 可增长，composer 可 window。
- redraw 分级：Full / BottomOnly / AnimationOnly，提升效率。
- 有 jump anchors：prompt/todo/failure/sub-agent/checkpoint/decision。

## 7. Checkpoints

位置：`draw_checkpoints`

界面：status rows，显示 id/entity/type/status/timestamp。无 master-detail，占满面积。

## 8. Settings

位置：`draw_settings`

界面：
- Theme、LLM profiles、local config 开关。
- 文本条目；按键提示在 title。
- scrollbar。

视觉特点：传统表单/配置列表，字段使用文本 token，没有 toggle 控件。

## 9. Templates / Resources / Triggers / Events / Git

位置：`draw_templates`, `draw_resources`, `draw_triggers`, `draw_events`, `draw_git`

界面：
- 统一行式卡片列表。
- title 包含操作提示。
- status rows 辅助颜色。

Git 仅 status list，没有 diff viewer、branch tree、commit graph。

## 10. 覆盖层

- Search palette：centered modal 70x60；input 行 + result list；`crates/app/tui/tui-render/src/screen_draw.rs::draw_search_palette`。
- Sidebar：左侧 30% screen list；`draw_sidebar_overlay`。
- History overlay：full-screen transcript；`draw_transcript_overlay`。
- Help modal / approval / question / model picker：组件库中对应文件 `crates/app/tui/tui-components/src/*modal*.rs`, `approval_overlay.rs`, `question_overlay.rs`, `model_picker.rs`。
- Confirm/password/prompt modal：`confirm_modal.rs`, `password_modal.rs`, `prompt_modal.rs`.

## 11. 全局视觉系统

- Theme 解析/缓存：`crates/app/tui/tui-style/src/theme/*`
- 颜色角色与主题匹配。
- OSC 探测：`crates/app/tui/tui-style/src/theme/osc.rs`
- scrollbar：`crates/app/tui/tui-components/src/scrollbar.rs`
- Markdown：`crates/app/tui/tui-markdown/*`（canonical dialect）

## 12. wf-mini

位置：`crates/app/cli/wf-mini/`

轻量 CLI，不复用 full screen Draw 家族，强调单会话/记录/transcript，组件更少。其视觉目标是低 footprint，不是管理控制台。本研究以 Full TUI 为主，mini 在 plan 中只作为一致性边界。