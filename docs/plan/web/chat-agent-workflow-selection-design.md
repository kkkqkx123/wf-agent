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

## 4. 设计方案

### 4.1 交互布局

在 Composer 的模型选择器旁新增两个紧凑选择器：

- `AgentPicker`：选择 agent 模板 id，默认 `@standard/main`，显示名称与描述 tooltip。
- `WorkflowPicker`：选择 workflow 模板 id，允许“无”选项；选择后作为当前对话上下文。

布局保持单行：`[AgentPicker] [WorkflowPicker] [ModelPicker] [textarea] [Send]`，
窄屏时折叠到次级行。

### 4.2 状态与持久化

- `ChatSessionController` 新增 `agentId` 与 `workflowId` 状态。
- 默认值从本地偏好/最近使用恢复；切换会话时按会话记录恢复。
- 新会话使用全局默认值。

### 4.3 请求链路

- `LoopStreamBody` 增加 `agentId` 与 `workflowId`。
- `streamLoopRun` 将 `agent_id` 发送给 `/api/v1/agent-loops/{id}/stream`。
- `workflowId` 的处理分两种模式：
  1. 上下文模式：后端将选中 workflow 的摘要/节点信息注入 agent 上下文。
  2. 执行模式：直接调用 workflow 执行流，并在对话中插入执行结果。
- 初期建议先实现上下文模式，执行模式作为后续扩展。

### 4.4 后端配合

- Agent：复用现有 `agent_id` 解析与模板组合逻辑。
- Workflow：需要在 run params 中增加可选 `workflow_id`，并在 composition 阶段注入 workflow 摘要；
  或提供独立的 workflow-to-context 转换函数。
- 会话记录：Agent Loop 元数据中保存 `agent_id`/`workflow_id`，便于回放与审计。

### 4.5 预览与数据源

- Preview 客户端需要 agent/workflow 模板列表路由（已覆盖大部分）。
- 选择器空数据时降级为默认值，不阻断发送。

## 5. 迁移步骤

1. 前端新增 `AgentPicker`/`WorkflowPicker` 与服务调用。
2. 扩展 `LoopStreamBody` 与 `streamLoopRun`。
3. 后端 composition 支持可选 workflow 上下文。
4. 会话元数据与回放支持 agent/workflow 标识。
5. 配置模块允许设置默认 Agent/Workflow。

## 6. 风险

- Workflow 上下文可能过大，需要摘要与截断策略。
- Agent 模板与 workflow 的组合语义需明确，避免用户误解。
- 预览数据源需补齐模板详情路由，避免选择器报错。