# Human Relay 功能实现分析

## 1. 功能概述

**Human Relay（人工中继）** 是 Modular Agent Framework 中的一种特殊 LLM Provider，其核心功能是**用人工输入替代 LLM API 调用**。这使得在需要人工介入的场景下，可以将外部 LLM（如 Web 版 ChatGPT、Claude 等）的响应通过人工输入的方式集成到工作流中。

### 1.1 核心特性

| 特性 | 说明 |
|------|------|
| **本质** | 用人工输入替代 LLM API 调用 |
| **触发方式** | 当 LLM Profile 的 provider 设置为 `HUMAN_RELAY` 时触发 |
| **输入内容** | 人工输入的回复消息（模拟 LLM 响应） |
| **返回值** | 作为 LLM 响应继续工作流执行 |
| **事件类型** | `HUMAN_RELAY_*` 系列事件 |

### 1.2 与 Tool Approval 的区别

| 特性 | Human Relay | Tool Approval |
|------|-------------|---------------|
| **目的** | 用人工输入替代 LLM 响应 | 人工审批是否允许工具执行 |
| **触发时机** | LLM 节点执行时 | 工具调用前 |
| **输入内容** | 人工输入的回复消息 | 审批结果（approve/reject）|
| **返回值** | 作为 LLM 响应继续工作流 | 决定是否执行工具 |
| **事件类型** | `HUMAN_RELAY_*` | `USER_INTERACTION_*` |

## 2. 架构层次分析

Human Relay 功能分布在多个架构层次，各层职责清晰：

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

## 3. 核心组件详细分析

### 3.1 类型定义层

**文件位置**: `packages/types/src/human-relay.ts`

核心类型定义包括：

```typescript
// HumanRelay 请求
export interface HumanRelayRequest {
  requestId: ID;
  messages: LLMMessage[];      // 对话历史
  prompt: string;              // 给用户的提示
  timeout: number;             // 超时时间（毫秒）
  metadata?: Metadata;
}

// HumanRelay 响应
export interface HumanRelayResponse {
  requestId: ID;
  content: string;             // 人工输入内容
  timestamp: number;
}

// HumanRelay 处理器接口（应用层实现）
export interface HumanRelayHandler {
  handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse>;
}

// HumanRelay 上下文
export interface HumanRelayContext {
  threadId: ID;
  workflowId: ID;
  nodeId: ID;
  getVariable(variableName: string): unknown;
  setVariable(variableName: string, value: unknown): Promise<void>;
  getVariables(): Record<string, unknown>;
  timeout: number;
  cancelToken: { cancelled: boolean; cancel(): void };
}
```

### 3.2 事件系统

**文件位置**: `packages/types/src/events/interaction-events.ts`

Human Relay 使用以下事件：

| 事件名称 | 触发时机 | 说明 |
|---------|---------|------|
| `HUMAN_RELAY_REQUESTED` | 请求开始时 | 通知应用层显示输入界面 |
| `HUMAN_RELAY_RESPONDED` | 用户输入完成 | 通知应用层输入已接收 |
| `HUMAN_RELAY_PROCESSED` | 处理完成 | 通知应用层处理结果 |
| `HUMAN_RELAY_FAILED` | 处理失败 | 通知应用层错误信息 |

事件构建器位于：`sdk/core/utils/event/builders/interaction-events.ts`

### 3.3 HumanRelayClient

**文件位置**: `sdk/core/llm/clients/human-relay-client.ts`

`HumanRelayClient` 实现了 `LLMClient` 接口，使 Human Relay 可以无缝替换任何 LLM Provider：

**设计特点**：
- **无状态设计**：每次请求独立处理
- **标准接口**：实现 `LLMClient` 接口，与 OpenAI、Anthropic 等客户端一致
- **超时控制**：支持请求超时机制（默认 5 分钟）
- **取消机制**：支持取消操作
- **流式模拟**：虽然本质是同步人工输入，但模拟流式响应以保持接口一致性

**核心方法**：

```typescript
// 非流式生成
async generate(request: LLMRequest): Promise<LLMResult>

// 流式生成（模拟）
async *generateStream(request: LLMRequest): AsyncIterable<LLMResult>
```

### 3.4 ClientFactory 集成

**文件位置**: `sdk/core/llm/client-factory.ts`

`ClientFactory` 负责创建 `HumanRelayClient` 实例：

```typescript
export class ClientFactory {
  private humanRelayHandler?: HumanRelayHandler;
  private humanRelayContextProvider?: () => HumanRelayContext;

  // 注册 HumanRelayHandler
  setHumanRelayHandler(handler: HumanRelayHandler): void

  // 注册上下文提供者
  setHumanRelayContextProvider(provider: () => HumanRelayContext): void

  // 根据 provider 创建对应客户端
  createClient(profile: LLMProfile): LLMClient
}
```

当 `profile.provider === 'HUMAN_RELAY'` 时，工厂会创建 `HumanRelayClient` 实例。

### 3.5 执行处理器

**文件位置**: `sdk/graph/execution/handlers/human-relay-handler.ts`

提供无状态的 Human Relay 执行函数：

| 函数 | 职责 |
|------|------|
| `createHumanRelayRequest()` | 创建 HumanRelayRequest 对象 |
| `createHumanRelayContext()` | 创建 HumanRelayContext 对象 |
| `emitHumanRelayRequestedEvent()` | 触发请求事件 |
| `emitHumanRelayRespondedEvent()` | 触发响应事件 |
| `emitHumanRelayProcessedEvent()` | 触发处理完成事件 |
| `emitHumanRelayFailedEvent()` | 触发失败事件 |
| `getHumanInput()` | 获取人工输入（支持超时和取消）|
| `convertToLLMMessage()` | 转换为 LLM 消息 |
| `executeHumanRelay()` | 主执行函数，协调整个流程 |

**执行流程**：

```
1. 创建工作流执行到 LLM 节点
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

### 3.6 API 资源层

**文件位置**: `sdk/api/graph/resources/human-relay/human-relay-resource-api.ts`

`HumanRelayResourceAPI` 提供：

- **Handler 管理**：注册、获取、清除 HumanRelayHandler
- **配置管理**：CRUD 操作 HumanRelayConfig
- **事件订阅**：提供 `onRelayRequested`、`onRelayResponded` 等事件订阅方法
- **Handler 同步**：自动同步 Handler 到 LLMWrapper 的 ClientFactory

### 3.7 CLI 应用实现

**文件位置**: 
- `apps/cli-app/src/handlers/cli-human-relay-handler.ts`
- `apps/cli-app/src/adapters/human-relay-adapter.ts`

`CLIHumanRelayHandler` 是 `HumanRelayHandler` 的 CLI 实现：

- 使用 `readline` 模块读取用户输入
- 支持多行输入（空行结束）
- 显示对话历史和当前提示
- 支持 Ctrl+C 取消操作

## 4. 配置文件

**文件位置**: `configs/llms/provider/human-relay/`

Human Relay 支持通过 TOML 配置文件进行配置：

```toml
# common.toml - 通用配置
provider = "human-relay"
model_type = "human-relay"
mode = "single"                    # single 或 multi
default_timeout = 300              # 默认超时时间（秒）
max_history_length = 50            # 多轮对话最大历史长度

# 前端交互配置
[frontend]
type = "tui"                       # tui, web, api
auto_detect = true

# TUI配置
[frontend.tui]
prompt_style = "highlight"
show_history = true

# Web配置
[frontend.web]
port = 8080
host = "localhost"
```

## 5. 关键设计决策

### 5.1 接口一致性

`HumanRelayClient` 实现 `LLMClient` 接口，可以无缝替换任何 LLM Provider，无需修改工作流执行逻辑。

### 5.2 无状态设计

执行函数采用无状态设计，便于测试和复用。所有状态通过参数传递。

### 5.3 事件驱动

通过事件通知外部系统，支持扩展。应用层可以订阅事件来实现自定义界面。

### 5.4 超时与取消

- **超时控制**：防止无限等待，提高健壮性
- **取消机制**：支持用户取消操作

### 5.5 流式响应模拟

虽然 Human Relay 本质是同步人工输入，但为了保持与 LLMClient 接口的一致性，`generateStream` 方法模拟了流式响应（先返回部分内容，再返回完成标记）。

## 6. 文件位置汇总

| 组件 | 文件路径 |
|------|---------|
| 类型定义 | `packages/types/src/human-relay.ts` |
| 事件类型 | `packages/types/src/events/interaction-events.ts` |
| 事件构建器 | `sdk/core/utils/event/builders/interaction-events.ts` |
| LLM 客户端 | `sdk/core/llm/clients/human-relay-client.ts` |
| 客户端工厂 | `sdk/core/llm/client-factory.ts` |
| 执行处理器 | `sdk/graph/execution/handlers/human-relay-handler.ts` |
| LLM 节点处理器 | `sdk/graph/execution/handlers/node-handlers/llm-handler.ts` |
| API 资源 | `sdk/api/graph/resources/human-relay/human-relay-resource-api.ts` |
| CLI Handler | `apps/cli-app/src/handlers/cli-human-relay-handler.ts` |
| CLI 适配器 | `apps/cli-app/src/adapters/human-relay-adapter.ts` |
| 配置文件 | `configs/llms/provider/human-relay/*.toml` |

## 7. 使用场景

Human Relay 适用于以下场景：

1. **Web LLM 集成**：将 Web 版 ChatGPT、Claude 等的响应集成到工作流
2. **人工审核**：需要人工确认或修改 LLM 响应的场景
3. **外部知识注入**：需要人工输入特定领域知识的场景
4. **测试调试**：开发和测试阶段模拟 LLM 响应
5. **合规要求**：某些场景下必须使用人工输入代替自动 LLM 调用

## 8. 扩展建议

如需扩展 Human Relay 功能，可考虑：

1. **Web 界面实现**：开发 WebHumanRelayHandler 提供 Web 界面
2. **文件输入支持**：支持从文件读取人工输入
3. **多模态支持**：扩展支持图片、音频等多模态输入
4. **历史记录持久化**：将人工输入历史持久化存储
