# TUI 多标签页会话设计

本文参考 `ref/opencode` 的 TUI 标签页实现，分析当前项目是否应补充类似的多标签页功能，以支持在一个 TUI 进程内同时打开多个交互式会话，通过共享底层基础设施（client、terminal、render pipeline）来避免多开 TUI 进程带来的额外开销。

## 0. 结论速览

- 当前 TUI 为单会话架构：`InteractiveController` 一次只驱动一个会话，`SessionHolder` 一次只持有一个 session id，用户必须退出当前会话才能切换到另一个。
- 多开 TUI 进程方案存在明显缺陷：每个进程独立持有终端控制（alt screen、raw mode）、独立的事件循环与渲染管线、独立的 client 连接，内存与 CPU 开销成倍增长。
- **建议补充标签页功能**：在一个 TUI 进程内支持多个并发交互式会话，共享 terminal 后端、client 连接和渲染管线，每个标签页拥有独立的滚动历史、输入状态和流式 reducer。
- 设计遵循现有架构模式：标签页状态归属 `tui-core` 的 screen/navigation 层，每标签页的会话状态归属 `InteractiveController`，共享基础设施通过 shell 层协调。

## 1. 现状分析

### 1.1 当前 TUI 架构

当前 TUI 采用分层多 crate 架构：

- `tui-terminal`：终端后端（crossterm）、能力探测、SIGINT 处理、stderr guard
- `tui-style`：主题、动画、motion
- `tui-markdown`：Markdown 解析与流式渲染
- `tui-core`：reducer（`SessionReducer`）、事件流、keymap、render model、screen data、status line
- `tui-components`：widget/overlay（footer、panel、question overlay 等）
- `tui-render`：帧渲染与屏幕绘制
- `tui-debug`：diff recorder 与调试视图

应用层：

- `wf-tui`：全屏 TUI 外壳，拥有 `AppState`、`InteractiveController`、事件循环与渲染循环
- `wf-mini`：轻量 crossterm TUI（独立行渲染内核，不消费 tui-core）
- `wf-headless`：无 TTY 的 headless CLI

### 1.2 当前会话管理方式

当前 TUI 的会话管理围绕以下几个核心结构：

- **`SessionHolder`**（`wf-tui/src/session_holder.rs`）：单会话生命周期管理器，状态机为 Empty → Held → Attached → Detached → Reconnecting。一次只持有一个 session id。
- **`InteractiveController`**（`wf-tui/src/interactive.rs`）：单个交互式会话的完整状态机，包含 `SessionReducer`、`Footer`、`scrollback`、流式状态、审批/问筹浮层、动画控制器、分页器等。一次只驱动一个会话。
- **`AppState`**（`wf-tui/src/state.rs`）：应用状态，包含 `ScreenState`（导航、数据缓存、filter）和 `NoticeQueue`。`ScreenKind::Interactive` 是特殊的自包含 screen，不走 `ScreenData` 管理屏数据通道。
- **`ScreenKind`**：枚举了 Dashboard、Workflow、Executions、Insights、Interactive、Checkpoints、Settings、Catalog、Git 九种 screen。Interactive 是唯一的会话型 screen。

### 1.3 当前架构的限制

1. **单会话约束**：`SessionHolder` 与 `InteractiveController` 均为单实例设计，一次只能有一个活跃会话。
2. **切换成本高**：从会话 A 切换到会话 B 需要：保存 A 的滚动位置与输入状态 → 关闭 A 的流式连接 → 加载 B 的历史记录 → 启动 B 的流式连接。过程中 A 的流式状态丢失。
3. **无法并发观察**：用户无法同时观察多个并发运行的 agent 会话（例如一个长任务和一个短任务并行）。
4. **多开进程开销大**：每个 TUI 进程独立持有终端控制（alt screen、raw mode）、独立的事件循环、独立的渲染管线、独立的 client 连接与运行时 task，内存与 CPU 开销成倍增长。

## 2. ref/opencode 标签页实现分析

### 2.1 TUI 侧：轻量快速切换

ref/opencode 的 TUI 侧采用轻量级"快速切换"机制，而非完整标签页：

- **9 个快速切换槽位**：用户可将常用会话固定（pin）到槽位，通过 `<leader>1` 到 `<leader>9` 快速切换。
- **固定列表持久化**：固定列表保存到 `session.json`，重启后恢复。
- **会话列表对话框**：`dialog-session-list.tsx` 提供完整会话列表，支持搜索与切换。

TUI 侧不维护多标签页的完整状态，而是依赖服务端的会话持久化 + 客户端的快速切换。每次切换时重新加载目标会话的历史与流式状态。

### 2.2 Web/Desktop 侧：完整标签页系统

Web/Desktop 侧实现了完整的标签页系统：

- **两种标签页类型**：`SessionTab`（会话标签页）和 `DraftTab`（草稿标签页）。
- **标签页管理 Context**：`TabsProvider` 管理标签页列表、最近使用、关闭历史、标签页信息（标题、目录）。
- **持久化存储**：标签页状态、最近使用、关闭历史都持久化到本地。
- **标签页条 UI**：支持拖拽排序、关闭、重新打开、预览弹出框。
- **快捷键**：`mod+1` 到 `mod+9` 切换标签页，`mod+t` 新建，`mod+w` 关闭。

### 2.3 资源共享方式

- **服务器连接共享**：多个标签页共享同一个服务器连接（`ServerConnection.Key`）。
- **数据同步**：`SyncContext` 管理所有会话的数据同步与缓存，标签页切换时从缓存读取。
- **状态隔离**：每个标签页有独立的 prompt 状态和 UI 状态（滚动位置、输入内容）。

### 2.4 对当前项目的启示

ref/opencode 的两层设计（TUI 轻量切换 + Web 完整标签页）给我们的启示：

1. TUI 侧不需要照搬 Web 的完整标签页系统（拖拽排序、预览弹出框等），因为 TUI 的交互空间有限。
2. TUI 侧的核心价值在于：**在一个进程内维护多个会话的独立状态，避免重复加载与进程开销**。
3. 资源共享的关键是：共享 client 连接与 terminal 后端，但保持每个会话的流式状态与 UI 状态独立。
4. 标签页切换应该是轻量的：只切换渲染目标与输入焦点，不重新加载历史记录。

## 3. 设计方案

### 3.1 设计目标

1. 在一个 TUI 进程内支持多个并发交互式会话。
2. 每个标签页拥有独立的滚动历史、输入状态、流式 reducer 和 footer 状态。
3. 共享 terminal 后端、client 连接、渲染管线和动画时钟。
4. 标签页切换是轻量的：只切换渲染目标与输入焦点，不重新加载历史记录。
5. 遵循现有架构模式：不引入"version 2"模块，在原模块上扩展。

### 3.2 标签页数据结构

标签页状态归属 `tui-core` 的 screen/navigation 层。在 `screen_data.rs` 中扩展：

- 新增 `TabState` 结构，包含标签页 ID、会话 ID、标题、最后活跃时间。
- 新增 `TabBarState` 结构，包含标签页列表、当前活跃标签页索引。
- `ScreenKind::Interactive` 从单例变为多例：每个标签页是一个独立的 Interactive 会话。

### 3.3 会话状态管理

每个标签页的会话状态由独立的 `InteractiveController` 实例管理：

- `InteractiveController` 的结构不变，每个实例仍然拥有独立的 `SessionReducer`、`Footer`、`scrollback`、流式状态等。
- 在 `wf-tui` 的 shell 层引入 `TabSessionManager`，管理多个 `InteractiveController` 实例：
  - `new_tab(client, execution_id)`：创建新标签页。
  - `switch_tab(index)`：切换到指定标签页。
  - `close_tab(index)`：关闭指定标签页。
  - `current_tab()`：获取当前活跃的 `InteractiveController`。
- 同一时刻只有一个标签页处于"活跃"状态（接收输入与渲染），但所有标签页的流式状态（`SessionReducer`、`turn_task`、`poll_task`）在后台继续运行。

### 3.4 资源共享

共享以下基础设施：

- **client 连接**：所有标签页共享同一个 `WfClient` 实例。
- **terminal 后端**：所有标签页共享同一个 terminal 后端（crossterm、raw mode、alt screen）。
- **渲染管线**：所有标签页共享同一个渲染管线，但只渲染当前活跃标签页的内容。
- **动画时钟**：所有标签页共享同一个动画时钟（`AnimationController` 的 tick 由 shell 统一驱动）。
- **keymap**：所有标签页共享同一套 keymap，但标签页切换键（如 `alt+1` 到 `alt+9`）由 shell 层拦截，不传递给 `InteractiveController`。

### 3.5 标签页切换流程

1. 用户按下标签页切换键（如 `alt+1`）。
2. shell 层拦截按键，不传递给当前 `InteractiveController`。
3. `TabSessionManager` 保存当前标签页的 UI 状态（滚动位置、输入内容）。
4. `TabSessionManager` 切换到目标标签页，恢复其 UI 状态。
5. shell 层的渲染循环从新的活跃 `InteractiveController` 获取数据并渲染。
6. 后台标签页的 `turn_task` 和 `poll_task` 继续运行，流式事件继续到达并更新其 `SessionReducer` 和 `scrollback`。

### 3.6 UI 设计

标签页 UI 采用顶部标签页条（tab bar）设计：

- 在现有 status line 上方新增一行标签页条。
- 每个标签页显示：标签页编号 + 会话标题（截断）+ 流式状态指示器（如 `⋯` 表示正在流式输出）。
- 当前活跃标签页高亮显示。
- 标签页数量达到上限（如 9 个）时，新标签页替换最久未活跃的标签页。

### 3.7 快捷键设计

- `alt+1` 到 `alt+9`：切换到指定标签页。
- `alt+n`：新建标签页（创建新会话）。
- `alt+w`：关闭当前标签页。
- `alt+tab` / `alt+shift+tab`：循环切换标签页。

### 3.8 与现有架构的关系

- **`tui-core`**：扩展 `screen_data.rs`，新增 `TabState`、`TabBarState` 等数据结构。不修改 `SessionReducer`、`Footer` 等已有结构。
- **`tui-components`**：新增 `TabBar` widget，渲染标签页条。复用现有的主题与样式系统。
- **`tui-render`**：渲染循环从 `TabSessionManager` 获取当前活跃标签页的 `InteractiveController` 数据并渲染。不修改渲染管线本身。
- **`wf-tui`**：扩展 `AppState`，引入 `TabSessionManager` 管理多个 `InteractiveController`。扩展事件循环，拦截标签页切换键。不修改 `InteractiveController` 的内部结构。

## 4. 实现步骤

### 4.1 第一阶段：数据结构与状态管理

1. 在 `tui-core/screen_data.rs` 中新增 `TabState`、`TabBarState` 数据结构。
2. 在 `wf-tui` 中新增 `TabSessionManager`，管理多个 `InteractiveController` 实例。
3. 扩展 `AppState`，集成 `TabSessionManager`。

### 4.2 第二阶段：标签页切换与资源共享

1. 实现标签页切换逻辑：保存/恢复 UI 状态，切换渲染目标。
2. 实现后台标签页的流式事件处理：`turn_task` 和 `poll_task` 继续运行。
3. 实现 client 连接与 terminal 后端的共享。

### 4.3 第三阶段：UI 与快捷键

1. 在 `tui-components` 中实现 `TabBar` widget。
2. 在 `tui-render` 中集成标签页条渲染。
3. 在 `wf-tui` 的 keymap 中注册标签页切换快捷键。

### 4.4 第四阶段：测试与优化

1. 单元测试：`TabSessionManager` 的创建、切换、关闭逻辑。
2. 集成测试：多标签页场景下的渲染与事件处理。
3. 性能测试：多标签页场景下的内存与 CPU 使用情况。

## 5. 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 多标签页场景下内存占用过高 | 限制标签页数量上限（如 9 个）；后台标签页的 scrollback 历史记录限制大小；非活跃标签页的动画帧率降低。 |
| 后台标签页的流式事件处理导致 CPU 占用过高 | 后台标签页的 `turn_task` 继续运行但渲染频率降低；使用 `SegmentedPacer` 限制后台标签页的流式帧率。 |
| 标签页切换时 UI 状态丢失 | 切换前保存完整的 UI 状态（滚动位置、输入内容、审批浮层状态）；切换后恢复。 |
| 与现有单会话架构的兼容性 | 标签页模式作为可选功能，保留单会话模式作为默认行为；标签页数量为 1 时行为与当前单会话模式一致。 |

## 6. 总结

本文分析了 ref/opencode 的 TUI 标签页实现，并结合当前项目的架构特点，提出了 TUI 多标签页会话的设计方案。该方案在一个 TUI 进程内支持多个并发交互式会话，通过共享 terminal 后端、client 连接和渲染管线来避免多开进程的开销，同时保持每个标签页的独立状态。设计方案遵循现有架构模式，在原模块上扩展，不引入并行模块，不做历史兼容。
