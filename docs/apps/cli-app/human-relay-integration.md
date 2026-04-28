# Human-Relay 功能整合分析

## 功能概述

**Human-Relay（人工中继）** 是 Modular Agent Framework 中的一种特殊 LLM Provider，它允许人工介入 LLM 对话流程，以人的输入代替 LLM API 调用。这是一个创新的设计，使得工作流可以在需要时切换到人工处理模式。

## 架构层次

```
┌─────────────────────────────────────────────────────────┐
│                    CLI 应用层                            │
│  commands/human-relay/  →  适配器  →  格式化工具         │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│                    SDK API 层                            │
│  HumanRelayResourceAPI (资源管理、事件订阅)              │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│                    SDK 核心执行层                        │
│  human-relay-handler.ts + llm-handler.ts 集成           │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│                    类型定义层                            │
│  HumanRelayRequest/Response/Handler 接口                │
└─────────────────────────────────────────────────────────┘
```

## 类型定义层

### 核心类型（packages/types/src/human-relay.ts）

```typescript
// HumanRelay 请求类型
export interface HumanRelayRequest {
  requestId: ID;
  messages: LLMMessage[];      // 包含对话历史
  prompt: string;              // 给用户的提示信息
  timeout: number;             // 超时时间（毫秒）
  metadata?: Metadata;
}

// HumanRelay 响应类型
export interface HumanRelayResponse {
  requestId: ID;
  content: string;             // 人工输入的消息内容
  timestamp: number;
}

// HumanRelay 处理器接口（应用层必须实现）
export interface HumanRelayHandler {
  handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse>;
}
```

### LLM Provider 类型（packages/types/src/llm/state.ts）

```typescript
export type LLMProvider =
  | 'OPENAI_CHAT'
  | 'OPENAI_RESPONSE'
  | 'ANTHROPIC'
  | 'GEMINI_NATIVE'
  | 'GEMINI_OPENAI'
  | 'HUMAN_RELAY';  // 人工中继作为一种 LLM Provider
```

### 事件类型（packages/types/src/events/interaction-events.ts）

```typescript
// 请求事件
export interface HumanRelayRequestedEvent extends BaseEvent {
  type: 'HUMAN_RELAY_REQUESTED';
  requestId: ID;
  prompt: string;
  messageCount: number;
  timeout: number;
}

// 响应事件
export interface HumanRelayRespondedEvent extends BaseEvent {
  type: 'HUMAN_RELAY_RESPONDED';
  requestId: ID;
  content: string;
}

// 处理完成事件
export interface HumanRelayProcessedEvent extends BaseEvent {
  type: 'HUMAN_RELAY_PROCESSED';
  requestId: ID;
  message: { role: string; content: string };
  executionTime: number;
}

// 失败事件
export interface HumanRelayFailedEvent extends BaseEvent {
  type: 'HUMAN_RELAY_FAILED';
  requestId: ID;
  reason: string;
}
```

## SDK 核心执行层

### 核心执行处理器（sdk/core/execution/handlers/human-relay-handler.ts）

这是 HumanRelay 的核心执行逻辑，采用无状态函数式设计：

**关键函数**：

```typescript
// 执行 HumanRelay（主入口）
export async function executeHumanRelay(
  messages: LLMMessage[],
  prompt: string,
  timeout: number,
  threadEntity: ThreadEntity,
  eventManager: EventManager,
  humanRelayHandler: HumanRelayHandler,
  nodeId: string
): Promise<HumanRelayExecutionResult>

// 创建 HumanRelay 请求
export function createHumanRelayRequest(task: HumanRelayTask): HumanRelayRequest

// 获取人工输入（带超时和取消控制）
export async function getHumanInput(
  task: HumanRelayTask,
  request: HumanRelayRequest,
  context: HumanRelayContext,
  handler: HumanRelayHandler
): Promise<HumanRelayResponse>

// 将人工输入转换为 LLM 消息
export function convertToLLMMessage(
  task: HumanRelayTask,
  response: HumanRelayResponse
): LLMMessage
```

**执行流程**：
1. 创建 HumanRelay 请求
2. 触发 `HUMAN_RELAY_REQUESTED` 事件
3. 创建 HumanRelay 上下文
4. 调用应用层处理器获取人工输入（带超时和取消控制）
5. 触发 `HUMAN_RELAY_RESPONDED` 事件
6. 将人工输入转换为 LLM 消息
7. 触发 `HUMAN_RELAY_PROCESSED` 事件

### LLM 节点处理器集成（sdk/core/execution/handlers/node-handlers/llm-handler.ts）

在 LLM 节点执行时，会检查是否为 HumanRelay provider：

```typescript
// 检查是否为 HumanRelay provider
const profile = context.llmWrapper.getProfile(executionData.profileId || 'DEFAULT');
if (profile?.provider === 'HUMAN_RELAY') {
  return await executeHumanRelayLLMNode(thread, node, executionData, context, startTime);
}

// 执行 HumanRelay LLM节点
async function executeHumanRelayLLMNode(
  thread: Thread,
  node: Node,
  requestData: any,
  context: LLMHandlerContext,
  startTime: number
): Promise<LLMExecutionResult> {
  if (!context.humanRelayHandler) {
    throw new ExecutionError('HumanRelayHandler is not provided', node.id);
  }

  // 获取当前对话消息
  const messages = context.conversationManager.getMessages();

  // 调用 executeHumanRelay 函数
  const result = await executeHumanRelay(
    messages,
    requestData.prompt || 'Please provide your input:',
    requestData.parameters?.timeout || 300000,
    { thread, conversationManager: context.conversationManager } as any,
    context.eventManager,
    context.humanRelayHandler,
    node.id
  );

  return {
    status: 'COMPLETED',
    content: typeof result.message.content === 'string' ? result.message.content : JSON.stringify(result.message.content),
    executionTime: diffTimestamp(startTime, endTime)
  };
}
```

### API 资源管理（sdk/api/resources/human-relay/human-relay-resource-api.ts）

提供 HumanRelay 的资源管理 API：

```typescript
export class HumanRelayResourceAPI extends GenericResourceAPI<HumanRelayConfig, string, HumanRelayFilter> {
  // 处理器管理
  registerHandler(handler: HumanRelayHandler): void
  getHandler(): HumanRelayHandler | undefined
  clearHandler(): void

  // 处理请求
  async handleRequest(request: HumanRelayRequest): Promise<ExecutionResult<HumanRelayResponse>>

  // 事件订阅
  onRelayRequested(listener: (event: HumanRelayRequestedEvent) => void): void
  onRelayResponded(listener: (event: HumanRelayRespondedEvent) => void): void
  onRelayProcessed(listener: (event: HumanRelayProcessedEvent) => void): void
  onRelayFailed(listener: (event: HumanRelayFailedEvent) => void): void

  // 配置管理
  async setConfigEnabled(id: string, enabled: boolean): Promise<ExecutionResult<void>>
}
```

## CLI 应用层整合

### 命令注册（apps/cli-app/src/index.ts）

```typescript
import { createHumanRelayCommands } from './commands/human-relay/index.js';

// 添加 Human Relay 命令组
program.addCommand(createHumanRelayCommands());
```

### 命令实现（apps/cli-app/src/commands/human-relay/index.ts）

提供完整的 CLI 命令组：

| 命令 | 功能 |
|------|------|
| `human-relay register <file>` | 从文件注册配置 |
| `human-relay list` | 列出所有配置 |
| `human-relay show <id>` | 查看配置详情 |
| `human-relay delete <id>` | 删除配置 |
| `human-relay update <id>` | 更新配置 |
| `human-relay enable <id>` | 启用配置 |
| `human-relay disable <id>` | 禁用配置 |

### 适配器层（apps/cli-app/src/adapters/human-relay-adapter.ts）

封装 SDK API 调用：

```typescript
export class HumanRelayAdapter extends BaseAdapter {
  async listConfigs(filter?: HumanRelayFilter): Promise<HumanRelayConfig[]>
  async getConfig(id: string): Promise<HumanRelayConfig>
  async createConfig(config: HumanRelayConfig): Promise<HumanRelayConfig>
  async updateConfig(id: string, updates: Partial<HumanRelayConfig>): Promise<HumanRelayConfig>
  async deleteConfig(id: string): Promise<void>
  async enableConfig(id: string): Promise<void>
  async disableConfig(id: string): Promise<void>
}
```

### 格式化工具（apps/cli-app/src/utils/formatter.ts）

```typescript
// 格式化单个配置
export function formatHumanRelay(config: any, options: { verbose?: boolean } = {}): string

// 格式化配置列表
export function formatHumanRelayList(configs: any[], options: { table?: boolean } = {}): string
```

## 工作机制

### 执行流程图

```
工作流执行 LLM 节点
        ↓
检测 provider === 'HUMAN_RELAY'
        ↓
触发 HUMAN_RELAY_REQUESTED 事件
        ↓
调用 HumanRelayHandler 获取人工输入
        ↓
触发 HUMAN_RELAY_RESPONDED 事件
        ↓
将人工输入转换为 LLM 消息格式
        ↓
触发 HUMAN_RELAY_PROCESSED 事件
        ↓
继续工作流执行
```

### 配置阶段

1. 通过 CLI 或 API 注册 HumanRelay 配置
2. 配置包含 ID、名称、描述、默认超时时间等

### 工作流定义

1. 在 LLM 节点配置中指定 `provider: 'HUMAN_RELAY'`
2. 设置相应的 profileId 指向 HumanRelay 配置

### 执行阶段

1. 当工作流执行到 LLM 节点时，检测到 provider 为 `HUMAN_RELAY`
2. 触发 `HUMAN_RELAY_REQUESTED` 事件，通知应用层
3. 应用层通过 `HumanRelayHandler` 获取人工输入
4. 支持超时控制和取消操作
5. 将人工输入转换为 LLM 消息格式
6. 触发相应的事件（成功/失败）

## 事件系统

支持完整的事件生命周期：

| 事件类型 | 触发时机 |
|---------|---------|
| `HUMAN_RELAY_REQUESTED` | 请求人工输入时 |
| `HUMAN_RELAY_RESPONDED` | 收到人工响应时 |
| `HUMAN_RELAY_PROCESSED` | 处理完成时 |
| `HUMAN_RELAY_FAILED` | 处理失败时 |

## 设计优势

1. **统一接口** - 与其他 LLM Provider 保持一致，工作流无需特殊处理
2. **事件驱动** - 完整的事件系统支持监控和扩展
3. **无状态设计** - 核心处理函数都是纯函数，易于测试
4. **可扩展性** - 应用层可自定义 `HumanRelayHandler` 实现
5. **超时控制** - 支持超时和取消操作

## 关键文件路径

| 层次 | 文件路径 |
|------|---------|
| 类型定义 | `packages/types/src/human-relay.ts` |
| 事件定义 | `packages/types/src/events/interaction-events.ts` |
| 核心执行 | `sdk/core/execution/handlers/human-relay-handler.ts` |
| LLM 集成 | `sdk/core/execution/handlers/node-handlers/llm-handler.ts` |
| API 资源 | `sdk/api/resources/human-relay/human-relay-resource-api.ts` |
| CLI 命令 | `apps/cli-app/src/commands/human-relay/index.ts` |
| CLI 适配器 | `apps/cli-app/src/adapters/human-relay-adapter.ts` |

## 使用场景

Human-Relay 特别适合以下场景：

1. **人工审核** - 在关键决策点需要人工确认
2. **复杂问题处理** - 当 LLM 无法处理时转交给人工
3. **训练数据收集** - 收集人工回复用于模型微调
4. **混合工作流** - 自动化流程中需要人工干预的环节
5. **调试和测试** - 在开发过程中模拟特定响应

## 总结

Human-Relay 将人工输入作为一种特殊的 LLM Provider，使得工作流可以在需要时无缝切换到人工处理模式。这种设计保持了与其他 LLM Provider 一致的接口，同时提供了完整的事件系统和可扩展的处理器接口，非常适合需要人工审核、决策或干预的场景。
