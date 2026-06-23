# 基础设施层LLM模块自定义请求头和参数支持分析

## 分析概述

本文档分析了基础设施层LLM模块对自定义请求头和各类请求参数的支持情况。

**分析日期**：2025-01-21
**分析范围**：`src/infrastructure/llm/` 模块
**分析结论**：✅ **完全支持**自定义请求头和各类请求参数

---

## 一、自定义请求头支持

### 1.1 支持情况：✅ 完全支持

#### 领域层支持

**文件**：[`src/domain/llm/entities/llm-request.ts`](src/domain/llm/entities/llm-request.ts:37)

[`LLMRequest`](src/domain/llm/entities/llm-request.ts:59) 实体包含 `headers` 属性：

```typescript
export interface LLMRequestProps {
  // ... 其他属性
  readonly headers?: Record<string, string>;  // 第37行
  // ...
}
```

**创建方法支持**（第106行）：

```typescript
public static create(
  model: string,
  messages: LLMMessage[],
  options?: {
    // ... 其他选项
    headers?: Record<string, string>;  // 第106行
    // ...
  }
): LLMRequest
```

**访问器方法**（第332行）：

```typescript
public get headers(): Record<string, string> | undefined {
  return this.props.headers ? { ...this.props.headers } : undefined;
}
```

#### 端点策略层支持

**文件**：[`src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:72)

[`BaseEndpointStrategy.buildHeaders()`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:72) 方法支持请求级自定义头部：

```typescript
buildHeaders(config: ProviderConfig, request?: LLMRequest): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };

  // 合并请求级自定义头部（第78-80行）
  if (request?.headers) {
    Object.assign(headers, request.headers);
  }

  return headers;
}
```

**实现示例**：

- **OpenAI兼容端点**：[`OpenAICompatibleEndpointStrategy.buildHeaders()`](src/infrastructure/llm/endpoint-strategies/openai-compatible-endpoint-strategy.ts:62)
  ```typescript
  override buildHeaders(config: ProviderConfig, request?: LLMRequest): Record<string, string> {
    const headers = super.buildHeaders(config, request);  // 继承自定义头部支持
    headers['Authorization'] = `Bearer ${config.apiKey}`;
    // ...
    return headers;
  }
  ```

- **Anthropic端点**：[`AnthropicEndpointStrategy.buildHeaders()`](src/infrastructure/llm/endpoint-strategies/anthropic-endpoint-strategy.ts:79)
  ```typescript
  override buildHeaders(config: ProviderConfig, request?: LLMRequest): Record<string, string> {
    const headers = super.buildHeaders(config, request);  // 继承自定义头部支持
    headers['x-api-key'] = config.apiKey;
    // ...
    return headers;
  }
  ```

#### HTTP客户端层支持

**文件**：[`src/infrastructure/common/http/http-client.ts`](src/infrastructure/common/http/http-client.ts:19)

[`HttpClient`](src/infrastructure/common/http/http-client.ts:19) 基于 Axios，支持完整的请求头配置：

```typescript
async post<T = any>(
  url: string,
  data?: any,
  config?: AxiosRequestConfig  // 支持自定义 headers
): Promise<AxiosResponse<T>> {
  return this.request({ ...config, method: 'POST', url, data });
}
```

**默认头部管理**（第179-185行）：

```typescript
setDefaultHeader(key: string, value: string): void {
  this.axiosInstance.defaults.headers.common[key] = value;
}

removeDefaultHeader(key: string): void {
  delete this.axiosInstance.defaults.headers.common[key];
}
```

### 1.2 使用示例

```typescript
// 创建带自定义请求头的请求
const request = LLMRequest.create('gpt-4', messages, {
  headers: {
    'X-Custom-Header': 'custom-value',
    'X-Request-ID': '12345',
    'X-Trace-ID': 'trace-abc-123'
  }
});

// 请求头会自动合并到最终HTTP请求中
// 最终请求头包含：
// - Content-Type: application/json (默认)
// - Authorization: Bearer <api-key> (提供商特定)
// - X-Custom-Header: custom-value (自定义)
// - X-Request-ID: 12345 (自定义)
// - X-Trace-ID: trace-abc-123 (自定义)
```

---

## 二、自定义请求参数支持

### 2.1 支持情况：✅ 完全支持

#### 2.1.1 标准参数支持

**文件**：[`src/domain/llm/entities/llm-request.ts`](src/domain/llm/entities/llm-request.ts:59)

[`LLMRequest`](src/domain/llm/entities/llm-request.ts:59) 支持以下标准参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `temperature` | `number \| undefined` | 温度参数（第17行） |
| `maxTokens` | `number \| undefined` | 最大token数（第18行） |
| `topP` | `number \| undefined` | top_p参数（第19行） |
| `frequencyPenalty` | `number \| undefined` | 频率惩罚（第20行） |
| `presencePenalty` | `number \| undefined` | 存在惩罚（第21行） |
| `stop` | `string[] \| undefined` | 停止词列表（第22行） |
| `tools` | `Array<Tool> \| undefined` | 工具列表（第23-30行） |
| `toolChoice` | `ToolChoice \| undefined` | 工具选择（第31行） |
| `stream` | `boolean \| undefined` | 流式传输（第32行） |
| `reasoningEffort` | `'low' \| 'medium' \| 'high' \| undefined` | 推理努力程度（第33行） |

#### 2.1.2 提供商特定参数支持

**核心机制**：通过 `metadata` 字段支持任意提供商特定参数

**文件**：[`src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:51)

[`BaseParameterMapper`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:51) 提供了灵活的参数传递机制：

##### 关键方法

**1. `addMetadataParam()` - 添加已知元数据参数**（第213-222行）

```typescript
protected addMetadataParam<T>(
  target: Record<string, any>,
  metadata: Record<string, any> | undefined,
  key: string,
  targetKey?: string
): void {
  if (metadata && metadata[key] !== undefined) {
    target[targetKey || key] = metadata[key];
  }
}
```

**2. `passUnknownMetadataParams()` - 传递未知元数据参数**（第240-254行）

```typescript
protected passUnknownMetadataParams(
  target: Record<string, any>,
  metadata: Record<string, any> | undefined
): void {
  if (!metadata) {
    return;
  }

  for (const [key, value] of Object.entries(metadata)) {
    // 只传递未知的参数
    if (!this.knownMetadataKeys.includes(key)) {
      target[key] = value;
    }
  }
}
```

**3. `addKnownMetadataKey()` - 注册已知元数据键**（第228-232行）

```typescript
protected addKnownMetadataKey(key: string): void {
  if (!this.knownMetadataKeys.includes(key)) {
    this.knownMetadataKeys.push(key);
  }
}
```

##### OpenAI 参数映射器示例

**文件**：[`src/infrastructure/llm/parameter-mappers/openai-parameter-mapper.ts`](src/infrastructure/llm/parameter-mappers/openai-parameter-mapper.ts:52)

[`OpenAIParameterMapper`](src/infrastructure/llm/parameter-mappers/openai-parameter-mapper.ts:52) 支持多种OpenAI特定参数：

**注册已知参数**（第56-66行）：

```typescript
constructor() {
  super('OpenAIParameterMapper', '2.0.0', OpenAIParameterSchema);

  // 注册已知的元数据键
  this.addKnownMetadataKey('responseFormat');
  this.addKnownMetadataKey('seed');
  this.addKnownMetadataKey('serviceTier');
  this.addKnownMetadataKey('user');
  this.addKnownMetadataKey('n');
  this.addKnownMetadataKey('logitBias');
  this.addKnownMetadataKey('topLogprobs');
  this.addKnownMetadataKey('store');
  this.addKnownMetadataKey('streamOptions');
}
```

**参数映射**（第94-116行）：

```typescript
// 从元数据中获取 OpenAI 特有参数
if (request.metadata) {
  this.addMetadataParam(openaiRequest, request.metadata, 'responseFormat', 'response_format');
  this.addMetadataParam(openaiRequest, request.metadata, 'seed');
  this.addMetadataParam(openaiRequest, request.metadata, 'serviceTier', 'service_tier');
  this.addMetadataParam(openaiRequest, request.metadata, 'user');
  this.addMetadataParam(openaiRequest, request.metadata, 'n');
  this.addMetadataParam(openaiRequest, request.metadata, 'logitBias', 'logit_bias');
  this.addMetadataParam(openaiRequest, request.metadata, 'topLogprobs', 'top_logprobs');
  this.addMetadataParam(openaiRequest, request.metadata, 'store');
  this.addMetadataParam(openaiRequest, request.metadata, 'streamOptions', 'stream_options');
}

// 传递未知的元数据参数（支持通用参数传递）
this.passUnknownMetadataParams(openaiRequest, request.metadata);
```

**支持的OpenAI参数**（第17-28行）：

```typescript
const OpenAIParameterSchema = BaseParameterSchema.extend({
  reasoningEffort: z.enum(['low', 'medium', 'high']).optional(),
  responseFormat: z.record(z.string(), z.any()).optional(),
  seed: z.number().int().optional(),
  serviceTier: z.string().optional(),
  user: z.string().optional(),
  n: z.number().int().min(1).max(10).optional(),
  logitBias: z.record(z.number(), z.number()).optional(),
  topLogprobs: z.number().int().min(0).max(20).optional(),
  store: z.boolean().optional(),
  streamOptions: z.record(z.string(), z.any()).optional(),
});
```

#### 2.1.3 查询参数支持

**领域层**：[`LLMRequest`](src/domain/llm/entities/llm-request.ts:38) 包含 `queryParams` 属性（第38行）

```typescript
export interface LLMRequestProps {
  // ...
  readonly queryParams?: Record<string, string>;  // 第38行
  // ...
}
```

**端点策略**：[`BaseEndpointStrategy.addQueryParams()`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:164) 方法支持向URL添加查询参数（第164-172行）

```typescript
protected addQueryParams(url: string, params: Record<string, string>): string {
  const urlObj = new URL(url);
  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined && value !== null) {
      urlObj.searchParams.set(key, value);
    }
  });
  return urlObj.toString();
}
```

### 2.2 使用示例

#### 示例1：标准参数

```typescript
const request = LLMRequest.create('gpt-4', messages, {
  temperature: 0.7,
  maxTokens: 2000,
  topP: 0.9,
  frequencyPenalty: 0.5,
  presencePenalty: 0.3,
  stop: ['\n\n', 'END'],
  stream: true
});
```

#### 示例2：OpenAI特定参数

```typescript
const request = LLMRequest.create('gpt-4', messages, {
  metadata: {
    // OpenAI 特定参数
    responseFormat: { type: 'json_object' },
    seed: 42,
    serviceTier: 'auto',
    user: 'user-123',
    n: 1,
    logitBias: { '123': -100, '456': 50 },
    topLogprobs: 5,
    store: true,
    streamOptions: { include_usage: true }
  }
});
```

#### 示例3：任意自定义参数

```typescript
const request = LLMRequest.create('custom-model', messages, {
  metadata: {
    // 任意自定义参数（会自动传递）
    customProviderParam: 'value',
    experimentalFeature: true,
    providerSpecificConfig: {
      option1: 'value1',
      option2: 42
    }
  }
});
```

#### 示例4：查询参数

```typescript
const request = LLMRequest.create('model', messages, {
  queryParams: {
    'custom-param': 'value',
    'version': 'v2'
  }
});
```

---

## 三、架构设计优势

### 3.1 分层清晰

请求头和参数的处理逻辑分布在不同的层次，职责明确：

| 层次 | 职责 | 文件 |
|------|------|------|
| **领域层** | 定义请求实体和属性 | [`LLMRequest`](src/domain/llm/entities/llm-request.ts:59) |
| **参数映射层** | 参数转换和验证 | [`BaseParameterMapper`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:51) |
| **端点策略层** | 构建端点和请求头 | [`BaseEndpointStrategy`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:48) |
| **HTTP客户端层** | 执行HTTP请求 | [`HttpClient`](src/infrastructure/common/http/http-client.ts:19) |

### 3.2 灵活扩展

1. **Metadata机制**：通过 `metadata` 字段支持任意提供商特定参数
2. **未知参数自动传递**：`passUnknownMetadataParams()` 确保未知参数不会丢失
3. **已知参数注册**：`addKnownMetadataKey()` 允许显式声明支持的参数

### 3.3 类型安全

1. **TypeScript类型系统**：所有参数都有明确的类型定义
2. **Zod验证**：使用 Zod schema 进行运行时参数验证
3. **编译时检查**：TypeScript 编译器会在编译时检查类型错误

### 3.4 向后兼容

1. **可选参数**：所有自定义参数都是可选的
2. **默认值**：标准参数有合理的默认值
3. **渐进式增强**：可以逐步添加对新参数的支持

---

## 四、完整使用流程

### 4.1 创建请求

```typescript
const request = LLMRequest.create('gpt-4', messages, {
  // 标准参数
  temperature: 0.7,
  maxTokens: 2000,

  // 自定义请求头
  headers: {
    'X-Custom-Header': 'value',
    'X-Request-ID': '12345'
  },

  // 查询参数
  queryParams: {
    'version': 'v2'
  },

  // 提供商特定参数（通过metadata）
  metadata: {
    responseFormat: { type: 'json_object' },
    seed: 42,
    customParam: 'value'
  }
});
```

### 4.2 参数映射

**文件**：[`src/infrastructure/llm/clients/base-llm-client.ts`](src/infrastructure/llm/clients/base-llm-client.ts:47)

[`BaseLLMClient.generateResponse()`](src/infrastructure/llm/clients/base-llm-client.ts:47) 方法处理参数映射：

```typescript
public async generateResponse(request: LLMRequest): Promise<LLMResponse> {
  await this.rateLimiter.checkLimit();

  try {
    // 1. 参数映射（第52-55行）
    const providerRequest = this.providerConfig.parameterMapper.mapToProvider(
      request,
      this.providerConfig
    );

    // 2. 构建端点和头部（第58-62行）
    const endpoint = this.providerConfig.endpointStrategy.buildEndpoint(
      this.providerConfig,
      providerRequest
    );
    const headers = this.providerConfig.endpointStrategy.buildHeaders(this.providerConfig, request);

    // 3. 发送请求（第65行）
    const response = await this.httpClient.post(endpoint, providerRequest, { headers });

    // 4. 转换响应（第68行）
    return this.providerConfig.parameterMapper.mapFromResponse(response.data, request);
  } catch (error) {
    this.handleError(error);
  }
}
```

### 4.3 最终HTTP请求

最终发送的HTTP请求包含：

**请求头**：
```http
Content-Type: application/json
Authorization: Bearer <api-key>
X-Custom-Header: value
X-Request-ID: 12345
```

**请求体**：
```json
{
  "model": "gpt-4",
  "messages": [...],
  "temperature": 0.7,
  "max_tokens": 2000,
  "response_format": { "type": "json_object" },
  "seed": 42,
  "customParam": "value"
}
```

**查询参数**：
```
?version=v2
```

---

## 五、支持的提供商

### 5.1 OpenAI

**客户端**：[`OpenAIChatClient`](src/infrastructure/llm/clients/openai-chat-client.ts:18)
**参数映射器**：[`OpenAIParameterMapper`](src/infrastructure/llm/parameter-mappers/openai-parameter-mapper.ts:52)
**端点策略**：[`OpenAICompatibleEndpointStrategy`](src/infrastructure/llm/endpoint-strategies/openai-compatible-endpoint-strategy.ts:46)

**支持的参数**：
- 标准参数：temperature, maxTokens, topP, frequencyPenalty, presencePenalty, stop, stream
- OpenAI特定：reasoningEffort, responseFormat, seed, serviceTier, user, n, logitBias, topLogprobs, store, streamOptions
- 工具参数：tools, toolChoice

### 5.2 Anthropic

**客户端**：[`AnthropicClient`](src/infrastructure/llm/clients/anthropic-client.ts:18)
**参数映射器**：[`AnthropicParameterMapper`](src/infrastructure/llm/parameter-mappers/anthropic-parameter-mapper.ts)
**端点策略**：[`AnthropicEndpointStrategy`](src/infrastructure/llm/endpoint-strategies/anthropic-endpoint-strategy.ts:63)

**支持的参数**：
- 标准参数：temperature, maxTokens, topP, stop, stream
- Anthropic特定：topK, systemMessages
- 工具参数：tools, toolChoice

### 5.3 Gemini

**客户端**：[`GeminiClient`](src/infrastructure/llm/clients/gemini-client.ts:18)
**参数映射器**：[`GeminiParameterMapper`](src/infrastructure/llm/parameter-mappers/gemini-parameter-mapper.ts)
**端点策略**：[`GeminiNativeEndpointStrategy`](src/infrastructure/llm/endpoint-strategies/gemini-native-endpoint-strategy.ts)

**支持的参数**：
- 标准参数：temperature, topP, topK, stopSequences, maxTokens
- Gemini特定：thinking_budget, cached_content

### 5.4 自定义提供商

可以通过继承以下类来支持自定义提供商：

1. 继承 [`BaseLLMClient`](src/infrastructure/llm/clients/base-llm-client.ts:19)
2. 实现 [`BaseParameterMapper`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:51)
3. 实现 [`BaseEndpointStrategy`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:48)

---

## 六、最佳实践

### 6.1 使用自定义请求头

```typescript
// ✅ 推荐：使用 headers 属性
const request = LLMRequest.create('model', messages, {
  headers: {
    'X-Request-ID': generateRequestId(),
    'X-Trace-ID': getTraceId(),
    'X-Custom-Auth': getCustomAuthToken()
  }
});
```

### 6.2 使用提供商特定参数

```typescript
// ✅ 推荐：使用 metadata 属性
const request = LLMRequest.create('gpt-4', messages, {
  metadata: {
    responseFormat: { type: 'json_object' },
    seed: 42,
    logitBias: { '123': -100 }
  }
});
```

### 6.3 使用查询参数

```typescript
// ✅ 推荐：使用 queryParams 属性
const request = LLMRequest.create('model', messages, {
  queryParams: {
    'version': 'v2',
    'region': 'us-east-1'
  }
});
```

### 6.4 组合使用

```typescript
// ✅ 推荐：组合使用所有自定义选项
const request = LLMRequest.create('gpt-4', messages, {
  // 标准参数
  temperature: 0.7,
  maxTokens: 2000,

  // 自定义请求头
  headers: {
    'X-Request-ID': generateRequestId()
  },

  // 查询参数
  queryParams: {
    'version': 'v2'
  },

  // 提供商特定参数
  metadata: {
    responseFormat: { type: 'json_object' },
    seed: 42
  }
});
```

---

## 七、限制和注意事项

### 7.1 请求头限制

1. **覆盖风险**：自定义请求头可能会覆盖提供商默认的请求头
2. **大小限制**：HTTP请求头有大小限制（通常8KB）
3. **安全性**：敏感信息不应放在请求头中

### 7.2 参数限制

1. **验证**：未知参数不会进行验证，可能导致API错误
2. **兼容性**：不同提供商对同一参数的支持可能不同
3. **文档**：需要查阅提供商文档了解支持的参数

### 7.3 查询参数限制

1. **URL长度**：查询参数会增加URL长度，有长度限制
2. **编码**：特殊字符需要正确编码
3. **缓存**：查询参数可能影响缓存行为

---

## 八、总结

### 8.1 支持情况总结

| 功能 | 支持情况 | 实现位置 |
|------|----------|----------|
| 自定义请求头 | ✅ 完全支持 | [`LLMRequest.headers`](src/domain/llm/entities/llm-request.ts:37), [`BaseEndpointStrategy.buildHeaders()`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:72) |
| 标准参数 | ✅ 完全支持 | [`LLMRequest`](src/domain/llm/entities/llm-request.ts:59) |
| 提供商特定参数 | ✅ 完全支持 | [`LLMRequest.metadata`](src/domain/llm/entities/llm-request.ts:36), [`BaseParameterMapper`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:51) |
| 查询参数 | ✅ 完全支持 | [`LLMRequest.queryParams`](src/domain/llm/entities/llm-request.ts:38), [`BaseEndpointStrategy.addQueryParams()`](src/infrastructure/llm/endpoint-strategies/base-endpoint-strategy.ts:164) |
| 未知参数传递 | ✅ 完全支持 | [`BaseParameterMapper.passUnknownMetadataParams()`](src/infrastructure/llm/parameter-mappers/base-parameter-mapper.ts:240) |

### 8.2 架构优势

1. **分层清晰**：职责明确，易于维护
2. **灵活扩展**：支持任意自定义参数
3. **类型安全**：TypeScript + Zod 双重保障
4. **向后兼容**：渐进式增强，不破坏现有功能

### 8.3 结论

基础设施层的LLM模块**完全支持**自定义请求头和各类请求参数，设计灵活且易于扩展。开发者可以通过以下方式自定义请求：

1. **自定义请求头**：使用 `headers` 属性
2. **标准参数**：直接使用请求选项
3. **提供商特定参数**：使用 `metadata` 属性
4. **查询参数**：使用 `queryParams` 属性

该架构设计遵循了领域驱动设计（DDD）原则，具有良好的可维护性和可扩展性。