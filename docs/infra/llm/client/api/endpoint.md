# LLM 客户端端点组装流程与配置规范

## 概述

本文档详细描述了 Modular Agent Framework 中 LLM 客户端的端点组装流程、各个供应商客户端的构建结构以及配置规范。特别关注端点策略（Endpoint Strategy）如何与配置文件协同工作，避免版本号冲突等问题。

## 端点组装流程

### 1. 整体架构

```
配置文件 (TOML) → 客户端配置 → 端点策略 → URL 构建 → HTTP 请求
```

### 2. 详细流程

#### 步骤 1: 配置文件加载
- **位置**: `configs/llms/provider/{provider}/common.toml`
- **关键字段**: `base_url`, `api_version`, `api_key`
- **示例**:
```toml
base_url = "https://api.openai.com/v1"
api_version = "v1"
api_key = "${OPENAI_API_KEY}"
```

#### 步骤 2: 客户端配置构建
- **位置**: `src/infrastructure/external/llm/clients/{provider}-client.ts`
- **关键代码**:
```typescript
const providerConfig = new ProviderConfigBuilder()
  .name('OpenAI')
  .apiType(ApiType.OPENAI_COMPATIBLE)
  .baseURL('https://api.openai.com/v1')  // 来自配置文件
  .apiKey(configManager.get('llm.openai.apiKey'))
  .endpointStrategy(new OpenAICompatibleEndpointStrategy())
  .parameterMapper(new OpenAIParameterMapper())
  .featureSupport(featureSupport)
  .defaultModel('gpt-3.5-turbo')
  .build();
```

#### 步骤 3: 端点策略执行
- **位置**: `src/infrastructure/external/llm/clients/base-llm-client.ts:57`
- **关键代码**:
```typescript
const endpoint = this.providerConfig.endpointStrategy.buildEndpoint(this.providerConfig, enhancedRequest);
const headers = this.providerConfig.endpointStrategy.buildHeaders(this.providerConfig);
```

#### 步骤 4: HTTP 请求发送
- **位置**: `src/infrastructure/external/llm/clients/base-llm-client.ts:61`
- **关键代码**:
```typescript
const response = await this.httpClient.post(endpoint, enhancedRequest, { headers });
```

## 各供应商客户端构建结构

### 1. OpenAI Chat 客户端

**文件**: `src/infrastructure/external/llm/clients/openai-chat-client.ts`

**端点策略**: `OpenAICompatibleEndpointStrategy`
**URL 构建**: `buildPath(config.baseURL, 'chat', 'completions')`
**最终 URL**: `https://api.openai.com/v1/chat/completions`

**配置示例**:
```typescript
const providerConfig: ProviderConfig = {
  name: 'OpenAI',
  apiType: ApiType.OPENAI_COMPATIBLE,
  apiKey: configManager.get('llm.openai.apiKey', ''),
  baseURL: 'https://api.openai.com/v1',  // 包含版本号
  parameterMapper: new OpenAIParameterMapper(),
  endpointStrategy: new OpenAICompatibleEndpointStrategy(),
  featureSupport: featureSupport,
  defaultModel: 'gpt-3.5-turbo'
};
```

### 2. OpenAI Response 客户端 (新)

**文件**: `src/infrastructure/external/llm/clients/openai-response-client.ts`

**端点策略**: `OpenAIResponsesEndpointStrategy`
**URL 构建**: `buildPath(config.baseURL, 'responses')`
**最终 URL**: `https://api.openai.com/v1/responses`

**配置示例**:
```typescript
const providerConfig = new ProviderConfigBuilder()
  .name('OpenAI Response')
  .apiType(ApiType.OPENAI_COMPATIBLE)
  .baseURL('https://api.openai.com/v1')
  .apiKey(configManager.get('llm.openai.apiKey'))
  .endpointStrategy(new OpenAIResponsesEndpointStrategy())
  .parameterMapper(new OpenAIParameterMapper())
  .featureSupport(featureSupport)
  .defaultModel('gpt-5')
  .extraConfig({
    endpointPath: 'responses',  // 不包含版本号
    enableBeta: true,
    betaVersion: 'responses=v1'
  })
  .build();
```

### 3. Anthropic 客户端

**文件**: `src/infrastructure/external/llm/clients/anthropic-client.ts`

**端点策略**: `AnthropicEndpointStrategy`
**URL 构建**: `buildPath(config.baseURL, 'v1', 'messages')`
**最终 URL**: `https://api.anthropic.com/v1/messages`

**配置示例**:
```typescript
const providerConfig: ProviderConfig = {
  name: 'Anthropic',
  apiType: ApiType.NATIVE,
  apiKey: configManager.get('llm.anthropic.apiKey', ''),
  baseURL: 'https://api.anthropic.com',  // 不包含版本号
  parameterMapper: new AnthropicParameterMapper(),
  endpointStrategy: new AnthropicEndpointStrategy(),
  featureSupport: featureSupport,
  defaultModel: 'claude-3-sonnet-20240229',
  extraConfig: {
    apiVersion: '2023-06-01'
  }
};
```

### 4. Gemini 客户端

**文件**: `src/infrastructure/external/llm/clients/gemini-client.ts`

**端点策略**: `GeminiNativeEndpointStrategy`
**URL 构建**: `buildPath(config.baseURL, 'v1beta', 'models', `${request['model']}:generateContent`)`
**最终 URL**: `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={apiKey}`

**配置示例**:
```typescript
const providerConfig = new ProviderConfigBuilder()
  .name('gemini')
  .apiType(ApiType.NATIVE)
  .baseURL('https://generativelanguage.googleapis.com')  // 不包含版本号
  .apiKey(configManager.get('llm.gemini.apiKey'))
  .endpointStrategy(new GeminiNativeEndpointStrategy())
  .parameterMapper(new GeminiParameterMapper())
  .featureSupport(featureSupport)
  .defaultModel('gemini-2.5-pro')
  .build();
```

## 配置规范

### 1. 版本号处理规范

#### 情况 A: baseURL 包含版本号
**适用**: OpenAI 系列
**配置**:
```toml
base_url = "https://api.openai.com/v1"
```
**端点策略**: 不添加版本号
```typescript
buildPath(config.baseURL, 'chat', 'completions')  // 或 'responses'
```
**结果**: `https://api.openai.com/v1/chat/completions`

#### 情况 B: baseURL 不包含版本号
**适用**: Anthropic、Gemini
**配置**:
```toml
base_url = "https://api.anthropic.com"
# 或
base_url = "https://generativelanguage.googleapis.com"
```
**端点策略**: 添加版本号
```typescript
buildPath(config.baseURL, 'v1', 'messages')  // Anthropic
buildPath(config.baseURL, 'v1beta', 'models', ...)  // Gemini
```
**结果**: `https://api.anthropic.com/v1/messages`

### 2. 端点策略配置规范

#### OpenAI Responses 端点策略配置
```typescript
.extraConfig({
  // 端点路径（不包含版本号）
  endpointPath: 'responses',
  
  // 认证配置
  authType: 'Bearer',
  
  // OpenAI 特定配置
  organization: 'org-your-org',
  project: 'proj-your-project',
  apiVersion: 'v1',
  
  // Beta 功能
  enableBeta: true,
  betaVersion: 'responses=v1',
  
  // 自定义请求头
  defaultHeaders: {
    'Content-Type': 'application/json',
    'User-Agent': 'MyApp/1.0'
  },
  
  // 自定义认证
  customAuth: {
    type: 'header',  // 'header' | 'body' | 'query'
    header: 'X-API-Key'  // 仅当 type 为 'header' 时
  }
})
```

### 3. 功能支持配置

#### 基础功能支持
```typescript
const featureSupport = new BaseFeatureSupport();
featureSupport.supportsStreaming = true;
featureSupport.supportsTools = true;
featureSupport.supportsImages = true;
featureSupport.supportsAudio = false;
featureSupport.supportsVideo = false;
featureSupport.supportsSystemMessages = true;
featureSupport.supportsTemperature = true;
featureSupport.supportsTopP = true;
featureSupport.supportsMaxTokens = true;
```

#### 供应商特定功能
```typescript
featureSupport.setProviderSpecificFeature('reasoning_effort', true);
featureSupport.setProviderSpecificFeature('previous_response_id', true);
featureSupport.setProviderSpecificFeature('verbosity', true);
```

## 端点策略实现

### 1. OpenAI Responses 端点策略

**文件**: `src/infrastructure/external/llm/endpoint-strategies/providers/openai-responses-endpoint-strategy.ts`

**核心方法**:
```typescript
buildEndpoint(config: ProviderConfig, request: ProviderRequest): string {
  const endpointPath = config.extraConfig?.['endpointPath'] || 'responses';
  if (endpointPath.startsWith('/')) {
    return this.buildPath(config.baseURL, endpointPath.slice(1));
  } else {
    const pathSegments = endpointPath.split('/');
    return this.buildPath(config.baseURL, ...pathSegments);
  }
}

buildHeaders(config: ProviderConfig): Record<string, string> {
  const headers = super.buildHeaders(config);
  const defaultHeaders = config.extraConfig?.['defaultHeaders'] || {};
  Object.assign(headers, defaultHeaders);
  
  if (!headers['Authorization'] && config.apiKey) {
    const authType = config.extraConfig?.['authType'] || 'Bearer';
    headers['Authorization'] = `${authType} ${config.apiKey}`;
  }
  
  if (!headers['OpenAI-Beta'] && config.extraConfig?.['enableBeta'] !== false) {
    headers['OpenAI-Beta'] = config.extraConfig?.['betaVersion'] || 'responses=v1';
  }
  
  return headers;
}
```

## 测试与验证

### 1. 单元测试
**文件**: `src/infrastructure/external/llm/endpoint-strategies/providers/openai-responses-endpoint-strategy.test.ts`

**测试覆盖**:
- 端点 URL 构建（默认路径、自定义路径、绝对路径）
- 请求头构建（基本头部、自定义头部、认证方式）
- 认证处理（默认、请求体认证、查询参数认证）
- 配置验证（有效性检查、错误检测）

### 2. 集成测试
```bash
npm test -- openai-responses-endpoint-strategy.test.ts
```

**结果**: 23 个测试用例全部通过

## 最佳实践

### 1. 版本号处理
- **始终检查** baseURL 是否包含版本号
- **配置驱动** 端点路径，避免硬编码
- **提供灵活性** 支持绝对路径和相对路径

### 2. 配置管理
- **分离关注点** 基础配置与供应商特定配置
- **环境变量** 用于敏感信息（API 密钥）
- **默认值** 提供合理的默认配置

### 3. 错误处理
- **详细验证** 配置有效性检查
- **清晰错误** 提供具体的错误信息
- **向后兼容** 保持接口稳定性

## 常见问题

### Q: 如何处理不同供应商的版本号策略？
A: 通过配置驱动的设计，baseURL 可以包含或不包含版本号，端点策略相应调整。

### Q: 端点策略和参数映射器的关系？
A: 端点策略负责 URL 和头部，参数映射器负责请求/响应格式转换，两者协同工作。

### Q: 如何支持新的供应商？
A: 实现新的端点策略类，继承 `BaseEndpointStrategy`，并在客户端配置中使用。

### Q: 版本号冲突如何检测？
A: 通过测试用例验证 URL 构建结果，确保没有重复的版本号路径段。

## 相关文件

- **端点策略**: `src/infrastructure/external/llm/endpoint-strategies/providers/`
- **客户端实现**: `src/infrastructure/external/llm/clients/`
- **配置示例**: `src/infrastructure/external/llm/endpoint-strategies/providers/openai-responses-config-example.ts`
- **测试文件**: `src/infrastructure/external/llm/endpoint-strategies/providers/openai-responses-endpoint-strategy.test.ts`
- **基础架构**: `src/infrastructure/external/llm/endpoint-strategies/base-endpoint-strategy.ts`