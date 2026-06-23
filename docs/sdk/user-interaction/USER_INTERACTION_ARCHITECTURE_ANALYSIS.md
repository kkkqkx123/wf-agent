# 用户交互节点架构分析与改进建议

## 问题陈述与澄清

### 用户交互节点的真实职责

用户交互节点的主要目的是**让用户能够介入工作流执行过程**：

1. **变量更新场景**：计划阶段结束时需要用户批准，更新工作流变量，供 ROUTE 节点判断是否进入下一阶段
2. **消息添加场景**：LLM 通过用户交互节点询问用户问题，将用户反馈添加到 LLM 消息数组中继续对话

### 当前实现的缺陷

1. **配置包含应用层细节**：`showMessage`、`userInput` 等是应用层关注的，SDK 不应关心
2. **缺乏业务语义**：配置没有定义用户交互的结果如何被使用（更新哪个变量？如何格式化消息？）
3. **处理逻辑模糊**：不清楚 SDK 应该如何处理用户交互的结果
4. **双重处理**：
   - SDK 层：USER_INTERACTION → 转换为提示词 → 交给 LLM（错误的）
   - 应用层：另有独立的 UserInteractionStrategy 处理实际交互（与 SDK 脱节）

---

## 1. 当前架构分析

### 1.1 SDK 层实现（当前有缺陷）

```
SDK 层流程：
NodeExecutionCoordinator.executeNodeLogic()
  ├─ isLLMManagedNode(USER_INTERACTION) → true
  ├─ executeLLMManagedNode()
  │   ├─ extractLLMRequestData()
  │   │   └─ transformUserInteractionNodeConfig()
  │   │       └─ 转换配置为文本提示词（无实际交互逻辑）
  │   └─ llmCoordinator.executeLLM()
  │       └─ 向 LLM 发送提示词（不合理）
  └─ 返回 NodeExecutionResult
```

**问题**：
- 提示词: `"Ask for approval: xxx"` 被发送给 LLM
- LLM 返回一个文本响应，而非真正的用户交互结果
- 完全不涉及前端或用户

### 1.2 应用层实现（实际处理）

```
应用层流程：
NodeExecutionHandler
  ├─ 检查 node type
  ├─ node.type === USER_INTERACTION
  ├─ UserInteractionStrategy.execute()
  │   └─ InteractionEngine.handleUserInteraction()
  │       └─ UserInteractionHandler
  │           ├─ 触发前端交互事件
  │           ├─ 等待用户响应
  │           └─ 返回用户输入结果
  └─ 返回结果给工作流
```

**问题**：
- 这是应用层的**单独实现**，与 SDK 层无关
- SDK 层不知道应用层如何处理用户交互
- 造成两层架构混乱

### 1.3 事件系统现状

SDK 定义的事件类型（`sdk/types/events.ts`）中**没有用户交互相关的事件**：

```typescript
// 现有事件类型（不完整列表）
enum EventType {
  NODE_STARTED,
  NODE_COMPLETED,
  NODE_FAILED,
  TOOL_CALL_STARTED,        // ← 工具调用事件
  TOOL_CALL_COMPLETED,       // ← 工具调用事件
  TOOL_CALL_FAILED,          // ← 工具调用事件
  // 缺少用户交互事件
}
```

缺少的事件：
- `USER_INTERACTION_REQUEST` ：请求用户交互
- `USER_INTERACTION_RESPONSE` ：用户交互完成
- `USER_INTERACTION_TIMEOUT` ：用户交互超时
- `USER_INTERACTION_FAILED` ：用户交互失败

---

## 2. 根本问题分析

### 2.1 架构问题

| 层级 | 问题 |
|---|---|
| **SDK 层** | 将用户交互当作 LLM 托管节点，转换为文本提示词，无实际逻辑 |
| **应用层** | 独立实现用户交互处理，与 SDK 层脱节 |
| **接口** | SDK 与应用层之间缺乏明确的用户交互接口定义 |
| **事件系统** | 缺少用户交互相关的事件类型 |

### 2.2 设计缺陷

#### 缺陷 1：错误的抽象
```typescript
// 当前的错误抽象
UserInteractionNodeConfig → 文本提示词 → LLM 处理

// 正确的抽象应该是
UserInteractionNodeConfig → 用户交互请求 → 应用层处理
```

#### 缺陷 2：无有意义的 SDK 实现
```typescript
// 现在的 transformUserInteractionNodeConfig（无意义）
export function transformUserInteractionNodeConfig(config: UserInteractionNodeConfig): LLMExecutionRequestData {
  let prompt: string;
  switch (config.userInteractionType) {
    case 'ask_for_approval':
      prompt = `Ask for approval: ${config.showMessage || 'Please approve this action'}`;
      break;
    // ... 其他类型
  }
  return {
    prompt,
    profileId: 'default',
    parameters: {}
  };
}

// 这个转换毫无意义：
// - 没有真正的交互逻辑
// - prompt 只是描述性文本
// - LLM 无法真正执行用户交互
```

#### 缺陷 3：双重处理
```
SDK 处理 USER_INTERACTION（错误的方式）
   ↓
应用层再次处理 USER_INTERACTION（正确的方式）
   
结果：两层架构混乱，职责不清
```

---

## 3. 改进方案

### 3.1 核心思想

**SDK 定义业务语义，应用层处理实现细节**

```
SDK 层（业务语义）
  ├─ 定义操作类型：UPDATE_VARIABLES、ADD_MESSAGE
  ├─ 定义变量更新规范：变量名、表达式
  ├─ 定义消息规范：添加到 LLM 的消息格式
  └─ 定义事件系统

应用层（实现细节）
  ├─ 实现 UserInteractionHandler 接口
  ├─ 负责获取用户输入（前端交互）
  └─ 返回原始用户数据给 SDK
  
SDK 执行
  ├─ 解析用户数据为变量值或消息
  ├─ 更新工作流变量
  └─ 添加消息到对话历史
```

### 3.0 接口设计原则

应该定义多层接口确保 SDK 和应用层的正确性：

1. **节点配置接口** (`UserInteractionNodeConfig`)：定义 SDK 需要什么业务信息（不包含应用层细节）
2. **处理器接口** (`UserInteractionHandler`)：定义应用层实现的契约（获取用户输入）
3. **上下文接口** (`UserInteractionContext`)：定义传递给处理器的执行上下文

### 3.2 改进步骤

#### 步骤 1：定义用户交互接口体系

**位置**: `sdk/types/interaction.ts` (新建) 和 `sdk/types/node.ts` (修改)

应定义以下关键接口：

**1. 节点配置接口** - 修改 UserInteractionNodeConfig（SDK 业务语义）

应改为抽象的操作规范，不涉及应用层细节：

- **operationType**：操作类型
  - `UPDATE_VARIABLES`：更新工作流变量
  - `ADD_MESSAGE`：添加用户消息到 LLM 对话
  
- **变量更新操作**（当 operationType = UPDATE_VARIABLES）
  - `variables`：变量更新列表
    - `variableName`：要更新的变量名
    - `expression`：变量更新表达式（可能包含用户输入的占位符）
    - `scope`：变量作用域（global、thread、subgraph、loop）

- **消息添加操作**（当 operationType = ADD_MESSAGE）
  - `message`：用户消息的定义
    - `role`：'user'（固定）
    - `contentTemplate`：消息内容模板（可能包含用户输入的占位符）

- **公共配置**
  - `prompt`：给用户的提示信息（应用层用于显示）
  - `timeout`：交互超时时间（毫秒）
  - `metadata`：额外的业务信息（应用层可用）

**2. 处理器接口** - UserInteractionHandler（应用层实现）

- 职责：获取用户输入，返回原始数据
- 入参：交互请求（包含 prompt、timeout 等）
- 出参：用户输入（字符串、JSON 等）
- 不关心 SDK 如何使用这个输入

**3. 上下文接口** - UserInteractionContext（SDK 提供）

- 线程信息：threadId、workflowId、nodeId
- 变量访问：getVariable、setVariable、getVariables
- 消息管理：conversationManager（用于添加消息）
- 超时控制：timeout、cancelToken
- 事件系统：eventManager（用于触发交互事件）

**关键设计原则**：

✓ **SDK 不知道应用层如何获取用户输入**（可能是前端弹窗、命令行、或其他）  
✓ **应用层不知道 SDK 如何使用用户输入**（可能更新变量或添加消息）  
✓ **通过接口和数据格式解耦**：约定用户输入的格式和 SDK 如何解析  
✓ **配置中没有应用层细节**：没有 UI 相关的 showMessage、userInput 等  

#### 步骤 2：添加用户交互事件和处理流程

**位置**: `sdk/types/events.ts`

定义用户交互的完整生命周期事件：

- **USER_INTERACTION_REQUESTED**：SDK 请求用户交互
  - 包含：nodeId、interactionId、operationType、prompt、timeout
  
- **USER_INTERACTION_RESPONDED**：应用层返回用户输入
  - 包含：interactionId、inputData、timestamp

- **USER_INTERACTION_PROCESSED**：SDK 完成结果处理
  - 包含：interactionId、operationType、results（更新的变量或添加的消息）

- **USER_INTERACTION_FAILED**：交互失败或超时
  - 包含：interactionId、reason、timestamp

**执行流程**：

```
SDK: 创建交互请求 (operationType, variables配置 或 message模板)
  ↓
事件: USER_INTERACTION_REQUESTED (nodeId, prompt, timeout, ...)
  ↓
应用层处理器: 获取用户输入 (通过前端弹窗、CLI 等)
  ↓
事件: USER_INTERACTION_RESPONDED (inputData)
  ↓
SDK: 解析输入，根据 operationType 处理
  ├─ UPDATE_VARIABLES: 更新工作流变量
  └─ ADD_MESSAGE: 添加消息到 conversationManager
  ↓
事件: USER_INTERACTION_PROCESSED (results)
  ↓
工作流继续执行
```

#### 步骤 3：修改 SDK 层的 USER_INTERACTION 处理

**位置**: `sdk/core/execution/coordinators/node-execution-coordinator.ts`

改进要点：
- USER_INTERACTION 不再被当作 LLM 托管节点
- 从 `isLLMManagedNode()` 中移除 USER_INTERACTION
- 单独处理，触发请求事件，等待应用层处理

#### 步骤 4：实现 UserInteractionCoordinator

**新建**: `sdk/core/execution/coordinators/user-interaction-coordinator.ts`

核心职责：
- 解析节点配置，创建交互请求
- 触发 USER_INTERACTION_REQUESTED 事件
- 调用应用层处理器获取用户输入
- 根据 operationType 处理用户输入：
  - UPDATE_VARIABLES：更新工作流变量
  - ADD_MESSAGE：添加消息到 conversationManager
- 触发 USER_INTERACTION_PROCESSED 或 FAILED 事件

#### 步骤 5：应用层实现 UserInteractionHandler

**位置**: `application/services/interaction/interaction-handler.ts`

应用层处理器职责：
- 接收交互请求（包含 prompt、timeout）
- 通过前端弹窗、CLI 等方式获取用户输入
- 返回原始用户数据
- 不关心 SDK 如何处理这个数据

实现重点：
- 超时控制和异常处理
- 与前端通信
- 用户输入的验证和序列化

---

## 4. 改进对比

### 4.1 核心改进

| 方面 | 改进前 | 改进后 |
|---|---|---|
| **节点配置** | 包含应用层细节（showMessage、userInput） | 纯业务语义（operationType、variables、message） |
| **SDK 职责** | 将输入转换为 LLM 提示词 | 定义操作规范，处理用户输入，更新变量/消息 |
| **应用层职责** | 独立实现，与 SDK 无关 | 实现 UserInteractionHandler 获取用户输入 |
| **接口定义** | 无清晰接口，配置混乱 | 3层接口：NodeConfig、Handler、Context |
| **处理逻辑** | 与 LLM 混淆 | 清晰分离：请求→处理→执行→继续 |
| **可扩展性** | 低，需要修改 SDK | 高，应用层灵活实现 |

### 4.2 配置改进示例

**改进前**：
```typescript
// 混淆了应用层细节
{
  userInteractionType: 'ask_for_approval',
  showMessage: '是否批准？'，  // 应用层关注
  userInput: undefined          // 应用层填充
}
```

**改进后**：
```typescript
// 纯业务语义
{
  operationType: 'UPDATE_VARIABLES',
  variables: [
    {
      variableName: 'approved',
      expression: '{{input}}',  // 用户输入占位符
      scope: 'thread'
    }
  ],
  prompt: '是否批准？',          // 应用层用于显示
  timeout: 5000
}

// 或消息场景
{
  operationType: 'ADD_MESSAGE',
  message: {
    role: 'user',
    contentTemplate: '{{input}}'  // 用户输入占位符
  },
  prompt: '请输入您的问题：',
  timeout: 30000
}
```

### 4.3 处理流程对比

**改进前**：
```
NODE_EXECUTION
  ├─ USER_INTERACTION 被识别为 LLM 托管节点
  ├─ 转换为提示词发送给 LLM
  └─ LLM 返回文本（无实际交互）

应用层（完全独立）
  └─ UserInteractionStrategy 单独处理真正的交互
```

**改进后**：
```
NODE_EXECUTION
  ├─ 解析配置，确定 operationType
  ├─ 触发 USER_INTERACTION_REQUESTED 事件
  ├─ 调用 UserInteractionHandler.handle()（应用层实现）
  ├─ 接收用户输入
  ├─ 处理用户输入（更新变量 或 添加消息）
  ├─ 触发 USER_INTERACTION_PROCESSED 事件
  └─ 工作流继续执行
```

---

## 5. 实施计划

### 第一阶段：定义接口体系

- 创建 `sdk/types/interaction.ts`：UserInteractionHandler、UserInteractionContext
- 修改 `sdk/types/node.ts`：UserInteractionNodeConfig（改为业务语义）
- 修改 `sdk/types/events.ts`：添加 4 个事件类型

### 第二阶段：SDK 层改造

- 创建 `sdk/core/execution/coordinators/user-interaction-coordinator.ts`
- 修改 `node-execution-coordinator.ts`：分离 USER_INTERACTION 处理
- 修改 `llm-request-operations.ts`：移除 USER_INTERACTION（不再是 LLM 托管节点）
- 修改 `config-utils.ts`：不再转换为 LLM 提示词

### 第三阶段：应用层实现

- 实现 `UserInteractionHandler` 接口
- 创建处理器注入机制
- 与前端通信模块集成

### 第四阶段：测试验证

- 单元测试：UserInteractionCoordinator
- 集成测试：完整的交互流程
- 端到端测试：两种场景（变量更新、消息添加）

---

## 6. 好处

**架构清晰**
- ✓ SDK 不知道应用层实现细节，应用层不知道 SDK 如何使用输入
- ✓ 通过接口和事件完全解耦
- ✓ 配置中没有应用层细节

**业务正确**
- ✓ 变量更新场景：用户批准→更新变量→ROUTE 判断下一步
- ✓ 消息添加场景：用户回答→添加到对话→LLM 继续对话
- ✓ 不再混淆 LLM 模块

**易于扩展**
- ✓ 新的交互操作类型：无需修改 SDK，定义新的 operationType 即可
- ✓ 新的应用层实现：完全独立，实现 Handler 接口即可
- ✓ 与不同的前端集成：通过处理器注入灵活切换

**更易测试**
- ✓ Handler 可独立 mock
- ✓ 协调器逻辑清晰，易于单元测试
- ✓ 事件流完整，便于集成测试

---

## 7. 总结

用户交互节点的改进方向是**从混淆的实现回归清晰的设计**：

**现状问题**：
- 配置混淆了应用层细节
- 执行逻辑与 LLM 模块混淆
- SDK 与应用层职责不清

**改进核心**：
- 定义清晰的接口体系（3层接口）
- 定义业务语义的配置（operationType + variables/message）
- 定义完整的事件系统（请求→处理→执行→继续）

**最终效果**：
- SDK 层清晰定义了用户交互的语义和流程
- 应用层专注于获取用户输入
- 二者通过接口和事件完全解耦
