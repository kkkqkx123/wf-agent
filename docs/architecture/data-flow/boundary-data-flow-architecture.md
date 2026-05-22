# 工作流边界数据传递架构

**文档日期**: 2026-05-22  
**状态**: 已实施

---

## 1. 设计理念

工作流本身被视为一个"函数调用"：

```
调用方 (父工作流)  ── 输入参数 ──▶  工作流 (被调用方)  ── 返回值 ──▶ 调用方
```

因此，工作流边界的数据传递应遵循**显式契约**原则：

- 入口节点声明输入参数（如同函数签名）
- 出口节点声明返回值（如同 return 语句）
- 所有跨边界传递必须是显式的，没有隐式继承

---

## 2. 三层数据架构

工作流边界数据传递分为三个独立层次，各自承担不同职责：

| 层次 | 存储位置 | 类型 | 用途 |
|---|---|---|---|
| **Data** | `WorkflowExecution.input / output` | 原始值 | 调用方传入/传出的执行数据 payload |
| **Variable** | `VariableStateManager` | 结构化变量 | 工作流内运行时变量，有生命周期管理 |
| **Message** | `MessageContextRegistry` | LLMMessage[] | LLM 对话历史，按命名上下文组织 |

### 2.1 传递路径

```
调用方执行输入 (input)
         │
         ▼  dataInputs（按 parentField 匹配）
    VariableStateManager
         │
         ├── variableInputs（父→子：按变量名映射）
         ├── variableOutputs（子→父：按变量名映射）
         │
         ▼  dataOutputs（按 outputKey 映射）
调用方执行输出 (output)

MessageContextRegistry（独立于变量系统）
    ├── messageInputs（父→子：注册命名上下文）
    └── messageOutputs（子→父：拷贝命名上下文）
```

### 2.2 分离理由

**Data 和 Message 不应合并：**

| 维度 | Data | Message |
|---|---|---|
| 本质 | 执行输入/输出数据（任意 JSON 值） | LLM 对话历史（role + content） |
| 存储路径 | `VariableStateManager` | `MessageContextRegistry` |
| 消费方 | 工作流内任何节点 | LLM 节点、Agent Loop |
| 业务语义 | 业务数据交换 | 对话上下文交换 |

如果合并，会导致：
- 消息上下文需要无意义地映射到变量管理器
- 变量变化会污染消息内容
- 配置意图不清晰

---

## 3. 统一边界配置接口

### 3.1 入口配置 (WorkflowStartConfig)

用于：`START`、`SUBGRAPH_START`、`START_FROM_TRIGGER` 节点

```typescript
interface WorkflowStartConfig {
  variableInputs?: WorkflowVariableInput[];   // 变量输入映射
  messageInputs?: WorkflowMessageInput[];      // 消息上下文输入
  dataInputs?: WorkflowDataInput[];            // 执行数据输入
}

interface WorkflowDataInput {
  parentField: string;           // 调用方 input 中的字段名
  internalName: string;          // 当前工作流内部变量名
  required?: boolean;
  defaultValue?: unknown;
}

interface WorkflowVariableInput {
  externalName: string;          // 父工作流中的变量名
  internalName: string;          // 当前工作流内部变量名
  required?: boolean;
  defaultValue?: unknown;
}

interface WorkflowMessageInput {
  externalName: string;          // 调用方的命名上下文 ID
  internalName: string;          // 当前工作流内部上下文 ID
  required?: boolean;
  defaultMessages?: LLMMessage[];
}
```

### 3.2 出口配置 (WorkflowEndConfig)

用于：`END`、`SUBGRAPH_END`、`CONTINUE_FROM_TRIGGER` 节点

```typescript
interface WorkflowEndConfig {
  variableOutputs?: WorkflowVariableOutput[];  // 变量输出映射
  messageOutputs?: WorkflowMessageOutput[];     // 消息上下文输出
  dataOutputs?: WorkflowDataOutput[];           // 执行数据输出
}

interface WorkflowDataOutput {
  internalName: string;          // 当前工作流内部变量名
  outputKey: string;             // 输出中的字段名
}

interface WorkflowVariableOutput {
  internalName: string;          // 当前工作流内部变量名
  externalName: string;          // 父工作流接受时的变量名
}

interface WorkflowMessageOutput {
  internalName: string;          // 当前工作流内部上下文 ID
  externalName: string;          // 父工作流接受时的上下文 ID
}
```

---

## 4. 各节点类型的实现状态

### 4.1 节点分类矩阵

| 节点类型 | 创建子执行？ | variable | message | data | 实现状态 |
|---|---|---|---|---|---|
| SUBGRAPH | ✅ createChildExecution | variableInputs / variableOutputs | messagePassing | dataInputs | **已完成** |
| START_FROM_TRIGGER | ✅ createChildExecution | variableInputs | messageInputs | dataInputs | **已完成** |
| CONTINUE_FROM_TRIGGER | ✅ 出口节点 | variableOutputs | messageOutputs | dataOutputs | **已完成** |
| AGENT_LOOP | ✅ AgentLoopCoordinator | - | - | dataInputs | **已完成** |
| FORK | ✅ createChildExecution (FORK_BRANCH) | 深拷贝父变量（无需映射） | 共享上下文（无需映射） | 深拷贝 input（无需映射） | **不需要** |
| JOIN | 占位符 | variableOutputs | messageOutputs | dataOutputs | **已完成** |
| SYNC | 分支内运行 | variableMappings | messageInputs | dataInputs | **已完成** |
| EMBED_GRAPH | 预处理展开 | 共享父变量管理器 | 共享父上下文 | 共享父 input | **不需要** |

### 4.2 详细说明

#### SUBGRAPH
- **工作流侧**: [SubgraphNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/subgraph-configs.ts) 包含 `variableInputs`、`variableOutputs`、`dataInputs`、`messagePassing`
- **处理器侧**: [subgraph-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts) 构建 `variableMapping` 和 `dataMapping`，通过 `createChildExecution()` 传递
- **构建器侧**: [workflow-execution-builder.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/factories/workflow-execution-builder.ts) 的 `initializeVariables` 处理 `dataMapping` 中的 data→variable 映射
- 完成后通过 `exportVariables` 将子变量写回父变量管理器

#### START_FROM_TRIGGER (触发子工作流入口)
- **配置类型**: `WorkflowStartConfig`（与 START 节点共享同一接口）
- **处理器侧**: [start-from-trigger-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/start-from-trigger-handler.ts)
  - `messageInputs`: 从 triggerInput 中读取消息上下文并注册到 `MessageContextRegistry`
  - `dataInputs`: 从 `workflowExecutionEntity.getInput()` 中读取 `parentField` 并设置到 `variableStateManager`
  - `variableInputs`: 通过 `triggerInput.variables` 传入（由 TriggeredSubworkflowHandler 的 `prepareInputData` 构建）

#### CONTINUE_FROM_TRIGGER (触发子工作流出口)
- **配置类型**: `WorkflowEndConfig`（与 END 节点共享同一接口）
- **处理器侧**: [continue-from-trigger-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/continue-from-trigger-handler.ts)
  - `variableOutputs`: 从子 `variableStateManager` 读取值，写入父 `variableStateManager`
  - `messageOutputs`: 从子 `MessageContextRegistry` 拷贝上下文到父注册表
  - `dataOutputs`: 从子变量读取值，写入 `workflowExecutionEntity.output`

#### AGENT_LOOP
- **配置类型**: [AgentLoopNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/agent-loop-configs.ts) 的 `inlineConfig` 包含 `dataInputs`
- **处理器侧**: [agent-loop-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts)
  - 在处理前从 `executionEntity.getInput()` 读取 `parentField` 并设置到 `variableStateManager`
  - Agent Loop 内部通过 `variableStateManager.getVariable("input")` 或 `getVariable("prompt")` 获取用户消息初始内容
  - 完成后通过 `variableStateManager.setVariable("output", result.content)` 写回
- **注意**: Agent Loop 通过 `AgentLoopCoordinator` 创建独立执行实体，不走 `createChildExecution`，因此 data 传递在当前工作流侧变量管理器内完成

#### FORK (分支)
- **变量传递**: [fork-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts) 在 `FORK_BRANCH` 类型的 `initializeVariables` 中执行：
  ```typescript
  case 'FORK_BRANCH':
    // Fork: complete deep clone
    child.variableStateManager.copyFrom(parent.variableStateManager);
  ```
- 每个分支拥有父执行所有变量的**深度克隆**，因此**不需要显式 data/variable 映射**
- 执行输入同样通过深拷贝获得

#### JOIN (合并)
- **配置类型**: [JoinNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/fork-join-configs.ts)
  - `variableOutputs`: 从分支导出变量到父工作流
  - `messageOutputs`: 从分支导出消息上下文到父工作流
  - `dataOutputs`: 从内部变量映射到执行输出键
- **处理器侧**: [join-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/join-handler.ts) 是一个占位符，实际合并逻辑由 `WorkflowCoordinator` 处理

#### SYNC (分支间同步)
- **配置类型**: [SyncNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs.ts)
  - `variableMappings`: 从源分支导入变量到目标分支（使用 `WorkflowVariableInput`）
  - `dataInputs`: 从父执行输入映射变量到目标分支
  - `messageInputs`: 从源分支同步消息上下文到目标分支
- **处理器侧**: [sync-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/sync-handler.ts)
  - 通过 `SyncBarrier` 定位源分支执行实体
  - 变量通过 `importVariables` 深度克隆
  - 消息通过 `MessageContextRegistry.register` 浅拷贝
  - data 直接设置到 `variableStateManager`

#### EMBED_GRAPH (嵌入图)
- **设计定位**: 轻量级子图，**共享父执行的所有状态**
- 预处理阶段展开为 `EMBED_START` / `EMBED_END`，运行在父执行实体内
- **不需要任何 data/variable/message 映射**，因为共享变量管理器和上下文注册表

---

## 5. 核心处理流程

### 5.1 数据输入处理 (dataInputs)

所有支持 dataInputs 的节点遵循统一模式：

```
1. 读取 workflowExecutionEntity.getInput()（执行输入）
2. 遍历 dataInputs 数组
3. 按 parentField 在 input 中查找值
4. 如果值存在，设置到 variableStateManager.setVariable(internalName, value)
5. 如果值不存在且 required=true，抛出 RuntimeValidationError
6. 如果值不存在且 defaultValue 存在，使用 defaultValue
```

### 5.2 数据输出处理 (dataOutputs)

所有支持 dataOutputs 的节点遵循统一模式：

```
1. 读取 workflowExecutionEntity.getOutput() || {}
2. 遍历 dataOutputs 数组
3. 从 variableStateManager.getVariable(internalName) 读取值
4. 如果值存在，设置到 output[outputKey] = value
5. 最终调用 workflowExecutionEntity.setOutput(output)
```

### 5.3 消息上下文输入处理 (messageInputs)

```
1. 从 triggerInput.messageContexts 或 input[externalName] 获取消息数组
2. 通过 MessageContextRegistry.register() 以 internalName 注册
3. 支持默认消息 (defaultMessages) 和必需性校验 (required)
```

### 5.4 消息上下文输出处理 (messageOutputs)

```
1. 从子 MessageContextRegistry.get(internalName) 获取命名上下文
2. 通过父 MessageContextRegistry.register() 以 externalName 注册
3. 使用浅拷贝 ([...messages]) 保持引用隔离
```

---

## 6. 边界数据流图

### 6.1 SUBGRAPH 场景

```
父工作流                        子工作流 (独立执行实体)
───────                        ────────────────────────
input: { ... } ── dataInputs ──▶ variableStateManager
variableStateManager ── variableInputs ──▶ variableStateManager
MessageContextRegistry ── messagePassing.inputs ──▶ MessageContextRegistry
                                                         │
variableStateManager ◀── variableOutputs ──── variableStateManager
output: { ... } ◀── (END 节点 dataOutputs) ── variableStateManager
MessageContextRegistry ◀── messagePassing.outputs ◀── MessageContextRegistry
```

### 6.2 TRIGGERED 子工作流场景

```
主工作流                       子工作流 (由 TriggeredSubworkflowHandler 管理)
───────                       ─────────────────────────────────────────────
触发器 ── prepareInputData ──▶ input
  ├── variables ──────────────▶ START_FROM_TRIGGER.variableInputs ──▶ variables
  ├── messageContexts ────────▶ START_FROM_TRIGGER.messageInputs ───▶ MessageContextRegistry
  └── input fields ───────────▶ START_FROM_TRIGGER.dataInputs ───────▶ variableStateManager
                                        │
variableStateManager ◀── CONTINUE_FROM_TRIGGER.variableOutputs ◀── variableStateManager
output ◀── CONTINUE_FROM_TRIGGER.dataOutputs ◀── variableStateManager
MessageContextRegistry ◀── CONTINUE_FROM_TRIGGER.messageOutputs ◀── MessageContextRegistry
```

### 6.3 SYNC 场景（分支间）

```
源分支 (sourcePathId)                   目标分支 (当前分支)
─────────────────                     ─────────────────────
variableStateManager ── variableMappings ──▶ variableStateManager
MessageContextRegistry ── messageInputs ────▶ MessageContextRegistry
父执行 input ── dataInputs ────────────────▶ variableStateManager
```

### 6.4 JOIN 场景

```
分支 1 ──▶ variableStateManager ── variableOutputs ──▶ 父 variableStateManager
分支 2 ──▶ variableStateManager ── variableOutputs ──▶ 父 variableStateManager
所有分支 ──▶ variableStateManager ── dataOutputs ────▶ 父 output
分支 1 ──▶ MessageContextRegistry ── messageOutputs ──▶ 父 MessageContextRegistry
```

---

## 7. 关键代码位置

| 组件 | 文件 |
|---|---|
| 边界配置类型定义 | [boundary-config.ts](file:///d:/项目/agent/wf-agent/packages/types/src/workflow/boundary-config.ts) |
| 边界配置 Zod Schema | [boundary-config-schema.ts](file:///d:/项目/agent/wf-agent/packages/types/src/workflow/boundary-config-schema.ts) |
| SUBGRAPH 节点配置 | [subgraph-configs.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/subgraph-configs.ts) |
| SUBGRAPH 处理器 | [subgraph-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts) |
| START_FROM_TRIGGER 处理器 | [start-from-trigger-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/start-from-trigger-handler.ts) |
| CONTINUE_FROM_TRIGGER 处理器 | [continue-from-trigger-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/continue-from-trigger-handler.ts) |
| AGENT_LOOP 配置 | [agent-loop-configs.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/agent-loop-configs.ts) |
| AGENT_LOOP 处理器 | [agent-loop-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts) |
| SYNC 配置 | [sync-configs.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs.ts) |
| SYNC 处理器 | [sync-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/sync-handler.ts) |
| JOIN 配置 | [fork-join-configs.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/fork-join-configs.ts) |
| JOIN 处理器（占位符） | [join-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/join-handler.ts) |
| END 处理器（参考实现） | [end-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/end-handler.ts) |
| 工作流执行构建器 | [workflow-execution-builder.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/factories/workflow-execution-builder.ts) |
| TriggeredSubworkflowHandler | [triggered-subworkflow-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/triggered-subworkflow-handler.ts) |
| FORK 处理器 | [fork-handler.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts) |