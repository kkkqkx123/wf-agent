# LLM Clients 架构设计文档

## 概述

本文档描述 `src/infrastructure/llm/clients` 模块的架构设计和主要决策。

## 架构原则

### 1. 配置驱动，无硬编码

**决策：** 所有客户端必须从配置文件读取必需配置，禁止硬编码默认值。

**理由：**
- 避免调试时使用错误的默认配置
- 强制用户明确配置，提高可维护性
- 便于环境切换和部署

**实现：**
```typescript
// ✅ 正确：从配置读取
const apiKey = configManager.get('llm.openai.apiKey');
if (!apiKey) {
  throw new Error('OpenAI API密钥未配置。请在配置文件中设置 llm.openai.apiKey。');
}

// ❌ 错误：硬编码默认值
const apiKey = configManager.get('llm.openai.apiKey', 'default-key');
```

### 2. 编译时类型安全

**决策：** 使用抽象方法强制子类实现关键功能。

**理由：**
- TypeScript 编译器在编译时检测未实现的方法
- 避免运行时才发现缺失实现
- 提高代码可靠性

**实现：**
```typescript
export abstract class BaseLLMClient {
  // 抽象方法：子类必须实现
  protected abstract parseStreamResponse(response: any, request: LLMRequest): Promise<AsyncIterable<LLMResponse>>;
}
```

### 3. 策略模式处理差异

**决策：** 使用 ParameterMapper 和 EndpointStrategy 处理提供商差异。

**理由：**
- 非流式响应格式相对统一，可以通过策略模式统一处理
- 流式响应格式差异巨大，需要子类实现
- 符合开闭原则，易于扩展新提供商

**实现：**
```typescript
// 非流式：使用 ParameterMapper（基类实现）
return this.providerConfig.parameterMapper.mapFromResponse(response.data, request);

// 流式：子类实现（格式差异大）
protected override async parseStreamResponse(response: any, request: LLMRequest): Promise<AsyncIterable<LLMResponse>> {
  // 处理特定提供商的流式格式
}
```

## 核心组件

### BaseLLMClient（基类）

**职责：**
- 提供通用的非流式响应处理
- 定义抽象方法强制子类实现
- 提供通用功能（token计算、成本计算等）

**抽象方法：**
```typescript
protected abstract getSupportedModelsList(): string[];
public abstract getModelConfig(): ModelConfig;
protected abstract parseStreamResponse(response: any, request: LLMRequest): Promise<AsyncIterable<LLMResponse>>;
```

**通用方法：**
```typescript
public async generateResponse(request: LLMRequest): Promise<LLMResponse> {
  // 1. 参数映射
  const providerRequest = this.providerConfig.parameterMapper.mapToProvider(request, this.providerConfig);
  
  // 2. 构建端点和头部
  const endpoint = this.providerConfig.endpointStrategy.buildEndpoint(this.providerConfig, providerRequest);
  const headers = this.providerConfig.endpointStrategy.buildHeaders(this.providerConfig);
  
  // 3. 发送请求
  const response = await this.httpClient.post(endpoint, providerRequest, { headers });
  
  // 4. 转换响应
  return this.providerConfig.parameterMapper.mapFromResponse(response.data, request);
}
```

### 具体客户端实现

#### OpenAI Chat Client
- **端点：** `POST /v1/chat/completions`
- **流式格式：** SSE `data: {...}`
- **配置路径：** `llm.openai.*`

#### OpenAI Response Client
- **端点：** `POST /v1/responses`
- **流式格式：** SSE `data: {...}`
- **配置路径：** `llm.openai-response.*`

#### Gemini Client（原生）
- **端点：** `POST /v1beta/models/{model}:generateContent`
- **流式格式：** JSON对象 `{candidates: [...]}`
- **配置路径：** `llm.gemini.*`

#### Gemini Client（OpenAI兼容）
- **端点：** `POST /v1beta/openai/chat/completions`
- **流式格式：** SSE `data: {...}`
- **配置路径：** `llm.gemini-openai.*`

#### Anthropic Client
- **端点：** `POST /v1/messages`
- **流式格式：** 事件类型 `{type: "content_block_delta"}`
- **配置路径：** `llm.anthropic.*`

#### Mock Client
- **用途：** 测试和开发
- **特殊实现：** 完全覆盖 `generateResponse` 和 `generateResponseStream`
- **配置路径：** `llm.mock.*`

#### HumanRelay Client
- **用途：** 人工介入
- **特殊实现：** 完全覆盖 `generateResponse` 和 `generateResponseStream`
- **配置路径：** `llm.human-relay.*`

## 配置管理

### 必需配置项

每个客户端必须配置以下项：

```typescript
// 1. API密钥
llm.{provider}.apiKey

// 2. 默认模型
llm.{provider}.defaultModel

// 3. 支持的模型列表
llm.{provider}.supportedModels

// 4. 模型详细配置
llm.{provider}.models.{model} = {
  maxTokens: number,
  contextWindow: number,
  temperature: number,
  topP: number,
  promptTokenPrice: number,
  completionTokenPrice: number,
  // 可选字段
  supportsStreaming?: boolean,
  supportsTools?: boolean,
  supportsImages?: boolean,
  supportsAudio?: boolean,
  supportsVideo?: boolean,
  metadata?: object
}
```

### 配置验证

所有客户端在构造函数中验证必需配置：

```typescript
// 验证 API 密钥
if (!apiKey) {
  throw new Error('{provider} API密钥未配置。请在配置文件中设置 llm.{provider}.apiKey。');
}

// 验证默认模型
if (!defaultModel) {
  throw new Error('{provider}默认模型未配置。请在配置文件中设置 llm.{provider}.defaultModel。');
}

// 验证支持的模型列表
if (!supportedModels || !Array.isArray(supportedModels) || supportedModels.length === 0) {
  throw new Error('{provider}支持的模型列表未配置。请在配置文件中设置 llm.{provider}.supportedModels。');
}

// 验证模型配置
const requiredFields = ['maxTokens', 'contextWindow', 'temperature', 'topP', 'promptTokenPrice', 'completionTokenPrice'];
for (const field of requiredFields) {
  if (config[field] === undefined || config[field] === null) {
    throw new Error(`{provider}模型 ${model} 缺少必需配置字段: ${field}`);
  }
}
```

## 流式处理设计

### 为什么流式处理需要抽象？

**原因：**
1. **格式差异巨大**
   - OpenAI: SSE `data: {...}`
   - Gemini: JSON对象 `{candidates: [...]}`
   - Anthropic: 事件类型 `{type: "content_block_delta"}`

2. **无法通过策略模式统一处理**
   - ParameterMapper 无法处理流式响应的增量解析
   - 需要逐块解析和转换

3. **编译时检查**
   - 强制子类实现，避免运行时错误
   - TypeScript 编译器可以检测未实现的方法

### 流式处理实现模式

```typescript
protected override async parseStreamResponse(response: any, request: LLMRequest): Promise<AsyncIterable<LLMResponse>> {
  async function* streamGenerator() {
    // 1. 遍历流式数据
    for await (const chunk of response.data) {
      // 2. 解析特定提供商的格式
      const data = parseProviderFormat(chunk);
      
      // 3. 转换为统一的 LLMResponse
      yield LLMResponse.create(...);
    }
    
    // 4. 发送最终块
    yield LLMResponse.create(..., finish_reason: 'stop');
  }
  
  return streamGenerator();
}
```

## 非流式处理设计

### 为什么非流式处理不需要抽象？

**原因：**
1. **ParameterMapper 已经处理格式差异**
   - `mapToProvider` 转换请求格式
   - `mapFromResponse` 转换响应格式
   - 策略模式的正确应用

2. **大多数客户端可以使用基类实现**
   - OpenAI、Gemini、Anthropic 都可以使用基类实现
   - 只有特殊客户端需要覆盖

3. **特殊客户端已正确覆盖**
   - MockClient: 完全覆盖，模拟响应
   - HumanRelayClient: 完全覆盖，委托给应用层服务

### 非流式处理实现

```typescript
// 基类实现（大多数客户端使用）
public async generateResponse(request: LLMRequest): Promise<LLMResponse> {
  const providerRequest = this.providerConfig.parameterMapper.mapToProvider(request, this.providerConfig);
  const endpoint = this.providerConfig.endpointStrategy.buildEndpoint(this.providerConfig, providerRequest);
  const headers = this.providerConfig.endpointStrategy.buildHeaders(this.providerConfig);
  const response = await this.httpClient.post(endpoint, providerRequest, { headers });
  return this.providerConfig.parameterMapper.mapFromResponse(response.data, request);
}

// 特殊客户端覆盖
public override async generateResponse(request: LLMRequest): Promise<LLMResponse> {
  // 自定义实现
}
```

## 扩展指南

### 添加新的 LLM 提供商

1. **创建客户端类**
```typescript
@injectable()
export class NewProviderClient extends BaseLLMClient {
  constructor(
    @inject(TYPES.HttpClient) httpClient: HttpClient,
    @inject(TYPES.TokenBucketLimiter) rateLimiter: TokenBucketLimiter,
    @inject(TYPES.TokenCalculator) tokenCalculator: TokenCalculator,
    @inject(TYPES.ConfigLoadingModule) configManager: ConfigLoadingModule
  ) {
    // 1. 创建功能支持配置
    const featureSupport = new BaseFeatureSupport();
    // ... 设置功能支持
    
    // 2. 从配置读取必需配置
    const apiKey = configManager.get('llm.new-provider.apiKey');
    const defaultModel = configManager.get('llm.new-provider.defaultModel');
    const supportedModels = configManager.get('llm.new-provider.supportedModels');
    
    // 3. 验证配置
    if (!apiKey) {
      throw new Error('NewProvider API密钥未配置...');
    }
    // ... 其他验证
    
    // 4. 创建 ProviderConfig
    const providerConfig = new ProviderConfigBuilder()
      .name('NewProvider')
      .apiType(ApiType.NATIVE)
      .baseURL('https://api.new-provider.com')
      .apiKey(apiKey)
      .endpointStrategy(new NewProviderEndpointStrategy())
      .parameterMapper(new NewProviderParameterMapper())
      .featureSupport(featureSupport)
      .defaultModel(defaultModel)
      .supportedModels(supportedModels)
      .build();
    
    super(httpClient, rateLimiter, tokenCalculator, configManager, providerConfig);
  }
  
  // 5. 实现抽象方法
  protected override getSupportedModelsList(): string[] {
    return this.providerConfig.supportedModels;
  }
  
  public override getModelConfig(): ModelConfig {
    const model = this.providerConfig.defaultModel;
    const configs = this.configLoadingModule.get<Record<string, any>>('llm.new-provider.models', {});
    const config = configs[model];
    
    if (!config) {
      throw new Error(`NewProvider模型配置未找到: ${model}`);
    }
    
    return ModelConfig.create({
      model,
      provider: 'new-provider',
      maxTokens: config.maxTokens,
      contextWindow: config.contextWindow,
      temperature: config.temperature,
      topP: config.topP,
      frequencyPenalty: config.frequencyPenalty ?? 0.0,
      presencePenalty: config.presencePenalty ?? 0.0,
      costPer1KTokens: {
        prompt: config.promptTokenPrice,
        completion: config.completionTokenPrice
      },
      supportsStreaming: config.supportsStreaming ?? true,
      supportsTools: config.supportsTools ?? true,
      supportsImages: config.supportsImages ?? false,
      supportsAudio: config.supportsAudio ?? false,
      supportsVideo: config.supportsVideo ?? false,
      metadata: config.metadata ?? {}
    });
  }
  
  // 6. 实现流式处理
  protected override async parseStreamResponse(response: any, request: LLMRequest): Promise<AsyncIterable<LLMResponse>> {
    async function* streamGenerator() {
      // 解析 NewProvider 的流式格式
      for await (const chunk of response.data) {
        const data = JSON.parse(chunk.toString());
        // 转换为 LLMResponse
        yield LLMResponse.create(...);
      }
      
      // 发送最终块
      yield LLMResponse.create(..., finish_reason: 'stop');
    }
    
    return streamGenerator();
  }
}
```

2. **创建 ParameterMapper**
```typescript
export class NewProviderParameterMapper implements ParameterMapper {
  mapToProvider(request: LLMRequest, config: ProviderConfig): any {
    // 转换为 NewProvider 格式
    return {
      model: request.model,
      messages: request.messages.map(msg => ({
        role: msg.getRole(),
        content: msg.getContent()
      })),
      // ... 其他参数
    };
  }
  
  mapFromResponse(response: any, request: LLMRequest): LLMResponse {
    // 转换为统一格式
    return LLMResponse.create(
      request.requestId,
      request.model,
      [{
        index: 0,
        message: LLMMessage.createAssistant(response.content),
        finish_reason: response.finish_reason
      }],
      {
        promptTokens: response.usage.prompt_tokens,
        completionTokens: response.usage.completion_tokens,
        totalTokens: response.usage.total_tokens
      },
      response.finish_reason,
      0
    );
  }
}
```

3. **创建 EndpointStrategy**
```typescript
export class NewProviderEndpointStrategy implements EndpointStrategy {
  buildEndpoint(config: ProviderConfig, request: any): string {
    return `${config.baseURL}/v1/chat/completions`;
  }
  
  buildHeaders(config: ProviderConfig): Record<string, string> {
    return {
      'Authorization': `Bearer ${config.apiKey}`,
      'Content-Type': 'application/json'
    };
  }
}
```

4. **注册到 DI 容器**
```typescript
// 在 DI 配置中
container.bind<NewProviderClient>(TYPES.NewProviderClient).to(NewProviderClient);
```

5. **添加到工厂**
```typescript
// 在 LLMClientFactory 中
case 'new-provider':
  return this.newProviderClient;
```

## 最佳实践

### 1. 配置管理
- ✅ 所有配置从配置文件读取
- ✅ 配置缺失时抛出明确错误
- ❌ 禁止硬编码默认值

### 2. 错误处理
- ✅ 提供清晰的错误信息
- ✅ 指导用户如何修复
- ❌ 避免模糊的错误消息

### 3. 类型安全
- ✅ 使用抽象方法强制实现
- ✅ 利用 TypeScript 编译时检查
- ❌ 避免运行时才发现问题

### 4. 代码复用
- ✅ 使用基类提供通用功能
- ✅ 使用策略模式处理差异
- ❌ 避免重复代码

## 总结

本架构设计遵循以下核心原则：

1. **配置驱动**：无硬编码，强制配置
2. **类型安全**：编译时检查，避免运行时错误
3. **策略模式**：处理提供商差异
4. **开闭原则**：易于扩展新提供商

通过这些设计，LLM Clients 模块具有：
- ✅ 高度可维护性
- ✅ 强类型安全
- ✅ 易于扩展
- ✅ 清晰的错误提示
- ✅ 统一的实现模式