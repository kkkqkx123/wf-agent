# opencode TUI 实现位置与输入/输出隔离机制分析

> 分析对象：`ref/opencode` 快照（sst/opencode 仓库）。
> 分析目的：为 wf-cli mini 模式的输入/输出隔离改造提供参考基准。
> 结论速览：**ref 快照中并不存在 `--mini` 参数**；opencode 的交互形态只有默认 TUI（`opencode`）与 headless `run`。用户期望的"类似 opencode --mini"，本质是 opencode 默认 TUI 会话界面的简洁形态——其输入与输出在同一渲染树内通过 flex 布局物理隔离，这是本次对比的核心机制。

---

## 一、重要事实澄清：opencode 没有 `--mini`

对 `ref/opencode/packages` 全量检索 `mini` 关键字，结果均为无关词（`minimum`、`plugin runs`、`MIN_SAFE_INTEGER` 等）：

| 检索位置 | 结果 |
| :--- | :--- |
| `packages/tui/src` | 无 `mini` 匹配 |
| `packages/cli/src` | 无 `mini` 匹配 |
| `packages/opencode/src` | 仅 `minimum` / `runs` / `MIN_SAFE_INTEGER` 等无关匹配 |
| `packages/cli/src/commands` | 无 `--mini` 标志 |

opencode CLI 的实际形态：

| 形态 | 触发 | 渲染 |
| :--- | :--- | :--- |
| 完整 TUI | `opencode`（默认） | 全屏渲染，多路由（home / session），面板、对话框 |
| headless | `opencode run "prompt"` | stdout 流式输出，无界面 |

因此本次对比的参考基准取 opencode **默认 TUI 的会话（session）路由**——它是"输入框 + 输出区同屏隔离"的直接实现，也是 `--mini` 期望形态的源头。

---

## 二、opencode TUI 实现位置整理

### 2.1 入口链

| 路径 | 作用 |
| :--- | :--- |
| `packages/cli/src/index.ts` | CLI 入口，`Runtime.run(Commands, Handlers, ...)` 分发子命令 |
| `packages/cli/src/commands/*` | 子命令处理器（默认命令进入 TUI，`run` 进入 headless） |
| `packages/cli/src/tui.ts` | `runTui()`：组装 `@opencode-ai/tui` 的 `run()` + `TuiConfig`，提供传输层与插件宿主 |
| `packages/tui/src/index.tsx` | TUI 包导出 `{ run, TuiInput }` |
| `packages/tui/src/app.tsx` | **TUI 主程序**（1134 行）：`render()` 挂载根组件、renderer 生命周期（acquire/release）、主题探测、路由导航、keymap 注册、SIGINT/SIGTSTP 处理 |

### 2.2 渲染底座（自研 OpenTUI，非 ratatui）

- 渲染引擎：`@opentui/core` + `@opentui/solid`（SolidJS 风格的响应式 JSX → `BoxRenderable` / `ScrollBoxRenderable` / `TextareaRenderable` 组件树 → 逐帧 diff 输出）。
- 全屏渲染：`app.tsx` 中 `render()` 挂载根组件，渲染器独占整个终端屏幕（非 inline viewport 拼合）。
- 布局：flex 布局（`flexDirection` / `flexGrow` / `flexShrink` / `minHeight`），与 Web 端一致的盒子模型。

### 2.3 会话界面（核心参考对象）

**`packages/tui/src/routes/session/index.tsx`**（2706 行）——session 路由主组件 `Session()`：

```
<box flexDirection="row" flexGrow={1} minHeight={0}>          ← 主容器：内容列 + 可选 Sidebar
  <box flexGrow={1} minHeight={0} paddingBottom={1} ... gap={1}>
    <scrollbox flexGrow={1} stickyScroll stickyStart="bottom" ← 输出区：占满全部剩余高度
              verticalScrollbarOptions ...>
      <box height={1} />                                      ← 顶部留白
      <For messages()> UserMessage / AssistantMessage ...     ← 消息列表（只读渲染）
    </scrollbox>
    <box flexShrink={0}>                                      ← 底部固定区：绝不参与滚动
      <PermissionPrompt />  /  <QuestionPrompt />  /  <SubagentFooter />
      <pluginRuntime.Slot name="session_prompt">
        <Prompt onSubmit={toBottom} ... />                    ← 输入框（见 2.4）
      </pluginRuntime.Slot>
    </box>
    <Toast />
  </box>
  <Show sidebarVisible()> <Sidebar /> </Show>                 ← 侧栏（可选）
</box>
```

关键点：

1. **输出区是一个 `scrollbox`**（`flexGrow={1}`）：滚动发生在 scrollbox **内部**，`stickyScroll` + `stickyStart="bottom"` 使新内容到达时自动吸附底部。
2. **输入区是独立的 `flexShrink={0}` 盒子**：位于输出 scrollbox 之后、同一列布局内，高度只由内容决定，**永远不会被输出顶走或卷入滚动**。
3. 输出（scrollbox）与输入（prompt box）是**同一渲染树中的两个 flex 子节点**——由布局引擎按 flex 规则分配空间，滚动只发生在 scrollbox 内部，两者物理上不可能互相覆盖或混排。

### 2.4 输入框组件

**`packages/tui/src/component/prompt/index.tsx`**（1716 行）——`Prompt` 组件：

```
<box width="100%">                                           ← 锚点
  <box border={["left"]} borderColor={borderHighlight}       ← 左侧高亮边框（agent 色）
       customBorderChars={{ ...SplitBorder, bottomLeft: "╹" }}>
    <box paddingLeft=2 paddingRight=2 paddingTop=1 backgroundColor=backgroundElement>
      <textarea width="100%" minHeight={1} maxHeight={maxHeight()}   ← 输入主体
                placeholder={...} syntaxStyle={syntax()} ... />
      <box flexDirection="row" paddingTop={1} ...>            ← 底部元信息行
        <text>agent 名</text> <text>· 模型</text> <text>权限模式</text> ...
      </box>
    </box>
  </box>
</box>
```

关键点：

- 输入主体是 `textarea`（多行、`minHeight=1`、`maxHeight = max(6, rows/3)`），可随内容增高但**高度受限**，增高时向上撑开（输入区在底部，向下固定）。
- 输入框带独立视觉边界：左边框 + 背景色 + 底部元信息行（agent · model · permission mode），与输出区视觉上明确分离。
- `PromptRef` 暴露 `set()/reset()/submit()/focus()`，供外部（如 undo、timeline 对话框）注入草稿。

### 2.5 消息渲染

- `UserMessage`（index.tsx L1364）：用户消息块。
- `AssistantMessage`（L1469）：助手消息，内部按 part 类型分发：
  - `TextPart`（L1686）：markdown 渲染；
  - `ToolPart` / `GenericTool` / `InlineToolRow` / `BlockTool` / `Shell` / `Write` / `Read` / `Grep` / `WebFetch` / `WebSearch` / `Task` / `Edit` / `ApplyPatch` / `TodoWrite` / `Question` / `Skill` 等工具行渲染。
- 所有消息块都是 scrollbox 内的只读内容，不接收键盘焦点。

### 2.6 其他相关实现

| 路径 | 作用 |
| :--- | :--- |
| `packages/tui/src/routes/home/index.tsx` / `home.tsx` | home 路由（会话列表 / 新建），与隔离机制无关，略 |
| `packages/tui/src/routes/session/footer.tsx` | 底部状态条（目录、LSP/MCP/权限计数），位于最底部，独立于输入框 |
| `packages/tui/src/routes/session/sidebar.tsx` | 侧栏（宽屏内嵌 / 窄屏浮层 `position="absolute"` 覆盖） |
| `packages/tui/src/routes/session/permission.tsx` / `question.tsx` | 权限/提问浮层，渲染在输入区上方（同一 `flexShrink={0}` 块内），不混入输出 scrollbox |
| `packages/tui/src/ui/dialog.tsx` | 模态对话框（覆盖层），不参与正常文档流 |

---

## 三、opencode 输入/输出隔离机制总结

opencode 实现隔离的三条原则（供 wf-cli 对照）：

1. **单一渲染树 + 全屏所有权**：整个屏幕只有一个渲染器、一个组件树、一个布局引擎（flex）。输出区与输入区是同一棵树的两个分区，空间分配由布局引擎一次性完成，不存在"两套渲染路径拼合"。
2. **滚动局部化**：只有输出 scrollbox 内部滚动（`sticky bottom`），输入区 `flexShrink={0}` 固定在底部。滚动永远不触及输入区。
3. **输入区视觉自治**：输入框有独立边框、背景、元信息行，焦点状态（`TextareaRenderable`）与输出区无共享状态，光标只存在于输入区内。

对比 wf-cli mini（见 `docs/ref/wf-cli-mini-对比与改进建议.md`），后者正是违反了这三条原则：输出经 `insert_before` 直写终端 scrollback（第二套渲染路径）、滚动由终端整屏滚动承担（触及 footer）、输入框无独立视觉边界且光标坐标硬编码。
