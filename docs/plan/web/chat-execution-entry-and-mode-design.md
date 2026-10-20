# Chat 执行入口与 Chat 模式重构设计

## 1. 背景

当前 chat 首页 Composer 同时暴露 Agent、Workflow、Model、Mode 四类控制项。它们在 UI 上并列，但语义维度并不相同：

- Agent 控制执行策略、工具集、提示词和迭代预算。
- Workflow 是可执行图，当前却被作为上下文摘要注入 chat。
- Model 控制 LLM 推理模型。
- Mode 中的 Chat 仍然是 Agent Loop 的弱化版，而不是独立执行实例。

这种设计导致两个问题：

1. 入口层开始承担“运行配置组装”职责，而组合本应发生在模板、图、工具层。
2. Chat 被建模为“无工具、单轮的 agent loop”，保留了无业务意义的 iteration 概念。

## 2. 设计目标

1. 将 chat 首页入口收敛为“执行类型选择”，而不是运行时配置组装。
2. 将 Chat 建模为独立执行实例：纯多轮问答，无工具、无 iteration、无 checkpoint。
3. 保持 Model 作为独立维度，不与执行类型合并。
4. 将 Workflow 的图结构管理与执行留在 workflow/template 界面，不通过 chat Composer 注入上下文。
5. 通过 pinned / recently used / full panel 机制控制可运行实例过多的问题。
6. 保证会话可追溯：记录执行类型、agent/workflow/model 等上下文。

## 3. 核心概念

### 3.1 执行类型

chat 首页主入口只回答一个问题：这条消息交给哪种执行器？

- Agent：完整 agent loop，支持工具、迭代、checkpoint、审批。
- Workflow：图执行器，由 workflow/template 界面管理与触发。
- Chat：纯多轮问答执行器，无工具、无 iteration、无 checkpoint。

执行类型之间不是互斥的“类别选项”与“运行时参数”的混合，而是执行器类型。内部子调用通过图结构或工具调用创建，不在入口组装。

### 3.2 Model 维度

Model 独立于执行类型。一个 Agent 可以配不同 Model，一个 Model 也可以服务不同执行类型。因此 Model 保持独立选择器，不合并到执行类型入口。

### 3.3 Workflow 边界

Workflow 是可执行图，不是 chat 上下文。其节点、分支、模板应在 workflow/template 界面管理。chat 入口不应注入 workflow 摘要作为上下文，也不应在 Composer 中组装 workflow。若需要引用 workflow 结果，应作为明确的引用/附件能力，而不是隐式上下文。

### 3.4 Chat 语义

Chat 是独立执行实例，不是 Agent Loop 的 mode。它只做多轮问答：

- 无工具执行
- 无 iteration
- 无 checkpoint
- 无审批交互
- 保留对话历史和 token usage

iteration 与批量任务分发属于 Agent 或其他执行实例的职责，不进入 Chat。

## 4. UI/UX 设计

### 4.1 Composer 布局

Composer 保留两类控制：

1. 执行类型入口：Agent / Workflow / Chat。
2. Model 选择器：独立维度。

不再暴露 Mode toggle。Agent 的 tools、iteration、checkpoint 等细节由 agent template 管理，不在 Composer 直接调整。

### 4.2 实例过多控制

所有选择器统一采用三层数据：

- Pinned / Favorites：用户显式顶置的实例。
- Recently used：最近使用过的少量实例。
- All：完整列表，通过顶部按钮打开浮窗/面板查看。

主触发器只显示 pinned + recent 的少量条目，避免列表过长。浮窗支持分类、搜索、分组和管理入口。

### 4.3 Workflow 入口定位

Workflow 入口不在 chat Composer 中作为上下文注入。Workflow 的管理与执行应在 workflow/template 界面完成。chat 页面只负责调用执行类型，不负责 workflow 图结构组装。

### 4.4 Chat 展示

Chat 执行时，transcript 应明确标识本轮为 Chat，并显示使用的 Model。Agent 执行时显示 Agent 标识；Workflow 执行时显示 Workflow 标识。

## 5. 后端设计

### 5.1 执行类型路由

后端应按执行类型分发，而不是通过 `mode` 字段弱化 Agent Loop：

- Agent：调用 agent loop 执行链路，保留 tools、iteration、checkpoint。
- Workflow：调用 workflow/graph 执行链路。
- Chat：调用 tool-free chat 执行链路，不进入 agent loop。

### 5.2 Chat 执行链路

Chat 使用 tool-free multi-turn primitive，例如 `ChatSession` 或等价链路：

- 每轮追加用户消息。
- 调用 LLM 生成。
- 记录 assistant 消息。
- 更新 token usage。
- 不执行工具。
- 不创建 checkpoint。
- 不发布 agent loop 事件。

### 5.3 Agent Loop 保持纯净

Agent Loop 只服务 Agent 执行类型。`apply_loop_mode` 中的 chat 分支应移除，不再通过 mode 弱化 agent loop。Agent 的工具、iteration、checkpoint 由 agent template 和配置决定。

### 5.4 Workflow 上下文注入移除

后端不再支持 chat Composer 传入 `workflow_id` 作为上下文注入。Workflow 执行应通过 workflow 执行链路完成，而不是在 agent loop 中追加 workflow 摘要消息。

## 6. 数据与追溯

会话记录应保存本轮执行类型及相关上下文：

- 执行类型：Agent / Workflow / Chat
- Agent id（Agent 执行时）
- Workflow id（Workflow 执行时）
- Model
- 必要的运行元数据

这样回放、审计和 UI 展示都能准确还原当时使用的执行器。

## 7. 迁移步骤

1. 移除 chat 的 iteration 概念，不再通过 `mode=chat` 弱化 agent loop。
2. 将 Chat 改为独立执行链路，使用 tool-free chat primitive。
3. 从 chat Composer 移除 WorkflowPicker，或重新定义为 workflow 结果引用。
4. 将 Agent / Workflow / Chat 作为主入口的执行类型。
5. 保留 Model 作为独立选择器。
6. 统一 Picker 基础设施，支持 pinned / recent / all + 浮窗。
7. 在 transcript 和 session 元数据中记录执行类型与 model。
8. 更新前端 streaming 与后端路由，区分 chat 与 agent 执行链路。

## 8. 风险与缓解

- Chat 独立链路需要新增或适配 streaming 能力；应先评估现有 LLM streaming 是否可复用。
- 移除 Workflow context 注入可能影响现有用户习惯；需要明确迁移说明。
- pinned / recent 需要持久化策略，避免每次刷新丢失。
- 执行类型与 Model 的 UI 分层需要清晰，避免用户误以为 model 是执行类型的一部分。

## 9. 非目标

- 不在 chat 入口组装 graph workflow。
- 不在 chat 入口暴露 agent 的工具/iteration 细节。
- 不将 Model 合并到执行类型入口。
- 不保留 chat 的 iteration 概念。