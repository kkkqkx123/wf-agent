# USER_INTERACTION 节点处理逻辑分析

## 1. 概述

USER_INTERACTION 节点是 SDK 中用于暂停工作流执行、等待用户输入并根据用户响应更新状态或添加消息的节点类型。

### 1.1 核心能力

- **变量更新**: 根据用户输入更新工作流变量
- **消息添加**: 将用户输入作为消息添加到对话历史
- **超时控制**: 支持超时机制，超时后自动取消或返回默认值
- **取消机制**: 支持通过 CancelToken 取消交互

---

## 2. 类型定义

### 2.1 节点配置 (packages/types/src/node/configs/interaction-configs.ts)

```typescript
interface UserInteractionNodeConfig {
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  
  // operationType = UPDATE_VARIABLES 时必需
  variables?: Array<{
    variableName: string;
    expression: string;        // 支持 {{input}} 占位符
    scope: VariableScope;      // 'global' | 'thread' | 'subgraph' | 'loop'
  }>;
  
  // operationType = ADD_MESSAGE 时必需
  message?: {
    role: 'user';
    contentTemplate: string;  // 支持 {{input}} 占位符
  };
  
  prompt: string;              // 显示给用户的提示信息
  timeout?: number;            // 超时时间，默认 30000ms
  metadata?: Record<string, unknown>;
}
```

### 2.2 操作类型

```typescript
type UserInteractionOperationType =
  | 'UPDATE_VARIABLES'   // 更新工作流变量
  | 'ADD_MESSAGE'        // 添加用户消息到对话
  | 'TOOL_APPROVAL';     // 工具调用审批（LLM节点中使用）
```

### 2.3 交互上下文

```typescript
interface UserInteractionContext {
  threadId: ID;
  workflowId: ID;
  nodeId: ID;
  
  getVariable(variableName: string, scope?: VariableScope): unknown;
  setVariable(variableName: string, value: unknown, scope?: VariableScope): Promise<void>;
  getVariables(scope?: VariableScope): Record<string, unknown>;
  
  timeout: number;
  cancelToken: {
    cancelled: boolean;
    cancel(): void;
  };
}
```

---

## 3. 节点处理器流程

### 3.1 处理器入口 (sdk/graph/execution/handlers/node-handlers/user-interaction-handler.ts)

```
userInteractionHandler(thread, node, context)
         │
         ▼
┌─────────────────────┐
│ 1. 解析节点配置      │
│    - operationType   │
│    - variables       │
│    - message         │
│    - timeout         │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 2. 生成交互ID        │
│    generateId()      │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 3. 创建交互请求       │
│    createInteractionRequest()
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 4. 创建交互上下文     │
│    createInteractionContext()
│    - getVariable     │
│    - setVariable     │
│    - cancelToken     │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 5. 获取用户输入       │
│    getUserInput()    │
│    ┌─────────────┐  │
│    │ Promise.race │  │
│    │  - handler  │  │
│    │  - timeout  │  │
│    │  - cancel   │  │
│    └─────────────┘  │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 6. 处理用户输入       │
│    processUserInput()│
│    ┌─────────────┐  │
│    │ operationType│ │
│    │  - UPDATE   │ │
│    │  - ADD_MSG  │ │
│    └─────────────┘  │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 7. 返回执行结果      │
│    executionTime    │
└─────────────────────┘
```

### 3.2 核心函数详解

#### getUserInput() - 三向竞速机制

```typescript
async function getUserInput(request, context, handler) {
  // 超时控制
  const timeoutPromise = new Promise((_, reject) => {
    setTimeout(() => {
      reject(new Error(`User interaction timeout after ${request.timeout}ms`));
    }, request.timeout);
  });

  // 取消控制
  const cancelPromise = new Promise((_, reject) => {
    const checkCancel = setInterval(() => {
      if (context.cancelToken.cancelled) {
        clearInterval(checkCancel);
        reject(new Error("User interaction cancelled"));
      }
    }, 100);
  });

  try {
    // 竞速：用户输入、timeout、cancel
    return await Promise.race([
      handler.handle(request, context),  // 应用层实现
      timeoutPromise,
      cancelPromise
    ]);
  } finally {
    context.cancelToken.cancel();  // 清理取消检查
  }
}
```

#### replaceInputPlaceholder() - 占位符替换

```typescript
function replaceInputPlaceholder(template: string, inputData: unknown): string {
  if (typeof template !== "string") {
    return String(template);
  }
  // 将所有 {{input}} 替换为用户输入
  return template.replace(/\{\{input\}\}/g, String(inputData));
}
```

#### processVariableUpdate() - 变量更新

```typescript
async function processVariableUpdate(config, inputData, thread) {
  const results: Record<string, unknown> = {};
  
  for (const variableConfig of config.variables) {
    // 1. 替换占位符
    const expression = replaceInputPlaceholder(variableConfig.expression, inputData);
    
    // 2. 表达式求值（简化实现）
    const value = evaluateExpression(expression, inputData);
    
    // 3. 更新变量到线程作用域
    thread.variableScopes.thread[variableConfig.variableName] = value;
    
    results[variableConfig.variableName] = value;
  }
  
  return results;
}
```

---

## 4. 事件系统

### 4.1 事件类型 (sdk/core/utils/event/builders/interaction-events.ts)

| 事件类型 | 触发时机 | 参数 |
|---------|---------|------|
| USER_INTERACTION_REQUESTED | 开始等待用户输入 | interactionId, workflowId, nodeId, operationType |
| USER_INTERACTION_RESPONDED | 用户完成输入 | interactionId, inputData |
| USER_INTERACTION_PROCESSED | 处理完成 | interactionId, workflowId, results |
| USER_INTERACTION_FAILED | 处理失败 | interactionId, error |

### 4.2 事件订阅 (sdk/api/graph/resources/user-interaction/user-interaction-resource-api.ts)

```typescript
// 订阅用户交互请求
onInteractionRequested(listener: (event: UserInteractionRequestedEvent) => void);

// 订阅用户响应
onInteractionResponded(listener: (event: UserInteractionRespondedEvent) => void);

// 订阅处理完成
onInteractionProcessed(listener: (event: UserInteractionProcessedEvent) => void);

// 订阅处理失败
onInteractionFailed(listener: (event: UserInteractionFailedEvent) => void);
```

---

## 5. Handler 注册机制

### 5.1 应用层实现接口

```typescript
interface UserInteractionHandler {
  handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown>;
}
```

### 5.2 注册流程

```
应用层
   │
   │ 1. 实现 UserInteractionHandler 接口
   ▼
┌─────────────────────┐
│ userInteractionHandler.handle()
│ - 显示 prompt 给用户
│ - 获取用户输入
│ - 返回 inputData
└─────────┬───────────┘
          │
          ▼
UserInteractionResourceAPI
   │
   │ 2. registerHandler(handler)
   ▼
┌─────────────────────┐
│ this.userInteractionHandler = handler
└─────────────────────┘
          │
          ▼
NodeHandlerContextFactory
   │
   │ 3. 创建节点处理上下文
   ▼
┌─────────────────────┐
│ userInteractionHandler: handler
│ conversationManager
│ timeout
└─────────────────────┘
```

### 5.3 注册示例

```typescript
// 应用层实现
const myHandler: UserInteractionHandler = {
  async handle(request, context) {
    // 显示 prompt 给用户
    const userInput = await showUI(request.prompt);
    return userInput;
  }
};

// 注册到 API
const resourceAPI = sdk.get(UserInteractionResourceAPI);
resourceAPI.registerHandler(myHandler);
```

---

## 6. 与 LLM 节点的集成

### 6.1 工具审批流程 (sdk/core/coordinators/tool-approval-coordinator.ts)

USER_INTERACTION 节点还在 LLM 执行器的工具审批流程中使用：

```
LLM 执行
   │
   │ 检测到需要审批的工具
   ▼
ToolApprovalCoordinator
   │
   │ 1. 触发 USER_INTERACTION_REQUESTED
   ▼
┌─────────────────────┐
│ 等待用户审批         │
│ - 批准/拒绝         │
│ - 编辑参数          │
│ - 超时              │
└─────────┬───────────┘
          │
          │ 2. 触发 USER_INTERACTION_RESPONDED
          ▼
┌─────────────────────┐
│ 处理审批结果         │
│ - approved: true   │
│ - editedParameters │
└─────────┬───────────┘
          │
          │ 3. 触发 USER_INTERACTION_PROCESSED
          ▼
继续 LLM 执行
```

### 6.2 LLM 执行器中的交互 (sdk/graph/execution/coordinators/llm-execution-coordinator.ts)

```typescript
// LLM 执行器等待交互响应
private waitForUserInteractionResponse(interactionId: string) {
  return new Promise((resolve, reject) => {
    const handler = (event: UserInteractionRespondedEvent) => {
      if (event.interactionId === interactionId) {
        eventManager.off("USER_INTERACTION_RESPONDED", handler);
        resolve(event.inputData);
      }
    };
    
    eventManager.on("USER_INTERACTION_RESPONDED", handler);
  });
}
```

---

## 7. 节点定义与校验

### 7.1 节点模板注册 (sdk/api/shared/config/validators/node-template-validator.ts)

```typescript
const userInteractionNodeValidator = {
  type: "USER_INTERACTION",
  schema: UserInteractionNodeConfigSchema,
  validate: isUserInteractionNodeConfig
};
```

### 7.2 配置校验规则

```typescript
// UPDATE_VARIABLES 操作必须包含 variables
.refine(data => {
  if (data.operationType === "UPDATE_VARIABLES") {
    return data.variables && data.variables.length > 0;
  }
  return true;
});

// ADD_MESSAGE 操作必须包含 message
.refine(data => {
  if (data.operationType === "ADD_MESSAGE") {
    return data.message !== undefined;
  }
  return true;
});
```

---

## 8. 典型使用场景

### 8.1 场景一：收集用户输入更新变量

```json
{
  "id": "ask-name",
  "type": "USER_INTERACTION",
  "name": "Ask User Name",
  "config": {
    "operationType": "UPDATE_VARIABLES",
    "variables": [
      {
        "variableName": "userName",
        "expression": "{{input}}",
        "scope": "thread"
      }
    ],
    "prompt": "Please enter your name:",
    "timeout": 60000
  }
}
```

### 8.2 场景二：添加用户消息到对话

```json
{
  "id": "collect-feedback",
  "type": "USER_INTERACTION",
  "name": "Collect Feedback",
  "config": {
    "operationType": "ADD_MESSAGE",
    "message": {
      "role": "user",
      "contentTemplate": "User feedback: {{input}}"
    },
    "prompt": "Please provide your feedback:",
    "timeout": 300000
  }
}
```

### 8.3 场景三：模板中的占位符使用

```json
{
  "id": "order-details",
  "type": "USER_INTERACTION",
  "name": "Get Order Details",
  "config": {
    "operationType": "UPDATE_VARIABLES",
    "variables": [
      {
        "variableName": "orderId",
        "expression": "Order #{{input}}",
        "scope": "thread"
      }
    ],
    "prompt": "Please enter your order ID:",
    "timeout": 60000
  }
}
```

---

## 9. 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                      Graph Workflow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                               │
│   ┌──────────┐    ┌────────────────────┐    ┌──────────┐    │
│   │ LLM Node │───▶│ USER_INTERACTION   │───▶│ LLM Node │    │
│   └──────────┘    │ Node              │    └──────────┘    │
│                   │                  │                     │
│                   │ - operationType  │                     │
│                   │ - variables      │                     │
│                   │ - message        │                     │
│                   │ - prompt         │                     │
│                   │ - timeout        │                     │
│                   └────────┬─────────┘                     │
│                            │                                │
└────────────────────────────┼────────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
┌─────────────────────────┐   ┌─────────────────────────────┐
│ NodeExecutionCoordinator│   │ UserInteractionResourceAPI  │
│                         │   │                             │
│ ┌───────────────────┐  │   │ ┌───────────────────────┐  │
│ │ userInteraction   │  │   │ │ registerHandler()     │  │
│ │ Handler           │  │   │ │ - 保存处理器引用       │  │
│ └─────────┬─────────┘  │   │ └───────────┬───────────┘  │
│           │            │   │             │              │
└───────────┼────────────┘   └─────────────┼──────────────┘
            │                            │
            │ handle()                   │ onInteraction*()
            ▼                            ▼
┌─────────────────────────┐   ┌─────────────────────────────┐
│ Application Layer       │   │ Event Manager               │
│                         │   │                            │
│ ┌───────────────────┐  │   │ - USER_INTERACTION_REQUESTED │
│ │ UI 显示 prompt    │  │   │ - USER_INTERACTION_RESPONDED│
│ │ 获取用户输入      │  │   │ - USER_INTERACTION_PROCESSED│
│ │ 返回 inputData   │  │   │ - USER_INTERACTION_FAILED  │
│ └───────────────────┘  │   └─────────────────────────────┘
└─────────────────────────┘
```

---

## 10. 执行结果类型

```typescript
interface UserInteractionExecutionResult {
  interactionId: string;     // 交互ID
  operationType: string;     // 操作类型
  results: unknown;         // 处理结果
  executionTime: number;     // 执行时间(ms)
}
```

---

## 11. 总结

USER_INTERACTION 节点提供了一套完整的用户交互机制：

1. **声明式配置**: 通过节点配置定义交互行为，无需编写执行逻辑
2. **灵活的输入处理**: 支持变量更新和消息添加两种操作类型
3. **占位符支持**: 通过 `{{input}}` 模板简化输入处理
4. **超时/取消控制**: 内置超时和取消机制确保可靠性
5. **事件驱动**: 完整的事件系统支持监控和日志
6. **应用层集成**: 通过 Handler 接口与应用层解耦