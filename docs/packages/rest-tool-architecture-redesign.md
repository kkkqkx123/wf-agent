# REST工具执行器架构重构方案

## 背景分析

### 当前问题

通过分析现有代码，发现REST工具执行器存在以下架构问题：

- **职责不清**：SDK层的`RestToolExecutor`包含了过多具体逻辑  
- **功能耦合**：认证、转换、重试等高级功能与基础HTTP调用混合  
- **扩展性差**：难以添加新的认证方式或转换逻辑  
- **HTTP方法支持不完整**：仅支持GET方法，其他方法未正确实现  

---

## 现有架构分析

### SDK层执行器（当前）

- `BaseToolExecutor`：提供基础执行框架（超时、重试、参数验证）  
- `RestToolExecutor`：REST API调用（但功能不完整）  
- `StatelessToolExecutor`：无状态函数调用  
- `StatefulToolExecutor`：有状态工具调用  
- `McpToolExecutor`：MCP协议工具调用  

---

### 关键问题代码

```ts
// sdk/core/http/transport.ts:81 - 硬编码GET方法
const response = await fetch(fullUrl, {
  method: 'GET', // 硬编码，不支持其他HTTP方法
  headers,
  signal: controller.signal,
});
```

```ts
// sdk/core/tools/executors/rest.ts:72 - body参数被忽略
const options = {
  headers,
  query: queryParams,
  // body参数未传递给HttpTransport
};
```

---

## 架构重构方案

### 分层设计原则

#### SDK层（sdk/core/tools/executors/）

**职责**：提供最小化的基础执行器，只包含核心协议支持  

**设计原则**：
- 只实现基础协议功能  
- 不包含业务逻辑和复杂配置  
- 提供可扩展的接口  
- 保持简洁和通用性  

**改造后的RestToolExecutor**：

```ts
// 只负责基础HTTP调用，支持所有HTTP方法
export class RestToolExecutor extends BaseToolExecutor {
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, any>,
    threadContext?: ThreadContext
  ): Promise<any> {
    // 1. 提取基础参数
    // 2. 调用HttpTransport执行请求
    // 3. 返回原始响应
    // 不包含认证、转换等高级功能
  }
}
```

---

#### Packages层（packages/）

**职责**：提供增强的工具包装器，实现高级功能  

**模块结构**：

```
packages/
├── rest-tools/              # REST工具增强包
│   ├── src/
│   │   ├── enhanced-rest-tool.ts      # 主增强类
│   │   ├── auth/                       # 认证策略
│   │   │   ├── bearer-auth.ts
│   │   │   ├── basic-auth.ts
│   │   │   └── oauth2-auth.ts
│   │   ├── transformers/               # 转换器
│   │   │   ├── request-transformer.ts
│   │   │   └── response-transformer.ts
│   │   ├── retry/                      # 重试策略
│   │   │   └── retry-strategy.ts
│   │   └── index.ts
│   ├── package.json
│   └── tsconfig.json
│
└── tool-utils/              # 通用工具工具包
    └── ...
```

**设计原则**：
- 可组合、可扩展  
- 支持插件化架构  
- 提供常用工具函数  
- 与SDK层解耦  

---

#### 应用层（apps/）

**职责**：配置和使用packages提供的工具  

**使用方式**：

```ts
// 应用层可以自由组合SDK和Packages的功能
import { RestToolExecutor } from '@modular-agent-framework/sdk';
import { EnhancedRestTool, BearerAuth, RequestTransformer } from '@modular-agent-framework/rest-tools';

// 方式1：直接使用基础执行器
const baseExecutor = new RestToolExecutor();

// 方式2：使用增强工具
const enhancedTool = new EnhancedRestTool(baseExecutor, {
  auth: new BearerAuth('token'),
  requestTransformer: new CustomRequestTransformer(),
  retryStrategy: new ExponentialBackoffStrategy()
});
```

---

## 核心改进点

### 1. SDK层 - 重构RestToolExecutor

**当前问题**：
- 只支持GET方法  
- body参数未使用  
- 错误处理不完善  

**改造方案**：

```ts
// sdk/core/tools/executors/rest.ts
export class RestToolExecutor extends BaseToolExecutor {
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, any>,
    threadContext?: ThreadContext
  ): Promise<any> {
    const config = tool.config as RestToolConfig;
    
    // 提取参数
    const url = parameters['url'] || parameters['endpoint'];
    const method = (parameters['method'] || 'GET').toUpperCase();
    const body = parameters['body'];
    const headers = parameters['headers'];
    const queryParams = parameters['query'] || parameters['params'];

    // 验证必需参数
    if (!url) {
      throw new ValidationError('URL is required for REST tool', 'url', url);
    }

    // 创建HttpTransport实例
    const transport = new HttpTransport(
      config?.baseUrl,
      config?.headers,
      config?.timeout
    );

    try {
      // 执行请求 - 传递所有参数
      const response = await transport.execute(url, {
        method,
        headers,
        query: queryParams,
        body,  // 传递body参数
      });

      // 返回标准化响应
      return {
        url: this.buildFullUrl(config?.baseUrl || '', url, queryParams),
        method,
        status: response.status,
        statusText: this.getStatusText(response.status || 200),
        headers: response.headers,
        data: response.data
      };
    } catch (error) {
      // 错误处理...
    }
  }
}
```

---

### 2. SDK层 - 扩展HttpTransport

**当前问题**：
- 硬编码GET方法  
- 不支持请求体  

**改造方案**：

```ts
// sdk/core/http/transport.ts
export class HttpTransport implements Transport {
  async execute<T = any>(url: string, options?: TransportOptions): Promise<TransportResponse<T>> {
    // ...URL构建和参数处理...
    
    // 支持所有HTTP方法
    const method = options?.method || 'GET';
    
    // 准备请求体
    const body = this.prepareBody(options?.body, headers);
    
    const response = await fetch(fullUrl, {
      method,
      headers,
      body,
      signal: controller.signal,
    });
    
    // ...响应处理...
  }

  private prepareBody(body: any, headers: Record<string, string>): string | FormData | undefined {
    if (!body) return undefined;
    
    // 根据Content-Type处理body
    const contentType = headers['Content-Type'] || headers['content-type'];
    
    if (contentType?.includes('application/json')) {
      return JSON.stringify(body);
    }
    
    if (contentType?.includes('multipart/form-data')) {
      const formData = new FormData();
      Object.entries(body).forEach(([key, value]) => {
        formData.append(key, value as any);
      });
      return formData;
    }
    
    if (contentType?.includes('application/x-www-form-urlencoded')) {
      return new URLSearchParams(body).toString();
    }
    
    return body;
  }
}
```

---

### 3. Packages层 - 创建增强REST工具

```ts
// packages/rest-tools/src/enhanced-rest-tool.ts
import { RestToolExecutor } from '@modular-agent-framework/sdk';
import type { Tool, ToolExecutionOptions } from '@modular-agent-framework/sdk';

export interface EnhancedRestToolConfig {
  /** 认证策略 */
  auth?: AuthStrategy;
  /** 请求转换器 */
  requestTransformer?: RequestTransformer;
  /** 响应转换器 */
  responseTransformer?: ResponseTransformer;
  /** 重试策略 */
  retryStrategy?: RetryStrategy;
  /** 其他增强配置 */
  [key: string]: any;
}

export class EnhancedRestTool {
  constructor(
    private baseExecutor: RestToolExecutor,
    private config: EnhancedRestToolConfig
  ) {}

  async execute(
    tool: Tool,
    parameters: Record<string, any>,
    options?: ToolExecutionOptions
  ): Promise<any> {
    // 1. 应用请求转换
    if (this.config.requestTransformer) {
      parameters = await this.config.requestTransformer.transform(parameters);
    }

    // 2. 应用认证
    if (this.config.auth) {
      parameters = await this.config.auth.applyAuth(parameters);
    }

    // 3. 执行基础请求
    let result = await this.baseExecutor.execute(tool, parameters, options);

    // 4. 应用响应转换
    if (this.config.responseTransformer && result.success) {
      result.result = await this.config.responseTransformer.transform(result.result);
    }

    return result;
  }
}
```

---

### 4. Packages层 - 认证策略

```ts
// packages/rest-tools/src/auth/auth-strategy.ts
export interface AuthStrategy {
  applyAuth(parameters: Record<string, any>): Promise<Record<string, any>>;
}
```

```ts
// packages/rest-tools/src/auth/bearer-auth.ts
export class BearerAuth implements AuthStrategy {
  constructor(private token: string) {}

  async applyAuth(parameters: Record<string, any>): Promise<Record<string, any>> {
    return {
      ...parameters,
      headers: {
        ...parameters.headers,
        'Authorization': `Bearer ${this.token}`
      }
    };
  }
}
```

```ts
// packages/rest-tools/src/auth/basic-auth.ts
export class BasicAuth implements AuthStrategy {
  constructor(
    private username: string,
    private password: string
  ) {}

  async applyAuth(parameters: Record<string, any>): Promise<Record<string, any>> {
    const credentials = Buffer.from(`${this.username}:${this.password}`).toString('base64');
    return {
      ...parameters,
      headers: {
        ...parameters.headers,
        'Authorization': `Basic ${credentials}`
      }
    };
  }
}
```

---

### 5. Packages层 - 转换器

```ts
// packages/rest-tools/src/transformers/request-transformer.ts
export interface RequestTransformer {
  transform(request: Record<string, any>): Promise<Record<string, any>>;
}
```

```ts
// packages/rest-tools/src/transformers/response-transformer.ts
export interface ResponseTransformer {
  transform(response: any): Promise<any>;
}
```

```ts
// 示例：JSON请求转换器
export class JsonRequestTransformer implements RequestTransformer {
  async transform(request: Record<string, any>): Promise<Record<string, any>> {
    return {
      ...request,
      headers: {
        ...request.headers,
        'Content-Type': 'application/json'
      },
      body: request.body ? JSON.stringify(request.body) : undefined
    };
  }
}
```

---

## 迁移策略

### 阶段1：SDK层重构
- 修复`RestToolExecutor`的HTTP方法支持  
- 扩展`HttpTransport`的请求体处理能力  
- 保持向后兼容性  

### 阶段2：Packages层建设
- 创建`rest-tools`包  
- 实现基础认证策略  
- 实现基础转换器  

### 阶段3：应用层迁移
- 更新现有应用使用增强工具  
- 提供迁移示例和文档  
- 逐步迁移现有REST工具配置  

---

## 优势分析

- **职责分离**：各层职责清晰，避免功能耦合  
- **可扩展性**：通过packages层可轻松添加新功能  
- **可测试性**：各层可独立测试，易于mock  
- **灵活性**：应用层可自由组合不同策略  
- **向后兼容**：现有代码可逐步迁移，不影响现有功能  

---

## 实施计划

- **立即实施**：重构SDK层`RestToolExecutor`  
- **短期目标**：创建`packages/rest-tools`基础模块  
- **中期目标**：完善认证和转换策略  
- **长期目标**：建立完整的工具生态系统