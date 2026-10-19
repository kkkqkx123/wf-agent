# 对话界面中的 Agent 与 Workflow 选择设计

## 1. 背景

当前对话界面仅支持模型选择（`ModelPicker`），后端 Agent Loop 运行已支持 `agent_id`，
且模板库已支持 agent/workflow 模板。但前端未提供 Agent/Workflow 选择入口，用户无法在对话中
切换智能体或工作流上下文。

本阶段仅给出设计，不执行代码修改。

## 2. 目标

1. 在 Composer 中提供 Agent 选择器（默认 main agent）。
2. 提供 Workflow 选择器（可选），用于将工作流作为对话上下文或触发执行。
3. 选择结果进入流式运行请求体，并在会话中可追溯。
4. 与模型选择、预设提示词、模板库保持一致的交互模式。

## 3. 现状与约束

- `streamLoopRun` 请求体目前仅包含 `model`、`message`、`conversation`。
- 后端 `RunAgentLoopBody` 已支持 `agent_id`，空值回退到默认 main agent。
- Workflow 有独立执行流（`streamWorkflowExecution`），与 Agent Loop 不同。
- 模板库 API 已支持 `kind = agent|workflow` 的浏览与详情。

## 4. UI/UX 参考分析

### 4.1 OpenCode 前端对话页面的 Agent 选择模式

OpenCode 的 `prompt-input.tsx` 提供了两种 Agent 入口：

1. **@mention 引用**：在输入文本中输入 `@agent-name` 引用 agent，作为 `AgentPart` 嵌入消息（`data-type=agent`）。Agent 列表来自 `controls.agents.available`，通过 `unifiedSearch` 通道解析。
2. **工具栏 Select**：在输入区上方工具栏放置紧凑的 `Select` 组件（`data-component="prompt-agent-control"`），与模型选择器并排。布局为 `[Agent Select] [Model Select] [input] [Send]`，与当前 wf-agent 的 `[ModelPicker] [textarea] [Send]` 同构。
3. **TooltipKeybind**：每个选择器带 tooltip 提示和键盘快捷键标签（`command.keybind("agent.cycle")`）。
4. **条件渲染**：仅当有可用 agent 时显示（`Show when={!agentsLoading()}`），避免空选择器干扰。
5. **Fade-in 动画**：选择器以 `animate-in fade-in duration-300` 淡入。
6. **Slash 命令集成**：agent 同时出现在 slash popover 目录中，支持 `/agent` 快捷方式。

### 4.2 ZCode 的通用选择器设计（ModelConfigSelect / V4ComposerToolbar）

ZCode 的 `ModelConfigSelect` 是成熟的通用下拉选择器，其设计模式可直接复用：

1. **受控 Dropdown 状态**：`open`/`onOpenChange` 由外部协调，通过 `activeConfigPicker` 实现排他打开（同一时间只有一个 picker 展开）。
2. **openRequestKey 模式**：通过递增 `openRequestKey` 请求打开菜单，快捷键触发时无需模拟 click，直接设置受控 open 状态。
3. **焦点恢复**：`focusSelectorOnClose` 指定 picker 关闭后焦点回到输入框，保持输入连续性。
4. **分组下拉**：使用 `DropdownMenuRadioGroup` + `DropdownMenuLabel` 按类别分组（如按 provider 分组），组间以分隔线区分。
5. **Footer 动作**：下拉底部提供管理入口（如"管理模型"），通过 `footerActions` 配置。
6. **键盘快捷键**：Ctrl+M 打开模型菜单、Ctrl+T 循环思考深度，快捷键标签动态读取生效命令表。
7. **Tooltip 集成**：选项支持 badge 标记、锁定状态 tooltip（如"运行中任务锁定"）。
8. **加载/错误/不可用状态**：完整的状态机——loading 时禁用、error 时显示重试按钮、unavailable 时显示原因。
9. **移动端适配**：响应式断点（`@max-sm/composer:size-7`），窄屏时 picker 折叠为纯图标按钮。
10. **Trigger Badge**：触发器可带小徽标（如"会话模型"、"不可用"），在触发器标签后显示。

### 4.3 当前 wf-agent ModelPicker 模式

当前 `ModelPicker.svelte` 的已有模式应保持并扩展：

- 弹出式面板 + 过滤输入框 + 自由输入回退。
- 列表项显示名称 + model/provider 次要行。
- 空状态提示与默认值标记。
- 键盘支持（Escape 关闭、Enter 提交输入值）。

### 4.4 可借鉴的设计要点汇总

| 模式 | 来源 | 应用场景 |
|------|------|----------|
| 排他打开协调（activeConfigPicker） | ZCode | 多个 picker 同时只展开一个 |
| 受控 open + openRequestKey | ZCode | 快捷键请求打开菜单 |
| 焦点恢复（focusSelectorOnClose） | ZCode | picker 关闭后焦点回到 textarea |
| @mention + Select 双模式 | OpenCode | agent 可在文本中引用或工具栏选择 |
| 分组下拉 + 分隔线 | ZCode | 按类别分组选项 |
| Footer 管理入口 | ZCode | 下拉底部提供模板管理跳转 |
| 键盘快捷键 + 动态标签 | 两者 | 快捷键提示跟随命令表 |
| Tooltip 描述 | 两者 | 选项描述信息 |
| 加载/错误/不可用状态机 | ZCode | 完整状态处理 |
| 移动端折叠为图标 | ZCode | 窄屏适配 |
| Badge/标记 | ZCode | 默认值、选中状态标记 |
| Fade-in 动画 | OpenCode | picker 淡入 |
| 条件渲染（仅有数据时显示） | OpenCode | 避免空选择器干扰 |

## 5. 设计方案

### 5.1 交互布局

在 Composer 的模型选择器旁新增两个紧凑选择器：

- `AgentPicker`：选择 agent 模板 id，默认 `@standard/main`，显示名称与描述 tooltip。
- `WorkflowPicker`：选择 workflow 模板 id，允许"无"选项；选择后作为当前对话上下文。

布局保持单行：`[AgentPicker] [WorkflowPicker] [ModelPicker] [textarea] [Send]`，
窄屏时折叠到次级行。

选择器统一采用受控 Dropdown 模式（参考 ZCode `ModelConfigSelect`）：

- 触发器为紧凑按钮，左侧图标 + 名称 + 下拉箭头，选中后显示名称。
- 下拉面板包含过滤输入框（支持搜索名称/id）+ 分组选项列表。
- 选项列表支持 badge 标记（如 "default"、"context"）。
- 面板底部提供 footer 动作（如 "管理模板"，跳转模板库页面）。
- 选项 hover 时显示 tooltip 描述。

### 5.2 状态协调

多个选择器共享排他打开状态（参考 ZCode `activeConfigPicker`）：

- Composer 维护一个 `activePicker` 状态（`'agent' | 'workflow' | 'model' | null`）。
- 任意选择器打开时关闭其他选择器，避免面板重叠。
- 切换会话时重置 `activePicker` 为 null。

### 5.3 焦点与键盘

- 选择器关闭后焦点恢复（参考 ZCode `focusSelectorOnClose`）：焦点回到 textarea。
- 键盘快捷键（参考 ZCode `openRequestKey` + OpenCode 快捷键标签）：
  - `Ctrl+Shift+A` 打开 AgentPicker。
  - `Ctrl+Shift+W` 打开 WorkflowPicker。
  - 快捷键触发时通过递增 `openRequestKey` 设置受控 open 状态，不模拟 click。
  - Tooltip 中显示快捷键标签（动态读取，不硬编码）。
- Escape 关闭面板并恢复焦点；Enter 提交过滤输入框中的自由文本（ModelPicker 已有模式）。

### 5.4 状态与持久化

- `ChatSessionController` 新增 `agentId` 与 `workflowId` 状态。
- 默认值从本地偏好/最近使用恢复；切换会话时按会话记录恢复。
- 新会话使用全局默认值。
- WorkflowPicker 允许 "无"（即不选择 workflow），此时不注入 workflow 上下文。

### 5.5 条件渲染与空状态

- 选择器仅在模板列表非空时显示（参考 OpenCode 条件渲染）。
- AgentPicker 无可用 agent 时隐藏，回退到默认 main agent。
- WorkflowPicker 无可用 workflow 时隐藏，回退到无 workflow 上下文。
- 模板列表加载中时选择器显示为禁用 + loading 指示。
- 加载失败时显示重试按钮（参考 ZCode error 状态）。

### 5.6 请求链路

- `LoopStreamBody` 增加 `agentId` 与 `workflowId`。
- `streamLoopRun` 将 `agent_id` 发送给 `/api/v1/agent-loops/{id}/stream`。
- `workflowId` 的处理分两种模式：
  1. 上下文模式：后端将选中 workflow 的摘要/节点信息注入 agent 上下文。
  2. 执行模式：直接调用 workflow 执行流，并在对话中插入执行结果。
- 初期建议先实现上下文模式，执行模式作为后续扩展。

### 5.7 后端配合

- Agent：复用现有 `agent_id` 解析与模板组合逻辑。
- Workflow：需要在 run params 中增加可选 `workflow_id`，并在 composition 阶段注入 workflow 摘要；
  或提供独立的 workflow-to-context 转换函数。
- 会话记录：Agent Loop 元数据中保存 `agent_id`/`workflow_id`，便于回放与审计。

### 5.8 预览与数据源

- Preview 客户端需要 agent/workflow 模板列表路由（已覆盖大部分）。
- 选择器空数据时降级为默认值，不阻断发送。

## 6. 迁移步骤

1. 前端新增 `AgentPicker`/`WorkflowPicker` 组件，采用受控 Dropdown + 排他打开 + 焦点恢复模式。
2. 实现 Composer 层 `activePicker` 排他协调与 `openRequestKey` 快捷键请求机制。
3. 扩展 `LoopStreamBody` 与 `streamLoopRun`，增加 `agentId` 与 `workflowId` 字段。
4. 后端 composition 支持可选 workflow 上下文注入。
5. 会话元数据与回放支持 agent/workflow 标识。
6. 配置模块允许设置默认 Agent/Workflow。

## 7. 风险与缓解

- Workflow 上下文可能过大，需要摘要与截断策略。
- Agent 模板与 workflow 的组合语义需明确，避免用户误解。
- 预览数据源需补齐模板详情路由，避免选择器报错。
- 多个选择器面板可能重叠，需通过排他打开状态协调。
- 快捷键冲突：`Ctrl+Shift+A` / `Ctrl+Shift+W` 需检查与浏览器/系统快捷键的冲突。
- 移动端窄屏下多个选择器可能挤压输入区，需折叠为图标或次级行。
- 模板列表加载失败时的降级体验需保证不阻断发送。