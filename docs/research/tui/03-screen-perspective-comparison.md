# 当前 Screen 与参考项目视角对比

## A. Dashboard

当前：
- `screen_draw.rs::draw_dashboard` 渲染计数 + 全部 screen 列表 + 最近执行。
- 视觉密度中等；`>` 选中 marker；无图标/卡片；最近执行最多 5 条。

可对应参考：
- opencode `routes/home.tsx`：Home 以 logo + prompt 为中心，screen navigation 更少。
- atomcode welcome/mascot/tips：品牌态与 onboarding。
- jcode session picker：将导航放在 picker/loader，而非 dashboard。
- codex chat 空态：`Type a prompt and press Enter...` 风格。

差距：
- 当前 dashboard 是功能目录，不是品牌/任务起始面。
- 参考项目更强调“立即开始会话”，当前把 Chat 作为 digit 4 的一个 screen。

建议方向：
- 将 Dashboard 从 screen catalog 转为“workspace overview + quick actions”，或默认进入 Interactive。
- 借鉴 opencode Home 的居中 prompt 和 atomcode welcome tips。

## B. Workflow

当前：
- master-detail；views 为文本 tab；字段文本；detail pane。
- 参考：opencode dialog list；jcode session picker list/filter；codex selection view。
差距：当前 lacks status visualization、node graph preview、run timeline；Workflow view 应该以画布/节点树或 step timeline 呈现。 reference `jcode-tui-workspace` workspace map 可借鉴 repository map。

建议：
- Workflow 列表可借鉴 jcode session picker 的 filter + detail side panel。
- detail 可借鉴 opencode diff-viewer tree 的 drilldown。
- 不再用 50/50 文本分栏作为主界面；改为 list + inspect drawer。

## C. Executions

当前：
- execution rows + status color；subviews Overview/Messages/Tools/Timeline/Variables。
- 参考：codex transcript/history cell；opencode session timeline dialog；jcode session facts。
差距：timeline/variables 应对应 codex execution cell 与 opencode timeline dialog；当前纯文本 detail 与 status color 不足。
建议：
- 与 codex `history_cell/exec.rs` 对齐：每个 execution 为 cell，包含 command、output、status。
- Timeline 对齐 opencode `dialog-timeline.tsx`。
- Variables 对齐 opencode sidebar/context panel。

## D. Insights

当前：tables 手工对齐。
参考：codex table render/markdown_render/table_key_value；opencode feature plugins。
建议：方案上对齐 codex markdown table renderer，支持 key/value、header、border；如后续图表，借鉴 jcode visual-debug/usage overlay。

## E. Interactive / Chat

当前：
- scrollback + footer + composer + spinner + overlays。
参考：
- codex `chatwidget.rs`, `bottom_pane/*`：history cell + bottom panel + approval overlay。
- opencode session route + sidebar + prompt。
- atomcode retained screen + modal。
- jcode split view + todos view。
差距：
- 历史单元应从 `HistoryLine` 升级到 HistoryCell 模型（display/raw/copy）。
- footer/composer 应更像 codex bottom pane，包含状态、key hints、approval 堆栈。
- 侧栏上下文应借鉴 opencode/jcode，而非所有 screen catalog。

建议：
- 对齐 codex chat cell 家族（message/tool/plan/approval/patch）。
- composer/approval/question 状态堆栈对齐 codex bottom pane。
- 侧栏上下文（files/todo/context/mcp）对齐 opencode sidebar。

## F. Checkpoints

当前：status list 全屏。
参考：jcode session picker、atomcode rewind modal。
建议：作为 modal/panel 而非主 screen；时间线/chain 视图对齐 jcode session/rewind。

## G. Settings

当前：文本配置块，shortcut 在 title。
参考：
- atomcode config panel/modal。
- codex thread_settings/settings popups。
- opencode dialog theme/model/provider/status。
建议：
- 借鉴 opencode dialog family：分 tab dialog；theme/model/provider separate。
- 本地 config 可保持文本，但 visual toggle 与 footer key hints 应标准化。

## H. Templates / Resources / Triggers / Events

当前：列表 + detail（部分）。
参考：
- opencode command palette / dialog list；
- codex selection view；
- jcode session picker filter.
建议：
- 合并为统一 searchable resource catalog，对齐 opencode command palette 的输入+结果模式。
- per-item detail drawer 对齐 atomcode modals。

## I. Git

当前：status list only。
参考：
- atomcode git diff/diff viewer；
- opencode diff-viewer feature plugin；
- codex diff_render；
- jcode info_widget_git。
建议：
- Git screen 应升级为 diff viewer：file tree + hunk viewer，对齐 opencode diff-viewer 与 atomcode diff viewer；status list 退为 sidebar summary。

## J. History Overlay

当前：full screen reflow transcript。
参考：
- codex history UI / transcript；
- opencode timeline dialog；
- jcode session picker/render。
建议：History should be pager/virtualized with roles preserved; 对齐 codex HistoryCell raw_lines.