# 用户交互节点改进指南 - 快速参考

## 核心问题

**当前**：配置混淆应用层细节 + 执行与 LLM 模块混淆  
**改进**：清晰的接口、业务语义、完整的事件系统

---

## 接口体系（3 层）

### 1. 节点配置接口（SDK 业务语义）
```
UserInteractionNodeConfig
├─ operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE'
├─ variables[]          (当 operationType = UPDATE_VARIABLES)
│  ├─ variableName
│  ├─ expression (包含 {{input}} 占位符)
│  └─ scope
├─ message              (当 operationType = ADD_MESSAGE)
│  ├─ role: 'user'
│  └─ contentTemplate (包含 {{input}} 占位符)
├─ prompt              (应用层显示给用户)
└─ timeout
```

### 2. 处理器接口（应用层实现）
```
UserInteractionHandler
└─ handle(request) → Promise<userInput>
   ├─ 接收：交互请求 (prompt, timeout等)
   ├─ 处理：获取用户输入 (前端弹窗/CLI等)
   └─ 返回：原始用户输入数据
```

### 3. 上下文接口（SDK 提供）
```
UserInteractionContext
├─ 线程信息：threadId, workflowId, nodeId
├─ 变量操作：getVariable(), setVariable()
├─ 消息管理：conversationManager
├─ 事件系统：eventManager
└─ 超时控制：timeout, cancelToken
```

---

## 两种使用场景

### 场景 1：变量更新（用户批准）

```
配置:
{
  operationType: 'UPDATE_VARIABLES',
  variables: [{
    variableName: 'approved',
    expression: '{{input}}',  // 用户输入
    scope: 'thread'
  }],
  prompt: '是否批准？',
  timeout: 5000
}

流程:
用户交互节点 → 显示提示 → 用户输入 → 更新 approved = true/false → ROUTE 判断
```

### 场景 2：消息添加（LLM 询问用户）

```
配置:
{
  operationType: 'ADD_MESSAGE',
  message: {
    role: 'user',
    contentTemplate: '{{input}}'
  },
  prompt: '请输入您的问题：',
  timeout: 30000
}

流程:
用户交互节点 → 显示提示 → 用户输入 → 添加消息到 conversationManager → LLM 继续对话
```

---

## 事件生命周期

```
1. USER_INTERACTION_REQUESTED
   ├─ SDK 发送：nodeId, prompt, operationType, timeout
   
2. (应用层处理：获取用户输入)
   
3. USER_INTERACTION_RESPONDED
   ├─ 应用层发送：inputData
   
4. SDK 处理用户输入
   ├─ UPDATE_VARIABLES：解析 {{input}}，更新变量
   └─ ADD_MESSAGE：格式化为消息，添加到对话历史
   
5. USER_INTERACTION_PROCESSED
   ├─ SDK 发送：results（更新的变量或消息）
   
6. 工作流继续执行
```

---

## 关键设计原则

| 原则 | 说明 |
|---|---|
| **分离关注** | SDK 不知道应用层如何显示 UI，应用层不知道 SDK 如何处理输入 |
| **业务语义** | 配置定义**业务操作**（UPDATE_VARIABLES、ADD_MESSAGE），不定义**UI 细节** |
| **占位符机制** | 通过 `{{input}}` 占位符让应用层输入集成到 SDK 处理中 |
| **事件驱动** | 完整的事件系统，每个阶段都有对应的事件，便于监听和扩展 |

---

## 对比：改进前后

### 配置对比

**改进前**（混淆应用层）：
```typescript
{
  userInteractionType: 'ask_for_approval',
  showMessage: '是否批准？',    // ← 应用层细节
  userInput: undefined           // ← 由应用层填充
}
```

**改进后**（纯业务语义）：
```typescript
{
  operationType: 'UPDATE_VARIABLES',
  variables: [{
    variableName: 'approved',
    expression: '{{input}}',
    scope: 'thread'
  }],
  prompt: '是否批准？'             // ← 应用层用于显示
}
```

### 执行对比

**改进前**：
```
USER_INTERACTION → 转换为 LLM 提示词 → LLM 返回文本 ✗
（完全错误，应该由应用层处理）
```

**改进后**：
```
USER_INTERACTION → 触发事件 → 应用层获取输入 → 根据 operationType 处理 → 更新变量/消息 ✓
（清晰的职责分工）
```

---

## 实施要点

### SDK 层（3 个接口）
- `UserInteractionHandler`：应用层必须实现的接口
- `UserInteractionContext`：SDK 提供的执行上下文
- `UserInteractionNodeConfig`：改为业务语义

### 执行流程
- 创建 `UserInteractionCoordinator`
- 修改 `NodeExecutionCoordinator`：从 LLM 托管节点中分离 USER_INTERACTION
- 添加 4 个事件类型

### 应用层
- 实现 `UserInteractionHandler` 接口
- 负责获取用户输入（前端交互）
- 返回原始数据给 SDK

---

## 三个关键改变

1. **配置改变**：从 `showMessage/userInput` → 业务语义的 `operationType/variables/message`
2. **执行改变**：从 LLM 混淆 → 清晰的请求→处理→执行流程
3. **接口改变**：从无清晰接口 → 3 层接口体系（Config、Handler、Context）

---

## 最终效果

```
SDK 层：清晰定义用户交互的语义和处理规范
   ↕ (接口 + 事件)
应用层：专注于获取用户输入，不关心 SDK 如何处理
```

**完全解耦** + **业务正确** + **易于扩展**
