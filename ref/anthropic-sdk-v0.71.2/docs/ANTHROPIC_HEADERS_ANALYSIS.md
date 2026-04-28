# Anthropic SDK v0.71.2 请求头设置分析

## 概述

Anthropic SDK 的请求头设置采用**分层构建模式**，通过在 `buildHeaders()` 方法中按顺序合并多个请求头源，后面的头覆盖前面的相同名称头。

---

## 请求头设置流程

### 核心入口：`buildHeaders()` 方法

**位置**：`src/client.ts` 第 869-908 行

```typescript
private async buildHeaders({
  options,
  method,
  bodyHeaders,
  retryCount,
}: {
  options: FinalRequestOptions;
  method: HTTPMethod;
  bodyHeaders: HeadersLike;
  retryCount: number;
}): Promise<Headers> {
  // 1. 构建幂等性头（如果需要）
  let idempotencyHeaders: HeadersLike = {};
  if (this.idempotencyHeader && method !== 'get') {
    if (!options.idempotencyKey) options.idempotencyKey = this.defaultIdempotencyKey();
    idempotencyHeaders[this.idempotencyHeader] = options.idempotencyKey;
  }

  // 2. 按顺序合并所有头
  const headers = buildHeaders([
    idempotencyHeaders,                        // 1️⃣ 幂等性头（可选）
    {
      Accept: 'application/json',               // 2️⃣ 标准请求头
      'User-Agent': this.getUserAgent(),
      'X-Stainless-Retry-Count': String(retryCount),
      ...(options.timeout ? { 'X-Stainless-Timeout': String(Math.trunc(options.timeout / 1000)) } : {}),
      ...getPlatformHeaders(),                  // 3️⃣ 平台检测头
      ...(this._options.dangerouslyAllowBrowser ?
        { 'anthropic-dangerous-direct-browser-access': 'true' }
      : undefined),
      'anthropic-version': '2023-06-01',        // 4️⃣ API 版本
    },
    await this.authHeaders(options),             // 5️⃣ 认证头
    this._options.defaultHeaders,                // 6️⃣ 全局默认头
    bodyHeaders,                                 // 7️⃣ 请求体相关头
    options.headers,                             // 8️⃣ 请求级别头（最高优先级）
  ]);

  this.validateHeaders(headers);
  return headers.values;
}
```

---

## 请求头组成详解

### 1️⃣ 幂等性头（可选）

**条件**：仅当 `idempotencyHeader` 被设置且 HTTP 方法不是 GET 时

**源码**：第 880-884 行

```typescript
let idempotencyHeaders: HeadersLike = {};
if (this.idempotencyHeader && method !== 'get') {
  if (!options.idempotencyKey) options.idempotencyKey = this.defaultIdempotencyKey();
  idempotencyHeaders[this.idempotencyHeader] = options.idempotencyKey;
}
```

**示例**：`Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000`

---

### 2️⃣ 标准请求头 + Stainless 工具链头

**固定头**：

| 头名称 | 值 | 说明 |
|--------|-----|------|
| `Accept` | `application/json` | 期望响应格式 |
| `User-Agent` | 例：`Anthropic/JS 0.71.2` | SDK 标识 + 版本 |
| `X-Stainless-Retry-Count` | 当前重试次数 | 标识重试次数 |
| `X-Stainless-Timeout` | 超时秒数（可选） | 仅在设置超时时包含 |
| `anthropic-version` | `2023-06-01` | API 版本固定值 |

**条件头**：

```typescript
...(options.timeout ? { 'X-Stainless-Timeout': String(Math.trunc(options.timeout / 1000)) } : {})
// 仅当请求设置了超时时包含此头
```

```typescript
...(this._options.dangerouslyAllowBrowser ?
  { 'anthropic-dangerous-direct-browser-access': 'true' }
: undefined)
// 仅当客户端选项允许浏览器危险访问时包含
```

---

### 3️⃣ 平台检测头

**位置**：`src/internal/detect-platform.ts` 第 194-196 行

```typescript
export const getPlatformHeaders = () => {
  return (_platformHeaders ??= getPlatformProperties());
};
```

**生成逻辑**（自动检测）：

#### Node.js 环境

```javascript
{
  'X-Stainless-Lang': 'js',
  'X-Stainless-Package-Version': '0.71.2',           // SDK 版本
  'X-Stainless-OS': 'Windows' | 'Linux' | 'MacOS',   // process.platform
  'X-Stainless-Arch': 'x64' | 'arm64',               // process.arch
  'X-Stainless-Runtime': 'node',
  'X-Stainless-Runtime-Version': 'v22.14.0'          // process.version
}
```

#### Deno 环境

```javascript
{
  'X-Stainless-Lang': 'js',
  'X-Stainless-Package-Version': VERSION,
  'X-Stainless-OS': Deno.build.os 规范化后,
  'X-Stainless-Arch': Deno.build.arch 规范化后,
  'X-Stainless-Runtime': 'deno',
  'X-Stainless-Runtime-Version': Deno.version
}
```

#### Cloudflare Edge 环境

```javascript
{
  'X-Stainless-Lang': 'js',
  'X-Stainless-Package-Version': VERSION,
  'X-Stainless-OS': 'Unknown',
  'X-Stainless-Arch': `other:${EdgeRuntime}`,
  'X-Stainless-Runtime': 'edge',
  'X-Stainless-Runtime-Version': process.version
}
```

#### 浏览器环境

```javascript
{
  'X-Stainless-Lang': 'js',
  'X-Stainless-Package-Version': VERSION,
  'X-Stainless-OS': 'Unknown',
  'X-Stainless-Arch': 'unknown',
  'X-Stainless-Runtime': 'browser:chrome' | 'browser:firefox' | ...,
  'X-Stainless-Runtime-Version': 浏览器版本字符串
}
```

**架构规范化**（`normalizeArch`）：

- `x32` → `x32`
- `x86_64`, `x64` → `x64`
- `arm` → `arm`
- `aarch64`, `arm64` → `arm64`
- 其他值 → `other:{arch}`

**平台规范化**（`normalizePlatform`）：

- `darwin` → `MacOS`
- `win32` → `Windows`
- `linux` → `Linux`
- `freebsd` → `FreeBSD`
- `openbsd` → `OpenBSD`
- `android` → `Android`
- `ios` → `iOS`
- 其他 → `Other:{platform}`

---

### 4️⃣ 认证头

**位置**：`src/client.ts` 第 391-407 行

#### 方法签名

```typescript
protected async authHeaders(opts: FinalRequestOptions): Promise<NullableHeaders | undefined> {
  return buildHeaders([await this.apiKeyAuth(opts), await this.bearerAuth(opts)]);
}
```

#### API Key 认证

```typescript
protected async apiKeyAuth(opts: FinalRequestOptions): Promise<NullableHeaders | undefined> {
  if (this.apiKey == null) {
    return undefined;
  }
  return buildHeaders([{ 'X-Api-Key': this.apiKey }]);
}
```

**来源**：

1. 构造函数参数 `apiKey`（类型可为字符串、函数或 null）
2. 如果不提供，检查环境变量 `process.env['ANTHROPIC_API_KEY']`

**说明**：
- API Key 可以是静态字符串或异步函数 `() => Promise<string>`
- 异步函数在每次请求前被调用（支持运行时凭证轮换）
- 若函数返回空字符串或抛异常，会被包装为 `AnthropicError`

#### Bearer Token 认证

```typescript
protected async bearerAuth(opts: FinalRequestOptions): Promise<NullableHeaders | undefined> {
  if (this.authToken == null) {
    return undefined;
  }
  return buildHeaders([{ Authorization: `Bearer ${this.authToken}` }]);
}
```

**来源**：

1. 构造函数参数 `authToken`
2. 环境变量 `process.env['ANTHROPIC_AUTH_TOKEN']`

**备注**：Bearer Token 通常用于 OAuth 2.0 或其他令牌认证

---

### 5️⃣ 全局默认头

**位置**：`src/client.ts` 第 166-229 行（`ClientOptions` 接口）

```typescript
export interface ClientOptions {
  /**
   * Default headers to include with every request to the API.
   *
   * These can be removed in individual requests by explicitly setting the
   * header to `null` in request options.
   */
  defaultHeaders?: HeadersLike | undefined;
  // ...
}
```

**使用示例**：

```typescript
const client = new Anthropic({
  apiKey: process.env.ANTHROPIC_API_KEY,
  defaultHeaders: {
    'X-Custom-Header': 'value',
    'X-Request-ID': generateRequestId()
  }
});
```

**特性**：可通过在请求选项中设置头为 `null` 来移除

---

### 6️⃣ 请求体相关头

**来源**：`buildBody()` 方法返回的 `bodyHeaders`

**位置**：`src/client.ts` 第 910-945 行

**例如**（自动检测）：

```typescript
{
  'content-type': 'application/json'  // 当请求体是 JSON 时
}
```

---

### 7️⃣ 请求级别头（用户传入）

**来源**：`FinalRequestOptions` 的 `headers` 属性

**最高优先级**：可覆盖所有前面的头

**使用示例**：

```typescript
client.messages.create({
  model: 'claude-3-5-sonnet-20241022',
  max_tokens: 1024,
  messages: [{ role: 'user', content: 'Hello' }],
  headers: {
    'X-Custom-Id': 'request-123',
    'Accept': 'application/json'  // 覆盖默认值
  }
});
```

---

## 请求头合并逻辑

### `buildHeaders()` 函数

**位置**：`src/internal/headers.ts` 第 73-94 行

```typescript
export const buildHeaders = (newHeaders: HeadersLike[]): NullableHeaders => {
  const targetHeaders = new Headers();
  const nullHeaders = new Set<string>();
  for (const headers of newHeaders) {
    const seenHeaders = new Set<string>();
    for (const [name, value] of iterateHeaders(headers)) {
      const lowerName = name.toLowerCase();
      if (!seenHeaders.has(lowerName)) {
        targetHeaders.delete(name);           // 清除旧值（大小写不敏感）
        seenHeaders.add(lowerName);
      }
      if (value === null) {
        targetHeaders.delete(name);           // null 表示删除头
        nullHeaders.add(lowerName);
      } else {
        targetHeaders.append(name, value);    // 添加新头
        nullHeaders.delete(lowerName);
      }
    }
  }
  return { [brand_privateNullableHeaders]: true, values: targetHeaders, nulls: nullHeaders };
};
```

### 关键特性

1. **头名称大小写不敏感**：根据小写名称去重
2. **后覆盖前**：后面的数组中的头覆盖前面的同名头
3. **显式删除**：设置头值为 `null` 可删除该头
4. **多值头**：同一头可通过 `append` 添加多个值

---

## 头名称大小写规范

Anthropic SDK 使用以下大小写约定：

| 类别 | 头名称 | 说明 |
|------|--------|------|
| 标准 HTTP | `Accept`, `User-Agent`, `Authorization` | 标准 HTTP 规范 |
| Anthropic 专有 | `X-Api-Key`, `anthropic-version` | Anthropic API 需求 |
| Stainless 工具链 | `X-Stainless-Lang`, `X-Stainless-Retry-Count` | Stainless 生成器添加 |
| 自定义 | 由用户定义 | 可全小写或标准驼峰 |

---

## 完整请求头示例

### 示例场景

```typescript
const client = new Anthropic({
  apiKey: 'sk-ant-...',
  defaultHeaders: {
    'X-Project-ID': 'proj-123'
  }
});

client.messages.create({
  model: 'claude-3-5-sonnet-20241022',
  max_tokens: 1024,
  messages: [{ role: 'user', content: 'Hello' }],
  headers: {
    'X-Request-ID': 'req-456'
  }
});
```

### 最终请求头（合并后）

```http
Accept: application/json
User-Agent: Anthropic/JS 0.71.2
X-Stainless-Retry-Count: 0
X-Stainless-Timeout: 600
X-Stainless-Lang: js
X-Stainless-Package-Version: 0.71.2
X-Stainless-OS: Windows
X-Stainless-Arch: x64
X-Stainless-Runtime: node
X-Stainless-Runtime-Version: v22.14.0
anthropic-version: 2023-06-01
X-Api-Key: sk-ant-...
X-Project-ID: proj-123
content-type: application/json
X-Request-ID: req-456
```

---

## 特殊情况处理

### 1. API Key 动态获取

```typescript
const client = new Anthropic({
  apiKey: async () => {
    // 从安全存储或远程服务获取 API Key
    const key = await fetchAPIKeyFromSecureStorage();
    return key;
  }
});
```

- API Key 函数在每次请求前被调用
- 支持凭证轮换和更新
- 若返回空字符串或抛异常会产生 `AnthropicError`

### 2. 浏览器环全链路安全警告

```typescript
const client = new Anthropic({
  apiKey: 'sk-ant-...',
  dangerouslyAllowBrowser: true  // ⚠️ 仅在有保护措施时使用
});
```

- 添加头：`anthropic-dangerous-direct-browser-access: true`
- 表明开发者理解浏览器暴露凭证的风险

### 3. 移除默认头

```typescript
client.messages.create({
  // ...
  headers: {
    'User-Agent': null  // 显式移除 User-Agent
  }
});
```

- 设置头值为 `null` 可删除该头
- 适用于所有来源的头（全局默认、请求级别等）

---

## 架构优势

| 优势 | 说明 |
|------|------|
| **灵活性** | 支持全局默认头、请求级别头，且后者可覆盖前者 |
| **可维护性** | 各头按来源分离，易于理解和扩展 |
| **向后兼容** | 平台检测和工具链头透明添加，无需用户干预 |
| **安全性** | 认证头独立构建，API Key 可动态生成 |
| **标准性** | 遵循 HTTP 规范和 Anthropic API 约定 |

---

## 与 OpenAI SDK 的对比

| 维度 | Anthropic | OpenAI |
|------|-----------|--------|
| 认证方式 | `X-Api-Key` 或 `Authorization: Bearer` | `Authorization: Bearer` |
| 平台检测 | `X-Stainless-*` 头（详细） | `User-Agent` 中编码 |
| 版本标识 | `anthropic-version` | `OpenAI-Organization` |
| 幂等性 | 可配置（`Idempotency-Key`） | 固定（`Idempotency-Key`） |

---

## 总结

Anthropic SDK 的请求头设置采用**分层构建 + 后覆盖前** 的模式：

1. **基础层**：幂等性头 → 标准头 → 平台检测头
2. **认证层**：API Key 认证 → Bearer Token 认证
3. **自定义层**：全局默认头 → 请求级别头

每层都可独立配置，且后一层可覆盖前一层的同名头，提供了**最大的灵活性和用户控制力**。
