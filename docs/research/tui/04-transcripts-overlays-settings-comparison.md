# Transcript、覆盖层、Settings 与 Git 对比

## 1. Transcript / History Overlay

当前：
- `screen_draw.rs::draw_transcript_overlay` 全屏 reflow，tail anchored。
- 负责刷新所有加载行，纯 ratatui Paragraph/scrollbar。
- `crates/app/tui/tui-components/src/transcript.rs` 提供 `HistoryLine`。
- `crates/app/cli/wf-tui/src/pager.rs` 处理分页。

参考：
- codex `history_cell/*`：HistoryCell trait 有 `display_lines()` / `raw_lines()`，消息、执行、计划、审批、patch  separate cell types。
- codex `app/history_ui.rs`、`session_state.rs`。
- opencode session timeline dialog + message dialog。
- jcode `session_picker`、`jcode-tui-messages`。
- atomcode transcript? 参考 `render/retained.rs` 与 modal/file viewer。

差距：
- 当前 `HistoryLine` 是通用行；参考将语义 cell 作为一等单元，支持 copy/raw。
- 当前 reflow 每帧重建，参考 retained/diff 或虚拟化。
- 当前 overlay 全屏， disrupting flow；参考 codex history 是 embedded/overlay hybrid。

## 2. Search Palette / Command Palette

当前：
- `draw_search_palette`：centered modal，input + result list，score 显示。
- `crates/app/tui/tui-components/src/modal.rs` / `help_modal.rs`。

参考：
- opencode `component/command-palette.tsx`：插件化命令面板。
- codex `selection_popup.rs` / bottom pane selection。
- atomcode modal family (`modals/*.rs`)。
- jcode session picker filtering。

差距：
- 当前 palette 已不错，但 command results 缺少 actions/context/shortcuts。
- 借鉴 opencode command-palette 的 command registry 与 preview，以及 atomcode modal family 的一致按键。

## 3. Sidebar Overlay

当前：
- `draw_sidebar_overlay`：左侧 30% screen list，cycan style fixed。
- 仅 screen navigation，不是上下文 sidebar。

参考：
- opencode `routes/session/sidebar.tsx`：context sidebar（files、MCP、todo 等）。
- jcode side panel tests / split view。
- codex side display。

差距：当前 “sidebar” 是 navigation overlay；参考 sidebar 是 contextual tool panel。概念混淆。应重命名或拆分为导航 palette 与 context sidebar。

## 4. Settings

当前：
- `draw_settings`：Theme/LLM profiles/local config in text block。
参考：
- atomcode `modals/config_panel.rs`、`modals/model_picker.rs`、`modals/provider_panel.rs`。
- codex `settings_popups.rs`、`thread_settings.rs`、`theme_picker.rs`。
- opencode dialogs theme/model/provider/status/console-org。
建议：
- UI 形态对齐 opencode dialog family；
- 数据与 profile 对齐 atomcode/codex settings popups；
- 动态主题切换对齐 codex theme_picker。

## 5. Approval / Question overlays

当前：
- `tui-components/src/approval_overlay.rs`, `question_overlay.rs`。
参考：
- codex `bottom_pane/approval_overlay.rs`、permission overlays；
- jcode `jcode-tui-permissions`。
- opencode permission/question prompts。
建议：
- 将 approval 从 overlay 退到 bottom pane 堆栈，对齐 codex 底部面板；
- permission 风险样式对齐 jcode permission crate 与 opencode permission dialog。

## 6. Git

当前：
- `draw_git` status list only。
参考：
- atomcode `modals/diff_viewer.rs`、`git_diff.rs`、`git/worktree.rs`。
- opencode diff-viewer feature plugin tree/UI。
- codex `diff_render.rs`、patches cell。
- jcode `info_widget_git.rs`。
建议：
- 交互与可视化对齐 opencode diff-viewer file tree + atomcode diff viewer；
- diff hunk rendering 对齐 codex diff_render。

## 7. 动效与颜色

当前：
- Theme role + spinner + jump badge；Theme OSC 探测 `osc.rs`。
参考：
- codex 终端感知颜色 + 明暗 + shimmer。
- atomcode retained diff + terminal bg。
- opencode theme assets。
- jcode anim crate。
建议：
- 颜色系统对齐 codex terminal_palette/style/color。
- 动画对齐 jcode anim 与 atomcode retained diff。
- 主题切换对齐 opencode theme assets + codex theme_picker。

## 8. wf-mini 说明

wf-mini 是轻量交互邻接界面，不直接具备 screen universe。视觉一致性应通过 shared tui-style/theme/component subset 实现，不强制 screen navigation。参考 jcode/atomcode 的 retained/diff engine 可作为 mini 的后续升级方向。