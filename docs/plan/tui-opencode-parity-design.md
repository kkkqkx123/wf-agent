# OpenCode TUI 功能与交互分析及当前项目对齐设计

## 1. 调研范围与结论

本文以 `https://github.com/anomalyco/opencode/` 的 `packages/tui` 包为参考（快照提交 `a41d09b`，采集日期 2026-10-25），整理其 TUI 的功能划分、交互逻辑与数据组织方式，并与当前项目 `wf-agent` 的 `crates/app/cli/wf-tui`、`crates/app/tui/*` 逐项对比，给出需要补充的功能与实施方案。

核心结论：

- OpenCode 的 TUI 不是"屏幕集合"，而是**一套命令注册表驱动的单 Shell 应用**。键位、命令面板、slash 命令、帮助、页脚提示全部从同一份命令定义派生，因此交互一致、可发现性强。
- OpenCode 的会话页是一个**由投影规则生成的行模型**：原始消息先按语义分组，再投影为可增量更新的行，最后交给虚拟列表渲染。分组、折叠、分页、权限阻塞共享同一套规则。
- OpenCode 把"辅助输入"做成了**与主输入并列的 pane 和 tab**：右侧 pane 在 panel / sidebar / terminal 之间按优先级切换，输入区下方还有 subagents / shell / terminals 多 tab。
- 当前项目的优势在**执行可观测性与回放**：逻辑行投影、锚点、工具块/子代理块、转录选择、分页器、headless 回放、checkpoint 与文件时间线等能力是 OpenCode 不具备的。差距集中在**命令与键位的统一内核、可搜索命令面板、slash 命令体系、会话工作区的多 pane、权限 diff 预览、transcript 语义分组**这几处结构性能力。

因此本方案的目标不是"照搬 OpenCode"，而是**补齐命令与工作区的结构性缺口，同时保留当前项目在可观测性上的优势**。

## 2. OpenCode TUI 的整体架构

### 2.1 渲染底座与运行形态

- 技术栈为 TypeScript + SolidJS + 自研 `@opentui/core` / `@opentui/solid` / `@opentui/keymap`，采用全屏 flex 布局，而不是字符网格逐格绘制。
- 应用入口在 `packages/tui/src/app.tsx`，通过多层 Provider 装配能力：日志、退出、路径、存储、生命周期、终端环境、启动、剪贴板、参数、配置、键位、Toast、路由、客户端、权限、数据、位置、会话标签、会话终端、主题、本地状态、草稿暂存、对话框、使用频次、输入历史、编辑器上下文、注意力、更新提示、面板、插件。
- 单一 Shell 下由路由切换界面：`routes/home.tsx` 与 `routes/session/index.tsx`，插件可以注册额外路由。
- 布局为：顶部会话标签栏 + 主列（Home / SessionFrame / 插件路由），叠加 DevTools 条、启动加载、迁移覆盖层、Toast。

### 2.2 与当前项目的结构差异

当前项目是"全屏 Shell + 屏幕栈"：`crates/app/cli/wf-tui/src/tui.rs` 的 `TuiApp` 持有屏幕栈、模态栈、覆盖层模式、会话标签集合；`screens.rs` 定义 Dashboard / Workflow / Executions / Insights / Interactive / Checkpoints / Catalog / Git / FileWorkspace / FileTimeline / FileSync / EditSessions / FileApprovals / Conflicts / Settings 等屏幕；会话屏幕本身由 `interactive::InteractiveController` 承载。

差异要点：

| 维度 | OpenCode | 当前项目 |
| --- | --- | --- |
| 界面组织 | 路由 + 单会话工作区 | 屏幕栈 + 会话屏幕控制器 |
| 命令来源 | 注册表统一派生 | 动作枚举、面板常量、slash 常量分别维护 |
| 辅助区域 | 右侧 pane + 输入区 tab | 覆盖层侧栏 + 输入区面板 |
| 行模型 | 分组 + 增量投影 | 逻辑行投影 + 工具/子代理块（无分组） |

两者都有"覆盖层/模态"概念，但 OpenCode 的模态是统一的 dialog 栈，当前项目是 `ModalStack` 与若干专用 overlay 并存，样式与交互细节不统一。

## 3. 命令、键位与帮助体系

这是 OpenCode 最值得借鉴的部分，也是当前项目差距最大的一块。

### 3.1 单一定义表

`packages/tui/src/config/keybind.ts` 维护一张约 250 项的定义表，每一项包含：命令标识、默认键位、标题、分组、描述。键位值支持关闭、`none`、单键、组合键、对象与数组多种形态，解析时对未知键名报错。用户配置在解析后覆盖默认值，而非替换整张表。

当前项目的对应实现是 `crates/app/tui/tui-core/src/keymap.rs` 的 `KeyAction` 枚举（约 40 个动作）加 `builtin_defaults`，用户覆盖来自 `crates/app/cli/wf-tui/src/tui/keymap_config.rs`（`~/.config/wf` 与项目内 `.wf/keymap-settings.toml`）。问题在于：

- 动作是"行为"，不是"命令"：一个命令可能由多个动作组合，一个动作也可能被多个入口复用，缺少标题、分组、描述、别名。
- 命令面板的 `CommandId`（`crates/app/tui/tui-components/src/panels/command.rs`，目前 12 项）与键位动作是两套枚举，新增命令要改多处。
- 页脚提示、帮助页、命令面板各自硬编码文案，容易与真实绑定漂移。

### 3.2 键位分层与模式栈

`packages/tui/src/context/keymap.tsx` 用 `createLayer` 声明键位层，每层可指定模式、优先级、启用条件、命名命令与内联绑定。模式是一个栈：基础层之上压入 composer、modal 等模式，退出时弹栈。全局还内置了几类适配层：逗号分隔的多绑定、基础布局回退、Esc/Backspace 清理待决序列、受控文本域层、leader 超时（默认 2000 毫秒）。

同时提供聚合钩子：当前快捷键、全部命令、待决按键序列、活跃键集合。命令面板、帮助页、页脚、which-key 都消费这些聚合结果。

当前项目的 `Keymap` 是"全局表 + 上下文表 + 内置回退"的扁平结构（`KeymapContext` 含 Global/List/Detail/Chat/Input/Modal/Composer/Panel/Approval/Question），没有模式栈、层优先级、条件启用，也没有待决序列概念。

### 3.3 命令的三态注册

OpenCode 的一条命令可以同时具有三种暴露方式：

- 命令面板条目（有标题、分组、描述、是否建议）；
- slash 命令（名称、别名、参数说明）；
- 键位绑定（一个或多个）。

slash 命令覆盖面很宽：会话相关有新建、清空、打开、模型、agent、变体、连接、设置、状态、更新、配对、重启、重载、调试、主题、帮助、退出、插件、统计、diff、终端、切换目录、编辑器、技能、工作树、重命名、时间线、分叉、压缩、撤销、重做；界面相关有会话列表、最近会话、命令面板等。

当前项目的 slash 能力只有 `handle_command_line` 解析出的固定命令集（`/new`、`/model`、`/skills`、`/queued`、`/editor`、`/quit`、`/help`、`/workflows`、`/resume`、`/executions`、`/copy`、`/auth`），不支持别名、参数说明、补全与动态注册。

### 3.4 leader 与 which-key

默认 leader 为 `ctrl+x`，大量管理命令挂在 leader 之下（退出、新会话、会话列表、时间线、压缩、撤销/重做、复制、模型、agent、侧栏、终端、按序号选标签）。leader 按下后进入待决序列，超时或按键不匹配则取消；which-key 面板会列出当前可用的后续键。

当前项目没有 leader 与待决序列，也没有 which-key 提示面板。

### 3.5 快捷键展示与帮助

OpenCode 的对话框页脚按"次要在左、主要 Action 在右"分组，Action 可以只声明命令标识，由命令注册表反查第一个快捷键后显示；帮助对话框同样从命令列表生成。这样只要注册表正确，所有展示都不会漂移。

当前项目的页脚提示分散在 `crates/app/tui/tui-components/src/footer.rs`、`interactive/keys.rs` 与各覆盖层组件中，属于硬编码文本。

## 4. 界面构成与交互逻辑

### 4.1 会话工作区

OpenCode 的会话页由 `session-frame.tsx` 组织，右侧 pane 的显示遵循固定优先级：

1. 显式打开的面板（panel）；
2. 用户打开的侧栏（sidebar）；
3. 终端（terminal）；
4. 自动侧栏（`session.sidebar = "auto"` 且终端宽度超过阈值时）。

窄屏时侧栏以半透明覆盖层呈现而非挤压主列。焦点可在左右 pane 之间切换，`pane.focus.left/right`、`session.sidebar.toggle`、`terminal.toggle/select/close` 都是注册命令。侧栏内容不是写死的，而是由插件槽组合出上下文、待办、MCP、限额等区块。

当前项目的对应实现是 `crates/app/tui/tui-components/src/context_sidebar.rs` 的 `SidebarSnapshot`：固定五段（Files / Todos / Context / MCP / Git），由 `fetch/sidebar.rs` 拉取；显示与否来自配置 `behavior.sidebar`（`tui/input_classify.rs` 的 `load_sidebar_opt_in`），在 `tui/key_routes.rs` 中以覆盖层方式切换。没有"面板 > 侧栏 > 终端"的优先级模型，没有终端 pane，也没有左右焦点切换。

### 4.2 输入区（composer）

OpenCode 的输入区是独立子系统（`component/prompt/index.tsx` 与 `routes/session/composer/*`），主要能力：

- 提交分两种投递语义：立即插入（steer）与排队（queue）；排队可通过专门命令触发，并有优先级。
- 提交有防重入保护；新会话采用乐观创建，失败时把内容恢复为草稿，避免丢失输入。
- 草稿暂存：可暂存、弹出、列出，配合独立对话框管理。
- slash 命令解析与参数化执行；输入以退出词结尾时直接退出。
- `@` 提及（文件与实体）带 extmark 与异步搜索，输入中保留 token 边界。
- 图片附件粘贴队列与预览对话框；编辑器上下文可合成为一条消息。
- shell 模式：`!` 前缀把输入当作 shell 命令执行，结果作为工具输出进入转录。
- 输入区下方还有 composer 多 tab：子代理、shell、终端；左右切换，Esc 关闭。

当前项目的对应能力：`FooterRoute`（Composer / Command / Model / Skill / Queued / Workflow / Mention / Auth）、mention 复合路由（`interactive/panels.rs` 的 `mention_token_start`、`refresh_mention_entries` 等）、输入历史与反向搜索、`/editor` 外置编辑、排队面板、Ctrl+C 双击退出、粘贴处理。缺少：投递语义区分、草稿暂存、shell 模式、图片附件、编辑器上下文合成、composer 多 tab。

### 4.3 权限与提问

OpenCode 的权限提示（`routes/session/permission.tsx`）按工具类型给出不同证据：

- 编辑类工具显示补丁 diff，支持分屏与统一两种视图；
- 外部目录访问列出匹配路径模式；
- shell、子代理、任务类显示完整命令；
- 选项为"仅本次允许 / 始终允许 / 拒绝"，拒绝会进入理由输入框（Enter 确认、Esc 取消）；
- 支持全屏模式，且在等待期间保持其余界面的可读性。

当前项目的 `ApprovalView`（`crates/app/tui/tui-components/src/approval_overlay.rs`）提供工具名、风险级别、描述、批量上下文、参数预览与 y/a/d/n/c 五个选项，并有会话级"记住决定"机制，但**没有 diff 预览、没有拒绝理由、没有全屏模式**。提问视图支持数字选择与 Esc/Enter。

### 4.4 转录行模型与语义分组

OpenCode 的会话渲染分三层：

- 分组规则（`routes/session/grouping/session.ts` 与 `grouping/tree.ts`）：把连续同类条目合并为 activity、reasoning、exploration、instructions 四类组；探索类工具集合包含读取、glob、grep、网页抓取与搜索。
- 行投影（`routes/session/rows.ts`）：按稳定 key 做增量协调，避免整屏重建；分页时保证不切断组边界；权限阻塞条目会被单独分区。
- 渲染预算：只挂载可视范围内的组件，滚动时按需补齐。

三档详细度（低/中/高）控制是否展开推理与探索的正文。此外还有消息导航、锚点视图、时间线对话框、分叉对话框、消息详情对话框等辅助面。

当前项目在这一层已有较强基础：`interactive.rs` 的逻辑行投影与偏移、`tool_blocks.rs`、`subagent_blocks.rs`、`anchors.rs`、`transcript_selection`、`pager`、`view_scroll`、`Ctrl+D` 详情级别循环、Ctrl+P/U/F/N/X/I 等锚点与块导航。缺少的是**语义分组**、**三档详细度的统一策略**，以及把"分页不切断组"作为行模型的硬约束。

### 4.5 会话标签

OpenCode 的标签栏（`component/session-tabs.tsx`、`context/session-tabs-model.ts`）不只是切换器：

- 每个标签有忙碌动画、未读标记、注意力提示、重命名状态；
- 标题过长时尾部渐隐，再超长则跑马灯；
- 溢出时折叠为轨道控件；支持拖拽/按键移动、右键菜单、中键关闭、长按关闭防误触；
- 支持最近关闭重开、搜索、按序号快速切换。

当前项目的会话标签由 `crates/app/cli/wf-tui/src/session_tabs.rs` 与 `crates/app/tui/tui-components/src/tab_bar.rs` 实现，具备创建、切换、关闭、移动、固定、按序号选择、LRU 上限与持久化（`tab_persistence.rs`），但缺少未读/忙碌/重命名/右键菜单/最近关闭。

### 4.6 对话框

OpenCode 的 dialog 是一个**栈式统一外壳**（`ui/dialog.tsx`）：

- 尺寸分中/大/特大（宽 60 / 88 / 116），可居中或按比例下沉；
- 背板半透明，点击背板关闭；若存在文本选择则不关闭，以支持复制；
- 对话框打开时压入 modal 模式，Esc / Ctrl+C 关闭；若存在文本选择则先清选择；若输入框有内容则先清空；
- 页脚 Action 分组（次要在左、主要 Action 在右），Action 可引用命令标识自动取快捷键；
- 选择器（`ui/dialog-select.tsx`）统一支持过滤、分组、建议项、空态与多选。

命令面板（`component/command-palette.tsx`）就是"命令列表 + 设置项"的合并选择器：命令项来自注册表，设置项以 `setting:` 前缀注入，未输入过滤词时先展示"建议"分组。

当前项目的模态由 `ModalStack` 与专用覆盖层（历史、搜索、会话列表、侧栏、问题、审批、面板）共同承担，没有统一尺寸/页脚/背板行为，也没有把设置项并入命令面板。

### 4.7 全局系统面

OpenCode 在 Shell 层还提供：Toast 队列、MCP 失败与需认证提示、服务重连遮罩（首次 5 秒、后续 1 秒退避）、启动加载、配置迁移覆盖层、更新提示、注意力提示音、终端标题切换、终端挂起、主题切换与明暗模式锁定、鼠标选中复制策略（自动复制/手动复制）。

当前项目有 `footer` 的通知 TTL、spinner、动画与主题模块，但没有 Toast 队列、重连遮罩、启动加载、更新提示、终端标题与挂起、复制策略配置。

### 4.8 轻量模式

OpenCode 的 `packages/tui/src/mini/*` 是一套独立实现的轻量界面：页脚变体（prompt / menu / form / permission / subagent / command）、滚动缓冲表面与写入器、流式片段（文本/图片/子代理/传输）、详细度控制。它与主界面共享数据与命令语义，但渲染更简单。

当前项目对应 `crates/app/cli/wf-mini`。两者定位接近，但 `wf-mini` 目前只覆盖基础会话，未复用主 TUI 的命令注册表与分组规则。

## 5. 当前项目现状盘点

| 能力域 | 当前实现位置 | 状态 |
| --- | --- | --- |
| 屏幕栈与导航 | `wf-tui/src/screens.rs`、`tui/key_routes.rs` | 完整 |
| 会话转录 | `wf-tui/src/interactive.rs`、`render.rs`、`logical_rows` | 强，缺语义分组 |
| 工具块/子代理块 | `interactive/tool_blocks.rs`、`subagent_blocks.rs` | 完整 |
| 锚点与块跳转 | `interactive/anchors.rs`、`keys.rs` | 强于参考项目 |
| 转录选择与复制 | `interactive/selection.rs`、`copy.rs` | 完整 |
| 分页器 | `wf-tui/src/pager.rs` | 完整 |
| 键位内核 | `tui-core/src/keymap.rs` | 扁平，缺命令模型 |
| 键位用户覆盖 | `wf-tui/src/tui/keymap_config.rs` | 已有，可迁移 |
| 命令面板 | `tui-components/src/panels/command.rs` | 12 项，无搜索/分类 |
| slash 命令 | `interactive/panels.rs` 的 `handle_command_line` | 固定 12 项，无别名/参数 |
| 输入区 | `tui-components/src/footer.rs`、`interactive/keys.rs` | 单层面板路由 |
| 侧栏 | `tui-components/src/context_sidebar.rs`、`fetch/sidebar.rs` | 固定五段，无优先级 |
| 审批 | `tui-components/src/approval_overlay.rs` | 参数预览，无 diff |
| 提问 | `interactive/keys.rs`、question overlay | 基础完整 |
| 会话标签 | `wf-tui/src/session_tabs.rs`、`tab_bar.rs` | 基础完整 |
| 主题与动效 | `tui-style/*` | 完整 |
| 轻量 TUI | `crates/app/cli/wf-mini` | 基础完整 |
| 对话框 | `ModalStack` + 专用覆盖层 | 分散，不统一 |

## 6. 与 OpenCode 的对比矩阵

| 能力 | OpenCode | 当前项目 | 判断 |
| --- | --- | --- | --- |
| 命令定义 | 单表约 250 项，含标题/分组/描述 | 动作枚举 + 面板常量 | 结构性缺口 |
| 键位分层 | mode 栈 + layer 优先级 + 条件启用 | 扁平上下文 + 全局回退 | 结构性缺口 |
| 命令三态 | palette / slash / binding 同一项 | 三者分离 | 结构性缺口 |
| 命令面板 | 全局可搜索，含设置项与建议 | 输入 `/` 触发，固定列表 | 缺口 |
| slash 命令 | 名称/别名/参数/补全/服务端命令 | 固定命令集 | 缺口 |
| leader / which-key | 有 | 无 | 缺口 |
| 帮助与页脚提示 | 由注册表派生 | 硬编码 | 缺口 |
| 右侧 pane | panel > sidebar > terminal > auto | 单一覆盖层侧栏 | 缺口 |
| 侧栏内容 | 插件槽组合 | 固定五段 | 部分缺口 |
| 输入区 tab | subagents / shell / terminals | 无 | 缺口 |
| 投递语义 | steer / queue 两种显式策略 | 运行中自动排队，无显式策略 | 部分缺口 |
| 草稿暂存 | stash pop/list | 无 | 缺口 |
| 提交健壮性 | 防重入 + 乐观创建 + 失败恢复草稿 | 运行中自动排队，无失败恢复 | 部分缺口 |
| 图片附件 | 粘贴队列 + 预览 | 无输入侧附件能力 | 缺口 |
| 编辑器上下文 | 合成为消息 | 仅外置编辑草稿 | 缺口 |
| shell 模式 | `!` 前缀 | 无 | 缺口 |
| 权限 diff 预览 | 分屏/统一 | 无 | 缺口 |
| 拒绝理由 | 有 | 无 | 缺口 |
| 权限全屏 | 有 | 无 | 缺口 |
| 语义分组 | 四类组 + 三档详细度 | 逻辑行 + 详情循环 | 部分缺口 |
| 分页边界 | 保证不切断组 | 未约束 | 缺口 |
| 消息导航/时间线 | 有 | 锚点 + Ctrl+I | 已具备 |
| 会话标签增强 | 未读/忙碌/重命名/最近关闭 | 基础标签 | 部分缺口 |
| 对话框外壳 | 统一栈 + Action 页脚 | 分散 | 缺口 |
| Toast / 重连 / 更新 | 有 | 无或简化 | 缺口 |
| 主题 | 30+ 套 + 模式锁 | 主题模块，套数待确认 | 部分缺口 |
| 可观测性/回放 | 无 | 逻辑行、回放、调试视图 | 当前项目领先 |
| checkpoint 集成 | 无 | 完整 | 当前项目领先 |

## 7. 需要补充的功能

### 7.1 必做（结构性，其余能力依赖它）

**统一命令注册表。** 引入一条命令描述包含：稳定标识、标题、分组、描述、默认键位列表、slash 名称与别名、参数说明、是否进入命令面板、启用条件、执行入口。所有展示面（命令面板、slash 补全、帮助、页脚提示、which-key）从注册表派生。执行入口仍调用现有的动作与控制器方法，不改变已有业务实现。

**键位分层与模式栈。** 在现有 `Keymap` 之上增加层与模式：基础层、输入层、面板层、模态层、权限层、提问层；层可声明优先级与启用条件；按下 leader 后进入待决序列，超时（默认 2 秒）取消。保留现有 TOML 覆盖格式，将其解析结果映射到新的命令标识上，避免用户配置失效。

**命令面板升级。** 提供全局可搜索面板（`ctrl+p`），支持分类、建议项、设置项入口；输入框内 `/` 复用同一数据源做前缀过滤，二者不再各自维护列表。选中后统一执行并记录使用频次。

**slash 命令体系。** 支持别名、参数解析、参数补全、帮助文案与未知命令提示；区分纯前端命令与需要服务端的命令，后者在无对应接口时明确降级为只读或提示不可用。

**会话右侧 pane 模型。** 明确"面板 > 侧栏 > 终端 > 自动侧栏"的优先级，支持左右焦点切换；窄屏时侧栏改为覆盖层；自动侧栏按终端宽度阈值启用。侧栏区块改为可组合的数据段，保留现有五段作为默认组合。

**权限交互增强。** 对编辑类工具渲染补丁 diff（先提供统一视图，再补分屏）；外部目录类列出匹配模式；其余显示命令与参数。增加拒绝理由输入与全屏模式。会话级"记住决定"保留。

**转录语义分组与详细度。** 在现有逻辑行之上增加分组层：活动、推理、探索、指令四类；探索类工具集合可配置；详细度三档控制推理与探索正文的展开；分页与滚动补齐时以组为最小单位，不允许把组切断。

### 7.2 应做（体验完整性）

- **输入区多 tab**：子代理、shell、终端三个 tab，左右切换，Esc 关闭；终端 tab 与右侧终端 pane 共享会话。
- **投递语义**：提交区分立即插入与排队，排队有优先级；排队面板支持编辑、删除、重新排序。
- **草稿暂存**：支持暂存、弹出、列表管理，切换会话或新建会话时不清空。
- **提交健壮性**：防重入、乐观创建会话、失败时把原文恢复到输入框并给出可重试提示。
- **会话标签增强**：未读标记、忙碌动画、重命名、右键菜单、最近关闭重开、长标题渐隐与跑马灯、溢出折叠。
- **统一对话框外壳**：尺寸三档、居中选项、背板点击关闭、选择优先、Action 页脚左右分组、Esc/Ctrl+C 关闭顺序（先清选择、再清输入、最后关闭）。
- **会话命令补齐**：压缩上下文、撤销/重做、分叉、时间线、重命名、diff、状态。其中撤销/重做与分叉优先映射到现有 checkpoint 能力，不新增独立状态机制。
- **系统状态面**：Toast 队列、MCP 状态提示、服务重连遮罩、启动加载、更新提示。
- **图片附件与编辑器上下文**：粘贴队列、附件预览、编辑器选中内容合成为消息。

### 7.3 可选（打磨）

- 多主题切换与明暗模式锁定；主题资源独立于代码。
- 终端标题、终端挂起、注意力提示音。
- 复制策略配置（选中即复制 / 手动复制）。
- 轻量 TUI 复用命令注册表与分组规则，保证两种形态的命令语义一致。

## 8. 目标交互模型

### 8.1 命令模型

命令是唯一事实来源，键位、slash、面板、帮助都是它的视图。命令按作用域划分：

- 应用级：退出、帮助、命令面板、主题、设置、状态、重连诊断；
- 导航级：屏幕切换、返回、搜索、历史；
- 会话级：新建、切换、重命名、关闭、恢复、分叉、压缩、撤销/重做、时间线；
- 输入级：提交、排队、暂存、清空、历史、外置编辑、提及、附件；
- 交互级：审批、提问、面板选择、翻页。

每个命令声明自己的触发方式集合。同一能力只允许有一个执行入口，其余入口做转发。

### 8.2 会话工作区布局

主列固定为转录区 + 输入区，右侧 pane 按优先级占用剩余宽度。焦点是一个显式状态（左/右），由命令切换；覆盖层与模态不参与焦点竞争，只压栈。窄屏退化为单列，侧栏与终端以覆盖层呈现。

### 8.3 转录投影

数据流分四步：原始事件 → 语义组 → 逻辑行 → 可视行。分组是纯函数，输入是消息与工具事件序列，输出是组树；投影按稳定 key 增量更新；渲染只取可视窗口。详细度作为投影参数，而不是渲染时的临时开关。权限阻塞条目在投影阶段单独分区，避免等待态打乱阅读顺序。

### 8.4 输入区状态机

输入区有明确的模式：普通编辑、slash 补全、提及补全、shell 命令、搜索。模式决定按键归属与页脚提示，退出模式时恢复草稿。提交是单飞操作，排队与暂存是提交之外的旁路，不阻塞编辑。

### 8.5 对话框与覆盖层

模态统一走 dialog 栈：声明尺寸、是否居中、页脚 Action、关闭回调。覆盖层（历史、搜索、会话列表）作为"轻量模态"复用同一关闭语义与背板行为，避免每个覆盖层各写一套按键处理。

## 9. 模块落点与依赖约束

按现有四层 TUI 结构放置，保持 DAG 不变：

- `tui-core`：命令描述与注册表（纯数据）、键位层与模式栈解析、分组规则、转录投影规则、pane 状态模型、dialog 栈状态。不依赖渲染。
- `tui-components`：命令面板、对话框外壳、which-key、pane 容器、侧栏区块、输入区多 tab、diff 预览、审批与提问视图。只依赖 `tui-core`、`tui-style`、`tui-markdown`。
- `tui-render`：分组行与 pane 的绘制、可视窗口裁剪。
- `wf-tui`：命令执行入口与控制器接线、数据拉取、会话标签、配置与键位覆盖迁移、系统状态面。允许依赖 `wf-api`。
- `wf-cli-shared` / `wf-api`：需要服务端的会话命令（压缩、撤销/重做、分叉、工作树）在此暴露接口；TUI 只调用，不直接访问存储。

依赖方向不变：`tui-core` 不依赖 `wf-api` 之外的业务层；`tui-components` 不依赖 `wf-tui`；终端 pane 若需要 PTY，复用 `wf-shell` / `wf-sandbox` 能力，不新增底层实现。

## 10. 实施顺序与验收

### 10.1 第一阶段：命令与键位内核

范围：命令注册表、键位层与模式栈、leader 与待决序列、命令面板升级、slash 体系、帮助与页脚提示派生。

验收：

- 新增一条命令只需修改注册表一处，命令面板、slash 补全、帮助、页脚提示同时生效；
- 现有 TOML 键位覆盖仍然可用，未覆盖项回落到新的默认定义；
- 未知 slash 命令给出明确提示与最接近的候选；
- 命令面板可在任意屏幕打开并搜索，设置项可直达；
- 键位解析与冲突检测有单元测试，leader 超时行为可测。

### 10.2 第二阶段：会话工作区

范围：右侧 pane 优先级与焦点切换、自动侧栏阈值、侧栏区块化、输入区多 tab、shell 模式、权限 diff 预览与拒绝理由、统一 dialog 外壳。

验收：

- 面板、侧栏、终端按优先级互斥显示，左右焦点可用命令切换；
- 终端宽度低于阈值时侧栏自动隐藏，窄屏以覆盖层呈现；
- 权限请求对编辑类显示 diff，拒绝可填理由，全屏可开关；
- 输入区 tab 可左右切换，Esc 关闭并回到主输入；
- 所有模态遵循同一关闭顺序，文本选择不会被误关闭；
- 120x40 与 80x24 两种尺寸下布局无重叠、无截断。

### 10.3 第三阶段：转录语义与会话命令

范围：分组与详细度、分页边界约束、以 checkpoint 为基础的撤销/重做与分叉、时间线、压缩、重命名。

验收：

- 连续同类条目合并为组，三档详细度可切换且状态可持久化；
- 分页与滚动补齐不切断组边界；
- 撤销/重做走 checkpoint 恢复路径，不引入第二套状态；
- 分叉生成的新会话继承指定检查点，原会话不受影响；
- 大会话滚动时只重建变化行，帧时间不随历史长度线性增长。

### 10.4 第四阶段：系统面与轻量模式

范围：Toast 队列、MCP 状态、重连遮罩、启动加载、主题切换与模式锁、终端标题与挂起、复制策略、轻量 TUI 复用命令与分组。

验收：

- 通知可排队、可过期、可手动关闭，不覆盖输入区；
- 断连时显示遮罩与重试状态，恢复后自动消失；
- 主题切换即时生效，模式锁定后跟随终端明暗；
- 轻量模式与主界面执行同一命令时行为一致。

## 11. 非目标与风险

非目标：

- 不引入 JavaScript 插件运行时与插件槽机制；当前项目的扩展点放在 Rust 侧。
- 不照搬 OpenCode 的 provider 连接、配对、Go 升级等产品化命令。
- 不为参考项目已有的每个命令都补实现；只补与本项目业务对应的部分。
- 不改变现有 checkpoint、文件历史、执行可观测性的数据语义。

风险与应对：

- **键位兼容**：已有用户覆盖必须继续生效。迁移时以"动作到命令"的映射表做一次性转换，并在解析失败时回落到内置默认，同时给出提示。
- **分层复杂度**：模式栈与层优先级容易在多个入口之间产生歧义。约束是同一时刻只允许一个输入型模式处于栈顶，其余层只能声明启用条件，不得抢占按键。
- **性能**：分组与投影若每次全量重算会拖慢大会话。必须按稳定 key 增量更新，并用现有回放与调试视图做回归。
- **范围膨胀**：会话命令（分叉、压缩、撤销）涉及服务端语义，先做只读或降级版本，待接口就绪再补全。
- **终端 pane**：需要 PTY 生命周期管理，涉及 `wf-shell` 的现有约束，先做"只读输出 + 输入转发"的最小版本。

## 12. 验证方式

- 编译与静态检查：`cargo clippy --all-targets --all-features`。
- 键位与命令：注册表一致性测试（每个命令至少一种触发方式、每个键位至多一个生效命令）、TOML 覆盖兼容测试、leader 超时测试。
- 渲染：使用现有回放与调试视图录制固定事件序列，校验分组边界、可视窗口裁剪、pane 宽度分配。
- 布局：在 80x24、120x40、200x60 三种尺寸下检查输入区、侧栏、终端、对话框不重叠且页脚提示完整。
- 交互：审批、提问、模态关闭顺序、选择优先等行为用控制器级测试覆盖，避免只靠人工验证。

## 13. 附：OpenCode 关键文件索引

| 文件 | 作用 |
| --- | --- |
| `packages/tui/src/config/keybind.ts` | 键位定义表与解析 |
| `packages/tui/src/context/keymap.tsx` | 键位层、模式栈、leader、聚合钩子 |
| `packages/tui/src/app.tsx` | Provider 装配、命令注册、路由与全局覆盖层 |
| `packages/tui/src/routes/home.tsx` | 首页布局与居中输入 |
| `packages/tui/src/routes/session/index.tsx` | 会话主视图 |
| `packages/tui/src/component/session-frame.tsx` | 右侧 pane 优先级与焦点 |
| `packages/tui/src/routes/session/sidebar.tsx` | 侧栏容器与插件槽 |
| `packages/tui/src/component/prompt/index.tsx` | 输入区、提交、队列、暂存、slash |
| `packages/tui/src/routes/session/composer/*` | 输入区多 tab |
| `packages/tui/src/routes/session/permission.tsx` | 权限与拒绝理由 |
| `packages/tui/src/routes/session/grouping/*` | 语义分组规则 |
| `packages/tui/src/routes/session/rows.ts` | 行投影与分页边界 |
| `packages/tui/src/component/session-tabs.tsx` | 标签栏交互 |
| `packages/tui/src/ui/dialog.tsx` | 对话框栈与页脚 Action |
| `packages/tui/src/ui/dialog-select.tsx` | 统一选择器 |
| `packages/tui/src/component/command-palette.tsx` | 命令面板与设置项合并 |
| `packages/tui/src/feature-plugins/sidebar/*` | 侧栏区块实现 |
| `packages/tui/src/mini/*` | 轻量模式实现 |
