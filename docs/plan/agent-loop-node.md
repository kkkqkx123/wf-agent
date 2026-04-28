
## 修订方案：Agent Loop 精简设计

### 一、设计定位

| 场景 | 推荐方案 |
|------|----------|
| **简单任务**（单轮/多轮工具调用） | ✅ Agent Loop |
| **主协调引擎**（按需调用子工作流） | ✅ Agent Loop |
| **复杂控制流**（条件分支、状态机） | ❌ 使用 LOOP_START/LOOP_END + 图编排 |

### 二、极简配置设计

```typescript
export interface AgentLoopNodeConfig {
  profileId: string;
  maxIterations?: number;  // default: 20
  tools?: string[];
  systemPrompt?: string;
}
```

### 三、执行流程（极简）

```
while iteration < maxIterations:
  response = llm.generate(messages)
  
  if !response.hasToolCalls:
    return response.content
  
  results = executeTools(response.toolCalls)
  messages.append(toolResults)
  iteration++

return response.content
```

### 四、与图工作流的关系

**场景1：简单任务**
```
[START] → [AGENT_LOOP] → [END]
```

**场景2：主协调引擎**
```
[START] → [AGENT_LOOP] → [END]
              └→ 工具调用：call_subgraph("任务A")
```

**场景3：复杂控制**（使用现有LOOP节点）
```
[START] → [LLM] → [ROUTE] → [TOOL_A] → [LLM] → [END]
```

### 五、关键设计决策

| 决策 | 说明 |
|------|------|
| 无复杂停止条件 | 只提供最大迭代次数 |
| 无内置检查点 | 复用节点级别配置 |
| 无上下文裁剪 | 复用ConversationManager |
| 简单输入输出 | 从上下文变量读取input，输出写入output |
| 工具调用复用 | 使用现有的ToolCallExecutor |

### 六、文件结构

```
packages/types/src/node/agent-loop.ts
sdk/core/execution/handlers/node-handlers/agent-loop-handler.ts
```

### 七、NodeType 更新

添加 `'AGENT_LOOP'` 到 NodeType 联合类型。
