# Transcript、覆盖层、Settings 与 Git 对比

## 1. Transcript / History Overlay

当前：
- `screen_draw.rs::draw_transcript_overlay` 全屏 reflow，tail anchored。
- 通过 `HistoryOverlayView` 管理 session_id、lines、scroll、has_more、loading、error。
- 重算 refugees role color mapping (`theme.style_for_role(source.role.to_color_role())`)。
- `crates/app/cli/wf-tui/src/pager.rs` 处理分页。
- `crates/app/tui/tui-components/src/transcript.rs` 提供 `HistoryLine`，display_lines + width reflow。
- jump badge paint 于 overlay。

参考：
- codex `history_cell/*`：HistoryCell trait 有 `display_lines()` / `raw_lines()`，消息、执行、计划、审批、patch separate cell types。
- codex `app/history_ui.rs`、`session_state.rs`。
- opencode session timeline dialog + message dialog。
- jcode `session_picker`、`jcode-tui-messages`。
- atomcode transcript? 参考 `render/retained.rs` 与 modal/file viewer。

差距：
- 当前 `HistoryLine` 是通用行；参考将语义 cell 作为一等单元，支持 copy/raw。
- 当前 reflow 每帧重建，参考 retained/diff 或虚拟化。
- 当前 overlay 全屏， disrupting flow；参考 codex history 是 embedded/overlay hybrid。

改善方向：
- History should be pager/virtualized with roles preserved；对齐 codex HistoryCell raw_lines.
- 利用 `prep_cache`/prep cache 的 row range 复用，避免全量 reflow 错峰。

## 2. Search Palette / Command Palette

当前：
- `draw_search_palette`：centered modal，`centered_rect(70, 60)`，input + result list；score 显示。
- state：running/truncated/error/no results。
- `crates/app/tui/tui-components/src/modal.rs` / `help_modal.rs`。
- footer `FooterRoute::Command` 也有 `/` CommandPalette 面板（`CommandId`, `/model`, `/copy` 等）。

参考：
- opencode `component/command-palette.tsx`：插件化命令面板。
- codex `selection_popup.rs` / bottom pane selection。
- atomcode modal family (`modals/*.rs`)。
- jcode session picker filtering。

差距：
- 当前 palette 已不错，但 command results 缺少 actions/context/shortcuts。
- modal palette 与 footer command route 是两套 command surface，状态语义需统一。
- 借鉴 opencode command-palette 的 command registry 与 preview，以及 atomcode modal family 的一致按键。

## 3. Sidebar Overlay

当前：
- `draw_sidebar_overlay`：左侧 30% screen list，cyan fixed + Clear。
- 仅 screen navigation，title "Navigation"；digit 对有 digit screen 显示，其他 `·`。
- 不是上下文 sidebar。

参考：
- opencode `routes/session/sidebar.tsx`：context sidebar（files、MCP、todo 等）。
- jcode side panel tests / split view。
- codex side display。

差距：当前 “sidebar” 是 navigation overlay；参考 sidebar 是 contextual tool panel。概念混淆。应重命名或拆分为导航 palette 与 context sidebar。

## 4. Settings

当前：
- `draw_settings` 完整文本块，Theme/LLM profiles/local config 与 toggle rows。
- 显示：Theme，profiles + default marker，local config toggles（mouse capture, session scrollbar, copy_target, tool output/level, preview lines/chars, appearance theme, motion performance, notification on/off/auto）。
- `m` 打开 ModelPicker；本地 config 保存到文件，session scrollbar 即时。
- scrollbar。

参考：
- atomcode `modals/config_panel.rs`、`modals/model_picker.rs`、`modals/provider_panel.rs`。
- codex `settings_popups.rs`、`thread_settings.rs`、`theme_picker.rs`。
- opencode dialogs theme/model/provider/status/console-org。

建议：
- UI 形态对齐 opencode dialog family；appearance theme/motion 对齐 codex theme_picker。
- 数据与 profile 对齐 atomcode/codex settings popups。
- dynamic theme switching 对齐 codex theme_picker + opencode theme assets。

## 5. Approval / Question overlays

当前：
- `tui_components::approval_overlay.rs`, `question_overlay.rs` 提供 `ApprovalView` / `QuestionView`。
- 通过 `FooterView::Permission` / `Question` 嵌入 footer 主区域，高度 12/14 rows。
- approval 有 remembered session decision；question 组件派生于 request。
- 弹出交互上仍使用 component family，但显示位置是 bottom pane，不对齐老式的居中 overlay。

参考：
- codex `bottom_pane/approval_overlay.rs`、permission overlays。
- jcode `jcode-tui-permissions`。
- opencode permission/question prompts。

改善方向：
- 继续将 permission/question 保持在 bottom pane 堆栈，而不是 returned modal；对齐 codex 底部面板。
- permission risk style 对齐 jcode permission crate 与 opencode permission dialog。

## 6. Git

当前：
- `draw_git`：status list + detail 50/50，status coloring。
- detail 可通过 FileViewer 打开；`r refresh`、Enter detail。
- 没有 diff viewer / branch tree / commit graph。
- Interactive 中 tool viewer 可根据 `is_diff` 使用 `DiffViewer::from_unified`。

参考：
- atomcode `modals/diff_viewer.rs`、`git_diff.rs`、`git/worktree.rs`。
- opencode diff-viewer feature plugin tree/UI。
- codex `diff_render.rs`、patches cell。
- jcode `info_widget_git.rs`。

设计决策：
- Git screen 保持只读 status summary + detail；不实现 file tree、hunk viewer 或交互式 git 操作，外部 `git`/`lazygit` 承担完整 Git 工作流。
- `DiffViewer` 仅用于 Interactive tool output 的 unified diff，不接入 Git screen。
- status list 保持 Navigation/sidebar/search palette 中的低频访问入口，不分配 digit shortcut。

## 7. 动效与颜色

当前：
- Theme role + selected black-on-accent；spinner + jump badge。
- Theme OSC 探测；terminology tui-terminal 拥有 capabilities/probe/sigint/stderr guard。
- redraw grading + prep cache；scrollbar visible/clamp。
- tui-style：animation/anim_core/motion/harmony/oklab/theme cache/resolve/derive。

参考：
- codex 终端感知颜色 + 明暗 + shimmer。
- atomcode retained diff + terminal bg。
- opencode theme assets。
- jcode anim crate。

建议：
- 颜色系统对齐 codex terminal_palette/style/color（term-probe, adaptive）。
- 动画对齐 jcode anim 与 atomcode retained diff。
- 主题切换对齐 opencode theme assets + codex theme_picker。

## 8. wf-mini 说明

wf-mini 是轻量交互邻接界面，不直接具备 screen universe。视觉一致性通过 shared tui-style/theme/component subset 实现；canonical markdown dialect 与 full TUI 共用。参考 jcode/atomcode 的 retained/diff engine 可作为 mini 的后续升级方向；`DiffViewer` 可复用给 mini 的 diff 展示。

## 9. 组件清单补充

当前 `tui-components` 已含，但有部分组件在 Full TUI 中已接线，部分主要存在于 component gallery/examples：
- 已接线：composer、footer、scrollbar、transcript、modal、model_picker、approval/question overlay、file_viewer/DiffViewer、property_list/data_table/tab_bar/toggle_row、panels (command/model/skill/queued/workflow/mention)。
- 库中存在但 Full TUI 主路径未直接使用：`bottom_pane`、`context_sidebar`、`command_palette`（非 footer command panel 的独立组件）、`file_selection`、`file_tree`、`select`、`overlay`。
- `queue` 已接入：交互会话在 turn 运行期间提交的 prompt 进入队列，turn 结束后按序 drain，queued panel 负责编辑与删除。
