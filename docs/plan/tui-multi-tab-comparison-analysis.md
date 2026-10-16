# TUI 多标签页功能对比分析：wf-agent vs OpenCode

## 1. 概述

本文档对比分析当前项目（wf-agent）的 TUI 多标签页实现与 `ref/opencode` 目录中的实现，评估当前设计的合理性，并提出改进建议。

---

## 2. 架构对比

### 2.1 当前项目（wf-agent）：单进程多会话标签页

当前项目采用**单进程多会话标签页**架构，在一个 TUI 进程内管理多个并发的交互式聊天会话。

**核心层次：**

| 层次 | 文件 | 职责 |
|------|------|------|
| 纯数据层 | `crates/app/tui/tui-core/src/screen_data.rs` | `ChatTabMeta`、`ChatTabs`、`MAX_CHAT_TABS` |
| 组件层 | `crates/app/tui/tui-components/src/tab_bar.rs` | 标签栏渲染（`draw_session_tab_bar`、`format_tabs`） |
| 活跃管理层 | `crates/app/cli/wf-tui/src/session_tabs.rs` | `SessionTabs` 管理器，持有 `InteractiveController` |
| Shell 集成 | `crates/app/cli/wf-tui/src/tui.rs` | `TuiApp` 持有 `sessions: SessionTabs` |
| 按键拦截 | `crates/app/cli/wf-tui/src/tui/key_routes.rs` | Tab 快捷键拦截与分发 |

**关键数据结构：**

```rust
// crates/app/tui/tui-core/src screen_data.rs:23-31
pub struct ChatTabMeta {
    pub id: String,
    pub title: String,
    pub last_active_ms: u64,
}

// crates/app/cli/wf-tui/src/session_tabs.rs:18-23
pub struct SessionTab {
    pub controller: InteractiveController,
    pub holder: SessionHolder,
    pub title: String,
    pub last_active_ms: u64,
}
```

**标签页生命周期：**
- 创建：`push()` — 新标签页成为活跃，其他标签页的 holder 被分离
- 切换：`switch()` — 更新活跃索引，分离前一个 holder，重新附加新 holder
- 删除：`remove()` — 移除标签页，调整活跃索引
- 淘汰：LRU 策略，当达到 9 个标签页上限时淘汰最久未活跃的非活跃标签页

### 2.2 OpenCode：单路由 + 快速切换槽位

OpenCode 采用**单活跃路由 + 快速切换槽位**模型，不显示传统标签页，而是通过键盘驱动的槽位系统快速切换会话。

**核心文件：**

| 文件 | 职责 |
|------|------|
| `ref/opencode/packages/tui/src/context/route.tsx` | 路由状态定义与导航 |
| `ref/opencode/packages/tui/src/context/local.tsx` | 会话固定、快速切换槽位 |
| `ref/opencode/packages/tui/src/app.tsx` | 主应用 Shell、命令面板 |
| `ref/opencode/packages/tui/src/routes/session/index.tsx` | 会话对话视图 |

**关键机制：**

```typescript
// ref/opencode/packages/tui/src/context/local.tsx:452-455
const slots = createMemo(() => {
  const existing = new Set(sync.data.session.filter((x) => x.parentID === undefined).map((x) => x.id))
  return sessionStore.pinned.filter((id) => existing.has(id)).slice(0, 9)
})

// ref/opencode/packages/tui/src/context/local.tsx:494-499
quickSwitch(slot: number) {
  const target = slots()[slot - 1]
  if (!target) return
  if (route.data.type === "session" && route.data.sessionID === target) return
  route.navigate({ type: "session", sessionID: target })
}
```

**会话切换方式：**
1. 快速切换槽位（数字键 1-9）
2. 会话列表对话框（`session.list` 命令）
3. 子会话导航（`session.child.first/next/previous/parent`）

---

## 3. 设计合理性分析

### 3.1 当前设计的优势

#### 3.1.1 视觉概览能力
当前项目的标签栏提供多会话的清晰视觉概览，用户可以一目了然地看到所有打开的会话及其状态（流式传输指示器）。

- 标签栏渲染：`crates/app/tui/tui-components/src/tab_bar.rs:131-175`
- 流式标记：`crates/app/tui/tui-components/src/tab_bar.rs:88`（`STREAMING_MARKER`）
- 活跃标签高亮：`crates/app/tui/tui-components/src/tab_bar.rs:151-158`

#### 3.1.2 资源隔离
每个标签页拥有独立的 `InteractiveController` 和 `SessionHolder`，实现清晰的资源隔离。

- `SessionTab` 结构：`crates/app/cli/wf-tui/src/session_tabs.rs:18-23`
- Holder 附加/分离：`crates/app/cli/wf-tui/src/session_tabs.rs:117-143`

#### 3.1.3 分层清晰
纯数据层（`ChatTabs`）与活跃管理层（`SessionTabs`）分离良好，符合项目的模块化架构原则。

- 纯数据层：`crates/app/tui/tui-core/src/screen_data.rs`
- 活跃层：`crates/app/cli/wf-tui/src/session_tabs.rs`

#### 3.1.4 后台流式传输
后台标签页保持流式传输，用户切换回标签页时可以看到最新内容。

- 后台流式检测：`crates/app/cli/wf-tui/src/session_tabs.rs:109-114`
- Holder 分离但控制器保持活跃：`crates/app/cli/wf-tui/src/session_tabs.rs:194-199`

#### 3.1.5 单标签兼容
单标签模式下不显示标签栏，保持像素完美的向后兼容。

- 条件渲染：`crates/app/cli/wf-tui/src/tui.rs` 中的 `draw_inner()` 仅在 `sessions.len() > 1` 时渲染标签栏

### 3.2 当前设计的不足

#### 3.2.1 无持久化机制
标签页状态在进程重启后完全丢失。OpenCode 的固定槽位会持久化到 `session.json`。

- OpenCode 持久化：`ref/opencode/packages/tui/src/context/local.tsx:425-434`
- 当前项目：无对应实现

#### 3.2.2 无会话浏览器
无法浏览所有可用会话。OpenCode 提供 `session.list` 对话框用于浏览和切换会话。

- OpenCode 会话列表：`ref/opencode/packages/tui/src/app.tsx:571-580`
- 当前项目：无对应实现

#### 3.2.3 无子代理导航
无法在父子会话间导航。OpenCode 提供 `session.child.first/next/previous/parent` 命令。

- OpenCode 子会话导航：`ref/opencode/packages/tui/src/routes/session/index.tsx:432-456`
- 当前项目：无对应实现

#### 3.2.4 无固定/取消固定概念
标签页是临时的，无法固定重要会话以便快速访问。OpenCode 的 `togglePin` 机制允许用户固定会话。

- OpenCode 固定机制：`ref/opencode/packages/tui/src/context/local.tsx:484-493`
- 当前项目：无对应实现

#### 3.2.5 无事件驱动的会话删除处理
当会话在外部被删除时，标签页可能变为孤立状态。OpenCode 通过 `session.deleted` 事件自动清理。

- OpenCode 事件处理：`ref/opencode/packages/tui/src/context/local.tsx:469-471`
- 当前项目：无对应实现

#### 3.2.6 无插件槽位集成
标签页系统未与插件系统集成。OpenCode 使用 `pluginRuntime.Slot` 提供扩展点。

- Open Code 插件槽位：`ref/opencode/packages/tui/src/app.tsx:1122-1127`
- 当前项目：无对应实现

#### 3.2.7 无工作区/项目上下文
标签页不携带工作区上下文信息，无法按项目分组。OpenCode 的会话与工作区关联。

- Open Code 工作区上下文：`ref/opencode/packages/tui/src/routes/session/sidebar.tsx`
- 当前项目：无对应实现

#### 3.2.8 无会话元数据
标签页仅存储标题和活跃时间，缺少丰富的元数据（如模型、代理、工作区等）。

- 当前 `ChatTabMeta`：`crates/app/tui/tui-core/src/screen_data.rs:23-27`
- OpenCode 会话元数据：`ref/opencode/packages/tui/src/routes/session/sidebar.tsx`

#### 3.2.9 无拖拽重排
无法通过拖拽重新排列标签页顺序。

#### 3.2.10 无标签页分组
无法按项目/工作区分组标签页。

---

## 4. 改进建议

### 4.1 高优先级改进

#### 4.1.1 会话持久化与恢复
**目标**：实现标签页状态的持久化，支持重启后恢复。

**参考**：OpenCode 的 `session.json` 持久化机制（`ref/opencode/packages/tui/src/context/local.tsx:425-434`）

**建议**：
- 在 `ChatTabMeta` 中增加 `session_id` 字段
- 实现标签页状态的序列化/反序列化
- 在 TUI 启动时恢复上次的标签页布局

#### 4.1.2 会话浏览器
**目标**：提供会话列表对话框，支持浏览所有可用会话。

**参考**：OpenCode 的 `session.list` 命令（`ref/opencode/packages/tui/src/app.tsx:571-580`）

**建议**：
- 新增 `SessionListOverlay` 覆盖层
- 实现会话搜索和过滤
- 支持从列表中选择并切换到会话

#### 4.1.3 固定/取消固定机制
**目标**：允许用户固定重要会话，防止被 LRU 策略淘汰。

**参考**：OpenCode 的 `togglePin` 机制（`ref/opencode/packages/tui/src/context/local.tsx:484-493`）

**建议**：
- 在 `ChatTabMeta` 中增加 `pinned: bool` 字段
- 修改 `eviction_victim()` 跳过固定标签页
- 新增固定/取消固定的快捷键

#### 4.1.4 事件驱动的会话删除处理
**目标**：当会话在外部被删除时，自动清理对应标签页。

**参考**：OpenCode 的 `session.deleted` 事件处理（`ref/opencode/packages/tui/src/context/local.tsx:469-471`）

**建议**：
- 监听会话删除事件
- 自动移除对应的标签页
- 如果删除的是活跃标签页，切换到相邻标签页

### 4.2 中优先级改进

#### 4.2.1 子代理导航
**目标**：支持在父子会话间导航。

**参考**：OpenCode 的子会话导航（`ref/opencode/packages/tui/src/routes/session/index.tsx:432-456`）

**建议**：
- 在 `ChatTabMeta` 中增加 `parent_id: Option<String>` 字段
- 实现 `child_first()`、`child_next()`、`child_prev()`、`parent()` 导航方法
- 新增对应的快捷键

#### 4.2.2 会话元数据扩展
**目标**：丰富标签页的元数据显示。

**参考**：OpenCode 的会话侧边栏（`ref/opencode/packages/tui/src/routes/session/sidebar.tsx`）

**建议**：
- 在 `ChatTabMeta` 中增加 `model`、`agent`、`workspace` 字段
- 在标签栏中显示更多元数据
- 新增会话详情侧边栏

#### 4.2.3 工作区/项目上下文
**目标**：支持按工作区分组标签页。

**参考**：OpenCode 的工作区上下文（`ref/opencode/packages/tui/src/routes/session/sidebar.tsx`）

**建议**：
- 在 `ChatTabMeta` 中增加 `workspace_id: Option<String>` 字段
- 实现按工作区分组的标签栏渲染
- 支持工作区切换时过滤标签页

### 4.3 低优先级改进

#### 4.3.1 拖拽重排
**目标**：支持通过拖拽重新排列标签页顺序。

**建议**：
- 实现鼠标拖拽检测
- 支持标签页位置交换
- 更新 `active` 索引

#### 4.3.2 标签页分组
**目标**：支持按项目/工作区分组标签页。

**建议**：
- 实现分组渲染
- 支持分组折叠/展开
- 新增分组管理快捷键

#### 4.3.3 插件槽位集成
**目标**：将标签页系统与插件系统集成。

**参考**：OpenCode 的 `pluginRuntime.Slot`（`ref/opencode/packages/tui/src/app.tsx:1122-1127`）

**建议**：
- 定义标签页相关的插件槽位
- 支持插件自定义标签栏渲染
- 支持插件监听标签页事件

---

## 5. 总结

### 5.1 当前设计评价

当前项目的多标签页设计在**视觉概览**、**资源隔离**和**分层架构**方面表现良好，特别是：
- 标签栏提供清晰的多会话视觉概览
- 每个标签页拥有独立的控制器和资源管理器
- 纯数据层与活跃管理层分离清晰

### 5.2 主要差距

与 OpenCode 相比，当前项目在以下方面存在明显差距：
1. **持久化**：无标签页状态持久化
2. **会话发现**：无会话浏览器
3. **固定机制**：无固定/取消固定概念
4. **事件驱动**：无会话删除的自动清理
5. **子代理导航**：无父子会话导航
6. **元数据**：标签页元数据较为简单

### 5.3 改进优先级

建议按以下优先级实施改进：
1. **高优先级**：会话持久化、会话浏览器、固定机制、事件驱动删除
2. **中优先级**：子代理导航、元数据扩展、工作区上下文
3. **低优先级**：拖拽重排、标签页分组、插件槽位集成

---

## 6. 参考资料

### 6.1 当前项目关键文件

| 文件 | 行号 | 内容 |
|------|------|------|
| `crates/app/tui/tui-core/src/screen_data.rs` | 23-31 | `ChatTabMeta` 定义 |
| `crates/app/tui/tui-core/src/screen_data.rs` | 33-45 | `ChatTabs` 定义 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 18-23 | `SessionTab` 定义 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 56-59 | `SessionTabs` 定义 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 117-127 | `push()` 方法 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 130-144 | `switch()` 方法 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 164-181 | `remove()` 方法 |
| `crates/app/cli/wf-tui/src/session_tabs.rs` | 185-192 | `eviction_victim()` 方法 |
| `crates/app/tui/tui-components/src/tab_bar.rs` | 12-24 | `format_tabs()` 函数 |
| `crates/app/tui/tui-components/src/tab_bar.rs` | 92-101 | `session_tab_cells()` 函数 |
| `crates/app/tui/tui-components/src/tab_bar.rs` | 131-175 | `draw_session_tab_bar()` 函数 |
| `crates/app/cli/wf-tui/src/tui/key_routes.rs` | 84-124 | Tab 按键拦截 |
| `crates/app/cli/wf-tui/src/tui.rs` | 59-60 | `DASHBOARD_ENTRIES` 定义 |

### 6.2 OpenCode 关键文件

| 文件 | 行号 | 内容 |
|------|------|------|
| `ref/opencode/packages/tui/src/context/route.tsx` | 6-23 | `Route` 类型定义 |
| `ref/opencode/packages/tui/src/context/route.tsx` | 37-39 | `navigate()` 方法 |
| `ref/opencode/packages/tui/src/context/local.tsx` | 411-501 | `createSession()` 函数 |
| `ref/opencode/packages/tui/src/context/local.tsx` | 452-455 | `slots` 计算属性 |
| `ref/opencode/packages/tui/src/context/local.tsx` | 484-493 | `togglePin()` 方法 |
| `ref/opencode/packages/tui/src/context/local.tsx` | 494-499 | `quickSwitch()` 方法 |
| `ref/opencode/packages/tui/src/app.tsx` | 92-104 | 快速切换命令注册 |
| `ref/opencode/packages/tui/src/app.tsx` | 571-580 | `session.list` 命令 |
| `ref/opencode/packages/tui/src/app.tsx` | 620-628 | 快速切换命令定义 |
| `ref/opencode/packages/tui/src/app.tsx` | 1110-1128 | 应用 Shell 路由渲染 |
| `ref/opencode/packages/tui/src/routes/session/index.tsx` | 432-456 | 子会话导航 |
