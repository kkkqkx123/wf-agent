# Human Relay 架构设计文档

## 概述

**Human Relay（人工中继）** 是 Modular Agent Framework 中的一种特殊 LLM Provider，它允许人工介入 LLM 对话流程，以人的输入代替 LLM API 调用。

### 核心概念

Human Relay 的本质是：**用人工输入替代 LLM API 调用**。这与工具审批（Tool Approval）有本质区别：

| 特性 | Human Relay | Tool Approval |
|------|-------------|---------------|
| **目的** | 用人工输入替代 LLM 响应 | 人工审批是否允许工具执行 |
| **触发时机** | LLM 节点执行时 | 工具调用前 |
| **输入内容** | 人工输入的回复消息 | 审批结果（approve/reject）|
| **返回值** | 作为 LLM 响应继续工作流 | 决定是否执行工具 |
| **事件类型** | `HUMAN_RELAY_*` | `USER_INTERACTION_*` |

## 架构层次

```
┌─────────────────────────────────────────────────────────────────┐
│                    应用层 (CLI/Web/App)                          │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  HumanRelayHandler 实现类                                │   │
│  │  - CLIHumanRelayHandler (命令行交互)                     │   │
│  │  - WebHumanRelayHandler (WebSocket/Web界面)              │   │
│  │  - APIHumanRelayHandler (REST API)                       │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              ↓ registerHandler()
┌─────────────────────────────────────────────────────────────────┐
│              SDK API 层 - HumanRelayResourceAPI                  │
│  - 管理 HumanRelayHandler 实例                                  │
│  - 提供事件订阅接口                                              │
│  - 配置管理 (CRUD)                                              │
│  - 同步 Handler 到 LLMWrapper                                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                    SDK 核心执行层                                │
│  ┌─────────────────────┐    ┌─────────────────────────────┐   │
│  │   HumanRelayClient  │←───│    ClientFactory            │   │
│  │   (LLMClient 实现)   │    │    - 创建 HumanRelayClient  │   │
│  └──────────┬──────────┘    └─────────────────────────────┘   │
│             ↓                                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │          human-relay-handler.ts                        │   │
│  │  - executeHumanRelay()  主执行函数                      │   │
│  │  - createHumanRelayRequest()  创建请求                  │   │
│  │  - getHumanInput()  获取人工输入                        │   │
│  │  - convertToLLMMessage()  转换为 LLM 消息               │   │
│  └─────────────────────────────────────────────────────────┘   │
│             ↓                                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │          llm-handler.ts                                │   │
│  │  - 检测 provider === 'HUMAN_RELAY'                      │   │
│  │  - 调用 executeHumanRelayLLMNode()                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                    类型定义层                                    │
│  - human-relay.ts: 核心类型定义                                  │
│  - interaction-events.ts: 事件类型定义                           │
└─────────────────────────────────────────────────────────────────┘
```

## 核心组件

### 1. 类型定义层

#### 1.1 HumanRelayRequest

```typescript
export interface HumanRelayRequest {
  requestId: ID;
  messages: LLMMessage[];      // 对话历史
  prompt: string;              // 给用户的提示
  timeout: number;             // 超时时间（毫秒）
  metadata?: Metadata;
}
```

#### 1.2 HumanRelayResponse

```typescript
export interface HumanRelayResponse {
  requestId: ID;
  content: string;             // 人工输入内容
  timestamp: number;
}
```

#### 1.3 HumanRelayHandler

```typescript
export interface HumanRelayHandler {
  handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse>;
}
```

#### 1.4 HumanRelayContext

```typescript
export interface HumanRelayContext {
  threadId: ID;
  workflowId: ID;
  nodeId: ID;
  getVariable(variableName: string, scope?: VariableScope): any;
  setVariable(variableName: string, value: any, scope?: VariableScope): Promise<void>;
  getVariables(scope?: VariableScope): Record<string, any>;
  timeout: number;
  cancelToken: { cancelled: boolean; cancel(): void };
}
```

### 2. 事件系统

Human Relay 使用以下事件：

| 事件名称 | 触发时机 | 说明 |
|---------|---------|------|
| `HUMAN_RELAY_REQUESTED` | 请求开始时 | 通知应用层显示输入界面 |
| `HUMAN_RELAY_RESPONDED` | 用户输入完成 | 通知应用层输入已接收 |
| `HUMAN_RELAY_PROCESSED` | 处理完成 | 通知应用层处理结果 |
| `HUMAN_RELAY_FAILED` | 处理失败 | 通知应用层错误信息 |

### 3. HumanRelayClient

`HumanRelayClient` 实现了 `LLMClient` 接口，使 Human Relay 可以无缝替换任何 LLM Provider：

```typescript
export class HumanRelayClient implements LLMClient {
  async generate(request: LLMRequest): Promise<LLMResult>;
  async *generateStream(request: LLMRequest): AsyncIterable<LLMResult>;
}
```

**设计特点：**
- 无状态设计
- 实现标准 LLMClient 接口
- 支持超时和取消机制
- 流式响应模拟（一次性返回）

### 4. ClientFactory 集成

`ClientFactory` 负责创建 `HumanRelayClient` 实例：

```typescript
export class ClientFactory {
  private humanRelayHandler?: HumanRelayHandler;
  
  setHumanRelayHandler(handler: HumanRelayHandler): void;
  createClient(profile: LLMProfile): LLMClient;
}
```

当 `profile.provider === 'HUMAN_RELAY'` 时，创建 `HumanRelayClient`。

## 执行流程

```
1. 工作流执行到 LLM 节点
        ↓
2. LLMHandler 检查 profile.provider === 'HUMAN_RELAY'
        ↓
3. 调用 executeHumanRelayLLMNode()
        ↓
4. executeHumanRelay() 执行：
   a. 创建 HumanRelayRequest
   b. 触发 HUMAN_RELAY_REQUESTED 事件
   c. 调用 HumanRelayHandler.handle()
   d. 触发 HUMAN_RELAY_RESPONDED 事件
   e. 转换为 LLMMessage
   f. 触发 HUMAN_RELAY_PROCESSED 事件
        ↓
5. 返回结果给工作流继续执行
```

## 文件位置

| 组件 | 文件路径 |
|------|---------|
| 类型定义 | `packages/types/src/human-relay.ts` |
| 事件类型 | `packages/types/src/events/interaction-events.ts` |
| 执行处理器 | `sdk/graph/execution/handlers/human-relay-handler.ts` |
| LLM 客户端 | `sdk/core/llm/clients/human-relay-client.ts` |
| 客户端工厂 | `sdk/core/llm/client-factory.ts` |
| LLM 包装器 | `sdk/core/llm/wrapper.ts` |
| API 资源 | `sdk/api/graph/resources/human-relay/human-relay-resource-api.ts` |
| CLI Handler | `apps/cli-app/src/handlers/cli-human-relay-handler.ts` |
| CLI 适配器 | `apps/cli-app/src/adapters/human-relay-adapter.ts` |

## 与 Tool Approval 的区别

```
┌─────────────────────────────────────────────────────────────┐
│                    Tool Approval                            │
│  - 询问用户是否批准工具调用                                   │
│  - 使用 USER_INTERACTION_REQUESTED 事件                     │
│  - 返回布尔值/审批结果                                        │
│  - 在 llm-execution-coordinator.ts 中实现                   │
└─────────────────────────────────────────────────────────────┘
                              
┌─────────────────────────────────────────────────────────────┐
│                    Human Relay                              │
│  - 用人工输入替代 LLM API 调用                               │
│  - 使用 HUMAN_RELAY_REQUESTED 事件                          │
│  - 返回消息内容作为 LLM 响应                                  │
│  - 在 human-relay-handler.ts 中实现                         │
└─────────────────────────────────────────────────────────────┘
```

## 设计原则

1. **接口一致性**：HumanRelayClient 实现 LLMClient 接口，可以无缝替换任何 LLM Provider
2. **无状态设计**：执行函数无状态，便于测试和复用
3. **事件驱动**：通过事件通知外部系统，支持扩展
4. **超时控制**：防止无限等待，提高健壮性
5. **取消机制**：支持用户取消操作
