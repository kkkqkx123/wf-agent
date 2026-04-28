# Human Relay 实现细节文档

## 概述

本文档详细描述 Human Relay 功能的实现细节，包括核心组件的实现逻辑和关键代码。

## 核心实现文件

### 1. HumanRelayClient 实现

**文件位置**: `sdk/core/llm/clients/human-relay-client.ts`

#### 1.1 类定义

```typescript
export class HumanRelayClient implements LLMClient {
  private profile: LLMProfile;
  private handler: HumanRelayHandler;
  private defaultTimeout: number;

  constructor(profile: LLMProfile, config: HumanRelayClientConfig) {
    this.profile = profile;
    this.handler = config.handler;
    this.defaultTimeout = config.defaultTimeout || 300000;
    this.validateConfig();
  }
}
```

#### 1.2 generate 方法

非流式生成实现：

```typescript
async generate(request: LLMRequest): Promise<LLMResult> {
  const startTime = now();
  const requestId = generateId();

  try {
    // 1. 构建 HumanRelayRequest
    const humanRelayRequest = this.buildHumanRelayRequest(request, requestId);

    // 2. 构建 HumanRelayContext
    const context = this.buildHumanRelayContext(request);

    // 3. 调用 handler 获取人工输入
    const response = await this.handler.handle(humanRelayRequest, context);

    // 4. 转换为 LLMResult
    return this.buildLLMResult(response, request, startTime);
  } catch (error) {
    throw new ExecutionError(
      `HumanRelay execution failed: ${error instanceof Error ? error.message : String(error)}`,
      requestId
    );
  }
}
```

#### 1.3 generateStream 方法

流式生成模拟实现：

```typescript
async *generateStream(request: LLMRequest): AsyncIterable<LLMResult> {
  const startTime = now();
  const requestId = generateId();

  // 获取人工输入
  const response = await this.handler.handle(humanRelayRequest, context);

  // 模拟流式响应 - 先返回部分内容
  yield {
    id: response.requestId,
    model: this.profile.model,
    content: response.content,
    message: { role: "assistant", content: response.content },
    finishReason: "null",
    duration: now() - startTime,
    metadata: { source: "human-relay", streaming: true },
  };

  // 再返回完成标记
  yield {
    ...finalResult,
    finishReason: "stop",
  };
}
```

#### 1.4 构建 HumanRelayRequest

```typescript
private buildHumanRelayRequest(request: LLMRequest, requestId: string): HumanRelayRequest {
  // 从 messages 中提取最后一条用户消息作为 prompt
  const messages = request.messages || [];
  let lastUserMessage: { role: string; content: unknown } | undefined;
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i]?.role === "user") {
      lastUserMessage = messages[i];
      break;
    }
  }
  const prompt = lastUserMessage?.content || "Please provide your input:";

  return {
    requestId,
    messages: request.messages || [],
    prompt: typeof prompt === "string" ? prompt : JSON.stringify(prompt),
    timeout: this.defaultTimeout,
    metadata: {
      profileId: this.profile.id,
      model: this.profile.model,
      temperature: request.parameters?.["temperature"],
      maxTokens: request.parameters?.["maxTokens"],
    },
  };
}
```

### 2. ClientFactory 集成

**文件位置**: `sdk/core/llm/client-factory.ts`

#### 2.1 添加 HumanRelayHandler 支持

```typescript
export class ClientFactory {
  private clientCache: Map<string, LLMClient> = new Map();
  private humanRelayHandler?: HumanRelayHandler;

  /**
   * 设置 HumanRelayHandler
   */
  setHumanRelayHandler(handler: HumanRelayHandler): void {
    this.humanRelayHandler = handler;
    // 清除缓存的 HumanRelayClient，以便使用新的 handler
    this.clearClientCacheByProvider("HUMAN_RELAY");
  }

  /**
   * 根据 provider 清除客户端缓存
   */
  private clearClientCacheByProvider(provider: string): void {
    for (const [key, client] of this.clientCache.entries()) {
      if (key.includes(provider)) {
        this.clientCache.delete(key);
      }
    }
  }
}
```

#### 2.2 创建 HumanRelayClient

```typescript
private createClientByProvider(profile: LLMProfile): LLMClient {
  switch (profile.provider) {
    // ... 其他 case ...

    case "HUMAN_RELAY":
      if (!this.humanRelayHandler) {
        throw new ConfigurationError(
          "HumanRelayHandler not registered. Please call setHumanRelayHandler() first.",
          "provider",
          { provider: profile.provider }
        );
      }
      return new HumanRelayClient(profile, {
        handler: this.humanRelayHandler,
        defaultTimeout: (profile.parameters?.["timeout"] as number) || 300000,
      });

    // ... 其他 case ...
  }
}
```

### 3. LLMWrapper 集成

**文件位置**: `sdk/core/llm/wrapper.ts`

```typescript
export class LLMWrapper {
  private clientFactory: ClientFactory;

  /**
   * 设置 Human Relay Handler
   */
  setHumanRelayHandler(handler: HumanRelayHandler): void {
    this.clientFactory.setHumanRelayHandler(handler);
  }
}
```

### 4. HumanRelayResourceAPI 实现

**文件位置**: `sdk/api/graph/resources/human-relay/human-relay-resource-api.ts`

#### 4.1 注册 Handler

```typescript
export class HumanRelayResourceAPI extends GenericResourceAPI<...> {
  private humanRelayHandler?: HumanRelayHandler;

  /**
   * 注册 Human Relay Handler
   * 同时同步到 ClientFactory
   */
  registerHandler(handler: HumanRelayHandler): void {
    this.humanRelayHandler = handler;

    // 同步到 LLMWrapper 的 ClientFactory
    const llmWrapper = this.dependencies.getLLMWrapper?.();
    if (llmWrapper) {
      llmWrapper.setHumanRelayHandler(handler);
    }
  }
}
```

#### 4.2 事件订阅

```typescript
/**
 * 订阅 Human Relay 请求事件
 */
onRelayRequested(listener: (event: HumanRelayRequestedEvent) => void): void {
  this.dependencies.getEventManager().on("HUMAN_RELAY_REQUESTED", listener);
}

/**
 * 订阅 Human Relay 响应事件
 */
onRelayResponded(listener: (event: HumanRelayRespondedEvent) => void): void {
  this.dependencies.getEventManager().on("HUMAN_RELAY_RESPONDED", listener);
}

/**
 * 订阅 Human Relay 处理完成事件
 */
onRelayProcessed(listener: (event: HumanRelayProcessedEvent) => void): void {
  this.dependencies.getEventManager().on("HUMAN_RELAY_PROCESSED", listener);
}

/**
 * 订阅 Human Relay 失败事件
 */
onRelayFailed(listener: (event: HumanRelayFailedEvent) => void): void {
  this.dependencies.getEventManager().on("HUMAN_RELAY_FAILED", listener);
}
```

### 5. human-relay-handler.ts 实现

**文件位置**: `sdk/graph/execution/handlers/human-relay-handler.ts`

#### 5.1 执行函数

```typescript
export async function executeHumanRelay(
  messages: LLMMessage[],
  prompt: string,
  timeout: number,
  threadEntity: ThreadEntity,
  eventManager: EventManager,
  humanRelayHandler: HumanRelayHandler,
  nodeId: string
): Promise<HumanRelayExecutionResult> {
  const requestId = generateId();
  const startTime = now();

  const task: HumanRelayTask = {
    messages,
    prompt,
    timeout,
    threadEntity,
    requestId,
    nodeId,
  };

  try {
    // 1. 创建 HumanRelay 请求
    const request = createHumanRelayRequest(task);

    // 2. 触发 HUMAN_RELAY_REQUESTED 事件
    await emitHumanRelayRequestedEvent(task, request, eventManager);

    // 3. 创建 HumanRelay 上下文
    const context = createHumanRelayContext(task, eventManager);

    // 4. 调用应用层处理器获取人工输入
    const response = await getHumanInput(task, request, context, humanRelayHandler);

    // 5. 触发 HUMAN_RELAY_RESPONDED 事件
    await emitHumanRelayRespondedEvent(task, response, eventManager);

    // 6. 将人工输入转换为 LLM 消息
    const message = convertToLLMMessage(task, response);

    // 7. 触发 HUMAN_RELAY_PROCESSED 事件
    const executionTime = diffTimestamp(startTime, now());
    await emitHumanRelayProcessedEvent(task, message, executionTime, eventManager);

    return { requestId, message, executionTime };
  } catch (error) {
    // 触发 HUMAN_RELAY_FAILED 事件
    await emitHumanRelayFailedEvent(task, getErrorOrNew(error), eventManager);
    throw error;
  }
}
```

#### 5.2 获取人工输入（带超时和取消）

```typescript
export async function getHumanInput(
  task: HumanRelayTask,
  request: HumanRelayRequest,
  context: HumanRelayContext,
  handler: HumanRelayHandler
): Promise<HumanRelayResponse> {
  // 超时控制
  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => {
      reject(new Error(`HumanRelay timeout after ${request.timeout}ms`));
    }, request.timeout);
  });

  // 取消控制
  const cancelPromise = new Promise<never>((_, reject) => {
    const checkCancel = setInterval(() => {
      if (context.cancelToken.cancelled) {
        clearInterval(checkCancel);
        reject(new Error("HumanRelay cancelled"));
      }
    }, 100);
  });

  try {
    // 竞争：人工输入、超时、取消
    return await Promise.race([
      handler.handle(request, context),
      timeoutPromise,
      cancelPromise,
    ]);
  } finally {
    // 清理取消检查
    context.cancelToken.cancel();
  }
}
```

#### 5.3 转换为 LLM 消息

```typescript
export function convertToLLMMessage(
  task: HumanRelayTask,
  response: HumanRelayResponse
): LLMMessage {
  return {
    role: "assistant" as MessageRole,
    content: response.content,
    id: task.requestId,
    timestamp: response.timestamp,
    metadata: {
      source: "human-relay",
      nodeId: task.nodeId,
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.threadEntity.getThreadId(),
    },
  };
}
```

### 6. CLIHumanRelayHandler 实现

**文件位置**: `apps/cli-app/src/handlers/cli-human-relay-handler.ts`

```typescript
export class CLIHumanRelayHandler implements HumanRelayHandler {
  async handle(
    request: HumanRelayRequest,
    context: HumanRelayContext
  ): Promise<HumanRelayResponse> {
    // 1. 显示提示信息
    output.infoLog("\n╔════════════════════════════════════════════════════════════╗");
    output.infoLog("║                 HUMAN RELAY REQUEST                        ║");
    output.infoLog("╚════════════════════════════════════════════════════════════╝");

    output.infoLog(`\nRequest ID: ${request.requestId}`);
    output.infoLog(`Timeout: ${request.timeout}ms`);

    // 2. 显示对话历史
    if (request.messages.length > 0) {
      output.infoLog("\n--- Conversation History ---");
      for (const msg of request.messages) {
        const role = msg.role.toUpperCase();
        const content =
          typeof msg.content === "string"
            ? msg.content
            : JSON.stringify(msg.content);
        const truncated =
          content.length > 200 ? content.substring(0, 200) + "..." : content;
        output.infoLog(`[${role}]: ${truncated}`);
      }
    }

    // 3. 显示当前提示
    output.infoLog("\n--- Current Prompt ---");
    output.infoLog(request.prompt);
    output.infoLog(
      "\n--- Please Enter Your Response (Empty line to finish, Ctrl+C to cancel) ---"
    );

    // 4. 读取用户输入
    const content = await this.promptUser();

    // 5. 返回响应
    return {
      requestId: request.requestId,
      content,
      timestamp: Date.now(),
    };
  }

  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });

      let content = "";
      let isFirstLine = true;

      rl.setPrompt("> ");
      rl.prompt();

      rl.on("line", (line) => {
        if (line.trim() === "" && !isFirstLine) {
          // 空行且已有内容，结束输入
          rl.close();
          resolve(content.trim());
        } else {
          if (!isFirstLine) {
            content += "\n";
          }
          content += line;
          isFirstLine = false;
          rl.prompt();
        }
      });

      rl.on("SIGINT", () => {
        rl.close();
        reject(new Error("User cancelled"));
      });

      rl.on("close", () => {
        if (isFirstLine) {
          resolve("(No response provided)");
        }
      });
    });
  }
}
```

### 7. CLI 应用初始化

**文件位置**: `apps/cli-app/src/index.ts`

```typescript
import { CLIHumanRelayHandler } from "./handlers/cli-human-relay-handler.js";

// 在 preAction hook 中
program.hook("preAction", async thisCommand => {
  // ... 其他初始化 ...

  // 4. 加载配置并初始化 SDK
  const config = await loadConfig();
  const sdk = getSDK({
    debug: options.debug,
    logLevel: options.debug ? "debug" : options.verbose ? "info" : "warn",
    presets: config.presets,
  });

  // 5. 注册 Human Relay Handler
  const humanRelayHandler = new CLIHumanRelayHandler();
  sdk.humanRelay.registerHandler(humanRelayHandler);
});
```

## 关键设计决策

### 1. 为什么 HumanRelayClient 要实现 LLMClient 接口？

- **无缝集成**：可以像普通 LLM 一样使用，无需修改现有代码
- **统一接口**：所有 LLM Provider 使用相同的调用方式
- **易于替换**：可以在配置中随时切换 Human Relay 和真实 LLM

### 2. 为什么需要同步 Handler 到 ClientFactory？

- **延迟创建**：ClientFactory 在需要时才创建客户端实例
- **缓存机制**：客户端实例会被缓存，需要清除缓存以使用新的 Handler
- **一致性**：确保 API 层和 Core 层使用相同的 Handler

### 3. 为什么流式响应是模拟的？

- **本质限制**：Human Relay 本质上是同步的人工输入
- **接口兼容**：为了兼容 LLMClient 接口，需要支持流式方法
- **实现方式**：先返回部分内容，再返回完成标记

### 4. 超时和取消机制如何工作？

- **Promise.race**：同时监听 handler、超时和取消三个 Promise
- **超时**：使用 setTimeout 创建超时 Promise
- **取消**：使用 setInterval 定期检查 cancelToken 状态
- **清理**：finally 块中清理取消检查定时器
