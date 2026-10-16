# TUI 多标签页改进实施记录

对照 `tui-multi-tab-comparison-analysis.md` 的差距分析，分阶段落地如下。所有修改只改原模块，未新增替代模块。

## 阶段一：核心模型扩展（高优先级基础）

- `ChatTabMeta` 增加 `pinned`、`parent_id`、`model`、`agent`、`workspace` 字段，`ChatTabs::add` 跳过固定标签页淘汰，仅在后台全固定时才淘汰固定页。
- `ChatTabs` 新增 `toggle_pin`、`set_pinned`、`set_parent`、`set_meta`、`index_of`、`children_of`、`focus_parent`、`focus_child_first`、`focus_child_next`、`focus_child_prev`、`move_tab`、`prune_missing`、`workspace_groups`。
- 参考：`crates/app/tui/tui-core/src/screen_data.rs`
- `SessionTab` 同步增加上述元数据字段与 `with_parent`、`execution_id`；`SessionTabs` 增加 `active_tab`、`get`、`iter`、`toggle_pin`、`set_active_meta`、`index_of_execution`、`children_of`、父子聚焦、`move_tab`（保持 holder 挂接跟随）、`prune_missing`；`eviction_victim` 同样跳过固定页。
- 参考：`crates/app/cli/wf-tui/src/session_tabs.rs`
- `SessionTabItem` 增加 `pinned`，`session_tab_cells` 前缀固定标记 `◆`，`draw_session_tab_bar` 对固定非活跃页加粗。
- 参考：`crates/app/tui/tui-components/src/tab_bar.rs`
- 新增按键动作 `TabPin`、`TabMovePrev`、`TabMoveNext`、`TabParent`、`TabChild`、`TabChildNext`、`TabChildPrev`、`SessionList`，默认绑定 `Alt-P` 固定、`Alt-S` 会话列表、`Alt-,/.` 移动、`Alt-Up/Down` 父子、`Alt-J/K` 子节点循环。
- 参考：`crates/app/tui/tui-core/src/keymap.rs`、`crates/app/cli/wf-tui/src/tui/keymap_config.rs`
- 覆盖层新增 `SessionList`。
- 参考：`crates/app/tui/tui-components/src/overlay.rs`

## 阶段二：Shell 集成（高优先级）

- 新增会话列表浏览器状态 `SessionList`（选择、上下移动、越界钳制）。
- 参考：`crates/app/cli/wf-tui/src/session_list.rs`
- 新增标签页持久化 `tab_persistence`：快照版本 1，文件为 `$XDG_STATE_HOME/wf/tabs.json`，保存会话标识、标题、固定、父子、模型、代理、工作区、活跃时间与活跃下标；加载失败返回空，保存为原子替换，失败静默。
- 参考：`crates/app/cli/wf-tui/src/tab_persistence.rs`
- 会话列表渲染 `draw_session_list`（序号、固定标记、流式标记、当前标记；按键提示含切换、关闭、固定、清理）。
- 参考：`crates/app/tui/tui-render/src/screen_draw.rs`
- `TuiApp` 新增 `session_list` 状态、`tab_strip_area` 命中矩形；`new_tab` 经 `new_tab_with_parent` 记录工作区并持久化；`close_tab`、切换、固定、移动后均持久化；首次进入会话屏时恢复固定标签页并回放历史；`prune_closed_tabs` 仅清理空闲未固定后台页（活跃、流式、固定、一分钟内触摸的页一律保留），无 executions 快照时不清理。
- 参考：`crates/app/cli/wf-tui/src/tui.rs`
- 按键路由拦截新动作并在会话列表覆盖层中处理 `Enter/j/k/p/d/x/数字/Esc`。
- 参考：`crates/app/cli/wf-tui/src/tui/key_routes.rs`
- 帮助文本补充标签页快捷键。
- 参考：`crates/app/tui/tui-components/src/help_modal.rs`

## 阶段三：中优先级（父子导航、元数据、工作区）

- 父子导航已在纯数据层与 live 层实现并接入 `Alt-Up/Down/J/K`；`new_tab_with_parent` 预留子标签页创建入口。
- 元数据 `model/agent/workspace` 读写接口已就绪，会话列表与快照均携带；服务端模型回填待后续接入。
- 工作区分组 `workspace_groups` 已在纯数据层实现，会话列表按工作区展示为后续步骤。

## 阶段四：低优先级（点击切换）

- 标签栏单击切换已实现：`draw_inner` 发布 `tab_strip_area`，`handle_mouse_press` 命中时切换并持久化，按宽度均分计算下标。
- 参考：`crates/app/cli/wf-tui/src/tui.rs`
- 标签页拖拽重排未做：已有键盘移动 `Alt-,/.` 覆盖核心诉求，拖拽命中复杂且收益低。
- 插件槽位未做：TUI Shell 无插件运行时，对应 OpenCode 的 `pluginRuntime.Slot` 在本项目无挂载点。

## 验证

- `cargo clippy --all-targets --all-features` 通过。
- `cargo test -p tui-core -p tui-components -p tui-render -p wf-tui --lib` 通过（544 项）。
