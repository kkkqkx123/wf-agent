# 参考项目 TUI 视觉体系与关键文件

## atomcode

整体定位：会话中心 TUI，强调 retained-mode 渲染、modal 系列、动画/欢迎态与 diff 渲染。

关键文件：
- `ref/atomcode/crates/atomcode-tuix/src/render/screen.rs`：retained-mode cell grid，diff、scroll region、同步输出、cursor 管理。
- `ref/atomcode/crates/atomcode-tuix/src/render/retained.rs`： retained 渲染适配。
- `ref/atomcode/crates/atomcode-tuix/src/render/theme.rs`：主题/颜色 role。
- `ref/atomcode/crates/atomcode-tuix/src/render/mascot.rs`、`welcome_tips.rs`：欢迎态与品牌视觉。
- `ref/atomcode/crates/atomcode-tuix/src/render/diff.rs`：diff 可视化。
- `ref/atomcode/crates/atomcode-tuix/src/render/cell.rs`：单元格、宽字符处理。
- `ref/atomcode/crates/atomcode-tuix/src/modals/*.rs`：model/provider/session/config/plugin/dir/rewind/qr/diff viewer 等 modal。
- `ref/atomcode/crates/atomcode-tuix/src/highlight/theme.rs`：语法高亮主题。
- `ref/atomcode/crates/atomcode-tuix/src/terminal_bg.rs`：终端背景处理。

视觉特点：
- 首屏更品牌化，有 logo/mascot/welcome tips，而非控制台 dashboard。
- 交互通过 modal family，screen 数量少。
- 渲染强调 diff/retained buffer，减少闪烁。
- 代码高亮与 diff 是一等视觉能力。

## opencode

整体定位：SolidJS + OpenTUI 的组件化 TUI，Home route 与 Session route，sidebar 与 dialog 家族。

关键文件：
- `ref/opencode/packages/tui/src/routes/home.tsx`：Home 视觉：居中 logo、prompt、footer slot。
- `ref/opencode/packages/tui/src/routes/session/index.tsx`：Session 主屏、transcript、权限/问题、sidebar toggle。
- `ref/opencode/packages/tui/src/routes/session/sidebar.tsx`：侧栏。
- `ref/opencode/packages/tui/src/component/prompt/index.tsx`：可扩展 prompt/composer。
- `ref/opencode/packages/tui/src/component/logo.tsx`、`spinner.tsx`：品牌与加载。
- `ref/opencode/packages/tui/src/component/command-palette.tsx`：命令面板。
- `ref/opencode/packages/tui/src/component/dialog-*.tsx`：dialog family。
- `ref/opencode/packages/tui/src/feature-plugins/builtins.ts`：home/session/sidebar feature slots。
- `ref/opencode/packages/tui/src/feature-plugins/home/footer.tsx`、`tips-view.tsx`、`tips.tsx`：首屏 footer 与 tips。
- `ref/opencode/packages/tui/src/feature-plugins/system/diff-viewer.tsx`、`diff-viewer-file-tree.tsx`、`diff-viewer-ui.tsx`：diff 视图。
- `ref/opencode/packages/tui/src/theme/index.ts` 与 `src/theme/assets/*.json`：主题系统。
- `ref/opencode/packages/tui/src/ui/dialog*.tsx`、`toast.tsx`：基础 UI。

视觉特点：
- UI 插件化，screen 从 layout/flexbox 组成，信息密度高但层次清晰。
- 首屏极简：logo + prompt + tips/footer。
- Session 采用 sidebar 上下文面板 + dialog overlay。
- 大量清单/弹窗交互，适合多任务。

## codex

整体定位：聊天历史 cell 系统，强终端感知颜色与底部面板。

关键文件：
- `ref/codex/tui/src/tui.rs`：终端生命周期、事件流、帧调度。
- `ref/codex/tui/src/app.rs`：顶层应用状态。
- `ref/codex/tui/src/chatwidget.rs` 与 `ref/codex/tui/src/chatwidget/*.rs`：聊天主控件。
- `ref/codex/tui/src/bottom_pane/mod.rs`、`chat_composer.rs`、`footer.rs`、`approval_overlay.rs`、`paste_burst.rs`：底部面板家族。
- `ref/codex/tui/src/history_cell/mod.rs` 及 `messages.rs/exec.rs/plans.rs/approvals.rs/patches.rs`：历史 cell。
- `ref/codex/tui/src/render/renderable.rs`、`highlight.rs`、`line_utils.rs`：渲染抽象/高亮。
- `ref/codex/tui/src/markdown*.rs` 与 `markdown_render/*.rs`：Markdown 渲染。
- `ref/codex/tui/src/diff_render.rs`：diff。
- `ref/codex/tui/src/motion.rs`、`shimmer.rs`：動效。
- `ref/codex/tui/src/color.rs`、`style.rs`、`terminal_palette.rs`、`terminal_probe.rs`、`theme_picker.rs`：颜色与主题。
- `ref/codex/tui/src/app/history_ui.rs`、`thread_settings.rs`：历史与设置。

视觉特点：
- 极简颜色哲学，终端感知，强调 adaptive style。
- shimmer 加载态、status indicator。
- 消息单元保留 raw/display 双视图，利于 copy 与渲染。
- 审批 overlay 明确且层级清晰。

## jcode

整体定位：Rust TUI，面向会话/工作区，侧边栏、usage overlay、session picker、diff/info widget。

关键文件：
- `ref/jcode/crates/jcode-tui/src/tui/ui_layout.rs`、`ui_panel_image_preview.rs`、`info_widget_layout.rs`、`info_widget_git.rs`：布局与信息组件。
- `ref/jcode/crates/jcode-tui/src/tui/session_picker.rs` 及 `session_picker/*.rs`：session picker。
- `ref/jcode/crates/jcode-tui/src/tui/app/split_view.rs`、`todos_view.rs`、`commands_review.rs`：拆分视图与审查。
- `ref/jcode/crates/jcode-tui/src/tui/ui_viewport.rs`、`ui_pinned_layout.rs`：视口与 pinned layout。
- `ref/jcode/crates/jcode-tui-anim`、`jcode-tui-render`、`jcode-tui-style`：动画、渲染、样式。
- `ref/jcode/crates/jcode-tui-workspace/*`：workspace map。
- `ref/jcode/crates/jcode-tui-mermaid`：Mermaid 图表。
- `ref/jcode/crates/jcode-tui-usage-overlay`：usage overlay。
- `ref/jcode/crates/jcode-tui-permissions`：权限覆盖。
- `ref/jcode/crates/jcode-tui-messages`：消息可视化。

视觉特点：
- 侧栏与上下文信息面板优先于多 screen。
- session picker 是 TUI 主要导航，而非 dashboard。
- workspace map 可視化 repository/任务。
- 动画、usage、mermaid 作为增强 visual capability。

## 当前项目可直接借鉴的模块边界

- `crates/app/cli/wf-tui/src/interactive/render.rs` 的 redraw grading 可扩展到所有 screen，但当前管理 screen 仍整帧重画。
- `crates/app/tui/tui-render/src/screen_draw.rs` 集中绘制，参考 atomcode 的 retained renderer 思路。
- `crates/app/tui/tui-components/src/*` 已有 modal/overlay family，适合继续扩展。