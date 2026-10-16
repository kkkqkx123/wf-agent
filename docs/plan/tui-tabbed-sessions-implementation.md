# TUI 多标签页会话细化修改方案

基于 `docs/plan/tui-tabbed-sessions-design.md`，对照 `ref/opencode` 的实现与当前仓库实际代码，给出可直接执行的细化修改方案。

结论：`ref/opencode` 的 TUI 侧是轻量快速切换（9 个 pin 槽位加 `<leader>1-9` 加会话列表对话框加 `session.json` 持久化），切换时重新加载目标会话，不在 TUI 进程内保留多会话的完整状态。本文方案保留其快捷键人体工学，但改为在单个 TUI 进程内保留多个 `InteractiveController` 实例，使切换不需要重新加载历史与流式状态，同时共享 client 连接、终端后端与渲染管线。

## 0. 实际代码锚点

- 会话状态机：`crates/app/cli/wf-tui/src/interactive.rs` 的 `InteractiveController`（单会话完整状态：`SessionReducer`、`Footer`、`scrollback`、`turn_task`、`poll_task` 等）。
- 壳层：`crates/app/cli/wf-tui/src/tui.rs` 的 `TuiApp`（`interactive: Option<InteractiveController>` 单实例，`held_session: SessionHolder`，`ensure_interactive` 在离开 Interactive 屏时销毁控制器，`draw_inner` 绘制活跃会话，`event_loop` 只泵送一个控制器）。
- 按键入口：`crates/app/cli/wf-tui/src/tui/key_routes.rs` 的 `handle_key`（Interactive 屏直接委托给 `session.handle_key`），`crates/app/cli/wf-tui/src/interactive/keys.rs`（`Alt+N`、`Alt+P` 已被锚点跳转占用，`Alt` 加数字空闲，`Ctrl+T`、`Ctrl+B`、`Ctrl+K` 已被历史、边栏、搜索占用）。
- 纯数据层：`crates/app/tui/tui-core/src/screen_data.rs`（`ScreenKind::Interactive` 是唯一自包含屏），`crates/app/tui/tui-core/src/keymap.rs`（集中式按键解析）。
- 标签页条现状：`crates/app/tui/tui-components/src/tab_bar.rs` 只有管理屏用的纯文本 `format_tabs`，没有会话标签页的 ratatui 绘制。

## 1. 快捷键终选（避开现有占用）

`Alt+N` 与 `Alt+P` 已被锚点跳转占用，不可复用。终选：

- `Alt+1` 到 `Alt+9`：切换到指定标签页。
- `Alt+T`：新建标签页。
- `Alt+W`：关闭当前标签页。
- `Alt+]`、`Alt+[` 与 `Alt+Right`、`Alt+Left`：下一个、上一个标签页。
- 大小写两种 `Key` 都注册（终端送达的字符大小写与 shift 状态有关，参考 `keys.rs` 对 `o` 与 `O` 的双匹配写法）。

以上按键由壳层在委托给 `InteractiveController` 之前拦截，不进入会话的 composer 与浮层。

## 2. 第一阶段：纯数据结构与按键解析

目标：不碰异步与渲染，先把可单元测试的纯逻辑落地。

### 2.1 `tui-core/src/screen_data.rs` 新增标签页元数据模型

- 新增 `ChatTabMeta`（标签页标识、标题、最后活跃毫秒时间戳）与 `ChatTabs`（`tabs` 向量加 `active` 下标）。
- 常量 `MAX_CHAT_TABS` 为 9，与 `ref/opencode` 的 9 个快速槽位对齐。
- 方法：新建、数量、是否为空、当前活跃元数据、按下标切换、循环下一个与上一个、新增（满时淘汰最久未活跃的非活跃页并修正下标）、关闭（修正下标，保证非空时下标合法）、触摸活跃时间、改标题、标题列表。
- 单元测试覆盖：新增与上限淘汰、切换越界失败、关闭末页与中间页的下标修正、循环换页回绕。

### 2.2 `tui-core/src/keymap.rs` 新增标签页动作

- `KeyAction` 新增 `TabNew`、`TabClose`、`TabNext`、`TabPrev`、`TabPick(u8)`。
- 全局默认绑定注册大小写双份的 `Alt+T`、`Alt+W`、`Alt+]`、`Alt+[`、`Alt+Left`、`Alt+Right` 与 `Alt+1` 到 `Alt+9`。
- 单元测试覆盖各默认绑定的解析，以及用户全局覆盖优先于默认。

### 2.3 `wf-tui/src/tui/keymap_config.rs` 支持用户覆盖

- `parse_action` 新增 `tab_new`、`tab_close`、`tab_next`、`tab_prev`，以及 `tab_pick_1` 到 `tab_pick_9` 的模式解析。
- 未知 action 仍跳过并告警，不中断启动。

验收：`cargo test -p tui-core` 通过。

## 3. 第二阶段：多控制器管理与壳层集成

目标：一个 TUI 进程内保留多个 `InteractiveController`，共享 client、终端、渲染管线。

### 3.1 新增 `wf-tui/src/session_tabs.rs`（按职责拆分的新模块，非替代性重写）

- `SessionTab` 条目：`controller: InteractiveController`、`holder: SessionHolder`（每页独立持有远端会话标识与连接状态，替代原来 `TuiApp` 上的单例 `held_session`）、`title: String`、`last_active_ms: u64`。
- `SessionTabs` 管理器：向量加 `active` 下标。
  - 查询：数量、是否为空、活跃下标、活跃不可变与可变借用、全部可变迭代（事件泵送用）、是否有后台流式（活跃页之外的 `is_processing`）、标题与流式快照（供标签页条渲染）。
  - 变更：`push`、`switch`（更新前后两页的 `last_active_ms` 并 attach 当前、detach 其余）、`next`、`prev`、`remove`（同步摘除并返回条目，调用方再异步 `shutdown`，因为 `shutdown` 按值消费控制器）。
  - 满 9 页时新建页的淘汰策略：摘除最久未活跃的非活跃页，返回被淘汰条目由调用方关闭；标题取执行标识前 8 位。
- 单元测试覆盖：切换更新活跃时间、越界切换失败、摘除中间页的下标修正、淘汰最久未活跃页。

### 3.2 `TuiApp` 结构变更（`tui.rs`）

- 删除 `interactive: Option<InteractiveController>` 与 `held_session: SessionHolder` 字段，替换为 `sessions: SessionTabs`。
- 新增 `interactive()` 与 `interactive_mut()` 取活跃页控制器的代理方法，其余约 40 处 `self.interactive` 直引点改为经由代理或 `sessions` 查询（不可变用 `active`，可变用 `active_mut`，存在性判断用 `is_empty` 取反，`take` 语义改为摘除活跃页并异步关闭）。
- `ensure_interactive` 改为 `ensure_active_tab`：
  - 在 Interactive 屏且页数为空时创建首个标签页（沿用原初始化序列：生成标识、`hold`、`InteractiveController::start`、应用性能档、拷贝策略、工具输出配置、滚动条开关、`attach`）。
  - 离开 Interactive 屏时不再销毁控制器，只将各页 holder `detach`；返回时将活跃页 `attach`。
  - `draw_inner` 的分支条件从控制器是否存在改为当前是否在 Interactive 屏且有活跃页，使管理屏导航不再丢失会话。
- `run` 的收尾关闭改为遍历关闭所有标签页（逐个 `begin_graceful_exit` 后 `shutdown`），`apply_interactive_exit` 改为仅关闭活跃页后返回上一屏（页数归零时行为与原来一致）。
- `apply_pending_replay`（从 Executions 屏回放）改为：有空闲页则复用活跃页，否则新建页再载入回放；页满时淘汰最久未活跃页并提示。
- 事件循环每轮泵送全部控制器（活跃页与后台页都调用 `handle_events`）；若后台页处于流式则请求一次整帧，使标签页条的流式指示器及时更新；动画与帧率判定只看活跃页，保持空闲开销不变。
- 鼠标、粘贴、拷贝、编辑器交接、尺寸变更等只作用于活跃页。

### 3.3 `SessionHolder` 本体不变

- 状态机与单页语义保持原样，只是持有者从每进程一个变为每标签页一个。

验收：单标签页时的创建、导航离开再返回、退出、回放行为与原来一致；多标签页下后台页的 `turn_task` 与 `poll_task` 继续推进。

## 4. 第三阶段：标签页条界面与按键接线

### 4.1 `tui-components/src/tab_bar.rs` 新增会话标签页条绘制

- 保留原有 `format_tabs` 不动，新增会话标签页条的数据快照类型（标题、是否流式、是否活跃）与宽度自适应截断函数（纯函数，可单元测试）。
- 新增 ratatui 绘制函数：在给定区域内绘制 `1 标题(流式时加省略号标记)` 序列，活跃页高亮，非活跃页普通样式，超宽时对标题做截断并保留编号与状态标记。
- 复用 `tui-style` 主题，不引入新的主题来源。

### 4.2 `tui.rs draw_inner` 预留标签页条高度

- 仅当当前在 Interactive 屏且页数大于 1 时，在内容区顶部预留 1 行绘制标签页条；单页时不预留，保持与原来逐像素一致。
- 绘制所需快照从 `SessionTabs` 的标题与流式查询组装，不借用控制器可变引用。

### 4.3 `tui/key_routes.rs` 拦截标签页按键

- 在 `ScreenKind::Interactive` 委托给 `session.handle_key` 之前，先经 `keymap` 解析标签页动作并处理：
  - 新建页为异步（`InteractiveController::start` 是异步），按键处理函数需改为异步或发出新建请求由调用方在 `await` 点执行；切换页与关闭页为同步摘除加异步关闭的组合，关闭的 `shutdown` 在按键处理后 `await`。
  - 新建页沿用 `ensure_active_tab` 的初始化序列并切换过去；关闭页后若无页则按原来退出会话的路径返回上一屏。
- 非 Interactive 屏不处理标签页按键（标签页只在会话屏内有效）。
- 帮助浮层追加标签页快捷键说明行。

验收：`Alt+数字`、`Alt+T`、`Alt+W`、`Alt+[`、`Alt+]` 在会话屏生效且不穿透到 composer；单页时界面与原来一致。

## 5. 第四阶段：测试与验证

- `tui-core`：标签页元数据模型的增删改查与淘汰、标签页按键解析。
- `tui-components`：标签页条截断与快照组装的纯函数测试。
- `wf-tui`：`SessionTabs` 管理器的切换、关闭、淘汰逻辑测试；多页 pump 与绘制分支的手工验证（至少一次真实 TUI 启动或一次回放快照测试）。
- 合并验证只做少量整包编译：`cargo fmt` 后跑 `cargo test -p tui-core -p tui-components`，再跑 `cargo clippy -p wf-tui --all-targets`（按仓库紧张资源要求，不做全工作区反复编译）。

## 6. 与 `ref/opencode` 的差异与取舍

- 采用其 9 页上限与数字切换人体工学，不采用其每次切换重新加载的轻量模式（本方案保留多控制器的完整状态，切换只换渲染目标与输入焦点）。
- 不做 `session.json` 式跨重启持久化、不做拖拽排序与预览浮层（TUI 交互空间有限，首版只做内存内标签页）。
- 不照搬其 `mod+1` 式快捷键（与本仓库已占用的 `Alt+N`、`Ctrl+T` 等冲突），改用第 1 节的终选组合。
- 资源共享与 `ref/opencode` 一致：共享服务器连接（此处为克隆的 `WfClient` 句柄）、共享终端后端与渲染管线、每页独立滚动历史、输入状态与流式 reducer。
