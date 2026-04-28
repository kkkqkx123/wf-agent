# SDK错误系统分析文档

## 一、错误系统概述

SDK模块采用分层错误处理架构，提供统一的错误类型体系和处理机制。

### 核心设计原则
- **统一格式**：所有错误继承自`SDKError`，提供一致的错误结构
- **丰富上下文**：包含错误码、上下文信息、错误链（cause）
- **可序列化**：支持`toJSON()`方法，便于日志记录和传输
- **分层处理**：不同层次（HTTP、LLM、执行）有不同的处理策略

## 二、错误类型体系

### 2.1 基础错误类

#### SDKError（基类）
```typescript
export class SDKError extends Error {
  constructor(
    public readonly code: ErrorCode,      // 错误码枚举
    message: string,                       // 错误消息
    public readonly context?: Record<string, any>,  // 上下文数据
    public override readonly cause?: Error  // 错误链
  )
  
  toJSON(): Record<string, any>  // 序列化方法
}
```

**特性**：
- 包含错误码、消息、上下文、错误链
- 支持完整的错误追踪和调试
- 可序列化为JSON格式

### 2.2 具体错误类型（11个）

#### 1. ValidationError（验证错误）
```typescript
export class ValidationError extends SDKError {
  constructor(
    message: string,
    public readonly field?: string,      // 验证失败的字段
    public readonly value?: any,         // 验证失败的值
    context?: Record<string, any>
  )
}
```

**使用场景**：
- 参数验证失败（100+处使用）
- 配置验证失败
- JSON Schema验证失败
- 安全验证失败（路径、表达式）

**处理逻辑**：
- ❌ 不重试（数据问题，重试无效）
- ❌ 不触发熔断器
- ❌ 无降级策略

#### 2. ExecutionError（执行错误）
```typescript
export class ExecutionError extends SDKError {
  constructor(
    message: string,
    public readonly nodeId?: string,      // 失败的节点ID
    public readonly workflowId?: string,  // 工作流ID
    context?: Record<string, any>,
    cause?: Error
  )
}
```

**使用场景**：
- 节点执行失败（50+处使用）
- 工作流执行失败
- 线程状态转换失败
- 条件执行失败

**处理逻辑**：
- ❌ 不重试（业务逻辑错误）
- ❌ 不触发熔断器
- ❌ 无降级策略

#### 3. ConfigurationError（配置错误）
```typescript
export class ConfigurationError extends SDKError {
  constructor(
    message: string,
    public readonly configKey?: string,   // 配置键
    context?: Record<string, any>
  )
}
```

**使用场景**：
- LLM配置错误（Profile缺失）
- 工作流配置错误
- 系统配置错误

**处理逻辑**：
- ❌ 不重试（配置问题，需人工修复）
- ❌ 不触发熔断器
- ❌ 无降级策略

#### 4. TimeoutError（超时错误）
```typescript
export class TimeoutError extends SDKError {
  constructor(
    message: string,
    public readonly timeout: number,      // 超时时间（毫秒）
    context?: Record<string, any>
  )
}
```

**使用场景**：
- HTTP请求超时
- 工具执行超时
- 节点执行超时

**处理逻辑**：
- ✅ 需要重试（临时性问题）
- ✅ 触发熔断器
- ❌ 无降级策略

#### 5. NotFoundError（资源未找到错误）
```typescript
export class NotFoundError extends SDKError {
  constructor(
    message: string,
    public readonly resourceType: string,  // 资源类型
    public readonly resourceId: string,    // 资源ID
    context?: Record<string, any>
  )
}
```

**使用场景**：
- 工作流未找到
- 线程未找到
- Profile未找到
- 工具未找到

**处理逻辑**：
- ❌ 不重试（资源不存在）
- ❌ 不触发熔断器
- ❌ 无降级策略

#### 6. NetworkError（网络错误）
```typescript
export class NetworkError extends SDKError {
  constructor(
    message: string,
    public readonly statusCode?: number,   // HTTP状态码
    context?: Record<string, any>,
    cause?: Error
  )
}
```

**使用场景**：
- HTTP请求失败
- 连接被拒绝
- DNS解析失败

**处理逻辑**：
- ✅ 需要重试（网络波动）
- ✅ 触发熔断器
- ❌ 无降级策略

#### 7. LLMError（LLM调用错误）
```typescript
export class LLMError extends NetworkError {
  constructor(
    message: string,
    public readonly provider: string,      // LLM提供商
    public readonly model?: string,        // 模型名称
    statusCode?: number,
    context?: Record<string, any>,
    cause?: Error
  )
}
```

**使用场景**：
- LLM API调用失败
- 认证失败
- 配额不足
- 模型错误

**处理逻辑**：
- ✅ 可重试（临时故障）
- ✅ 触发熔断器
- ✅ 可降级（切换备用模型）

#### 8. ToolError（工具调用错误）
```typescript
export class ToolError extends SDKError {
  constructor(
    message: string,
    public readonly toolName?: string,     // 工具名称
    public readonly toolType?: string,     // 工具类型
    context?: Record<string, any>,
    cause?: Error
  )
}
```

**使用场景**：
- 工具执行失败
- 工具参数错误
- 工具未找到
- 工具认证失败

**处理逻辑**：
- ✅ 可重试（网络工具）
- ❌ 不触发熔断器
- ✅ 可降级（使用备用工具）

#### 9. RateLimitError（限流错误）
```typescript
export class RateLimitError extends SDKError {
  constructor(
    message: string,
    public readonly retryAfter?: number,   // 重试等待时间（秒）
    context?: Record<string, any>
  )
}
```

**使用场景**：
- API限流（429）
- 工具限流
- LLM限流

**处理逻辑**：
- ✅ 延迟重试（按retryAfter等待）
- ❌ 不触发熔断器
- ❌ 无降级策略

#### 10. CircuitBreakerOpenError（熔断器打开错误）
```typescript
export class CircuitBreakerOpenError extends SDKError {
  constructor(
    message: string,
    public readonly state?: string,        // 熔断器状态
    context?: Record<string, any>
  )
}
```

**使用场景**：
- 熔断器打开，拒绝请求
- 服务降级触发

**处理逻辑**：
- ❌ 不重试（熔断器保护中）
- ✅ 熔断器已打开
- ✅ 触发降级策略

#### 11. HttpError（HTTP错误）
```typescript
export class HttpError extends NetworkError {
  constructor(
    message: string,
    public override readonly statusCode: number,  // HTTP状态码
    context?: Record<string, any>,
    cause?: Error
  )
}
```

**使用场景**：
- HTTP请求失败（精确区分状态码）
- 4xx客户端错误（400、401、403、404等）
- 5xx服务器错误（500、502、503等）
- 429限流错误

**处理逻辑**：
- 429和5xx - ✅ 可重试
- 4xx（除429）- ❌ 不重试
- ✅ 触发熔断器
- ✅ 支持认证刷新（401、403）

**设计原则**：
- 只提供`statusCode`字段，不提供判断方法
- 业务逻辑由调用方根据`statusCode`自行判断
- 保持错误类简单，避免过度设计

## 三、错误处理机制

### 3.1 HTTP层错误处理

#### 重试处理器（RetryHandler）
```typescript
export class RetryHandler {
  // 指数退避重试策略
  async executeWithRetry<T>(fn: () => Promise<T>): Promise<T>
  
  // 判断错误是否可重试
  private shouldRetry(error: any): boolean
  
  // 计算重试延迟
  private calculateDelay(attempt: number): number
}
```

**重试策略**：
- `TimeoutError` - 超时重试
- `NetworkError` - 网络错误重试
- `RateLimitError` (429) - 限流重试（带延迟）
- HTTP 5xx - 服务器错误重试
- HTTP 4xx - 客户端错误不重试（除429）

#### 熔断器（CircuitBreaker）
```typescript
export class CircuitBreaker {
  // 三种状态：CLOSED（关闭）、OPEN（打开）、HALF_OPEN（半开）
  private state: CircuitState
  
  // 执行函数（带熔断保护）
  async execute<T>(fn: () => Promise<T>): Promise<T>
  
  // 检查熔断器是否打开
  isOpen(): boolean
  
  // 记录失败/成功
  private recordFailure(): void
  private recordSuccess(): void
}
```

**熔断策略**：
- 失败次数达到阈值 → 打开熔断器
- 打开状态持续超时时间 → 半开状态
- 半开状态成功次数达到阈值 → 关闭熔断器

#### HTTP客户端（HttpClient）
```typescript
export class HttpClient {
  // 统一HTTP请求处理
  async request<T>(options: HttpRequestOptions): Promise<HttpResponse<T>>
  
  // 超时处理（使用AbortController）
  private async executeRequest<T>(options: HttpRequestOptions): Promise<HttpResponse<T>>
  
  // 创建HTTP错误（返回HttpError）
  private createHttpError(status: number, message: string, url?: string): Error
}
```

**超时处理**：
- 使用`AbortController`实现请求超时
- 超时后抛出`TimeoutError`

**HTTP错误处理**：
- 429状态码 → `RateLimitError`
- 其他HTTP错误 → `HttpError`（精确区分状态码）
- 支持基于`statusCode`的精确重试策略

### 3.2 LLM层错误处理

#### LLM基础客户端（BaseClient）
```typescript
export abstract class BaseClient {
  // 处理错误，转换为LLMError
  protected handleError(error: any): SDKError
  
  // 执行HTTP POST请求
  protected async doHttpPost(url: string, body: any, options?: any): Promise<LLMResult>
  
  // 执行HTTP POST流式请求
  protected async* doHttpPostStream(url: string, body: any, options?: any): AsyncIterable<LLMResult>
}
```

**错误转换**：
- 所有LLM错误统一转换为`LLMError`
- 包含提供商、模型、状态码等信息
- 支持错误链追踪

### 3.3 执行层错误处理

#### 节点执行器（Node Executors）
```typescript
export abstract class BaseNodeExecutor {
  // 执行节点（带错误捕获）
  async execute(node: Node, context: ThreadContext): Promise<NodeExecutionResult>
  
  // 验证节点配置
  protected abstract validateConfig(config: any): void
}
```

**错误处理**：
- 每个节点执行器都有try-catch
- 验证错误 → `ValidationError`
- 执行错误 → `ExecutionError`
- 网络错误 → `NetworkError`/`LLMError`/`ToolError`

#### 触发器执行器（Trigger Executors）
```typescript
export abstract class BaseTriggerExecutor {
  // 执行触发器（错误隔离）
  async execute(trigger: Trigger, event: any): Promise<TriggerExecutionResult>
}
```

**错误处理**：
- 触发器错误不影响主流程
- 记录错误日志
- 返回失败结果

#### 线程生命周期管理（ThreadLifecycleManager）
```typescript
export class ThreadLifecycleManager {
  // 状态转换（带验证）
  startThread(thread: Thread): ThreadStartedEvent
  pauseThread(thread: Thread): ThreadPausedEvent
  resumeThread(thread: Thread): ThreadResumedEvent
  completeThread(thread: Thread): ThreadCompletedEvent
  failThread(thread: Thread, error: Error): ThreadFailedEvent
  cancelThread(thread: Thread): ThreadCancelledEvent
}
```

**错误处理**：
- 状态转换验证失败 → `ValidationError`
- 无效状态转换抛出错误

## 四、错误处理策略矩阵

| 错误类型 | 重试策略 | 熔断器 | 降级策略 | 日志级别 | 监控指标 |
|---------|---------|--------|---------|---------|---------|
| ValidationError | ❌ 不重试 | ❌ 不触发 | ❌ 无 | WARN | 验证失败次数 |
| ExecutionError | ❌ 不重试 | ❌ 不触发 | ❌ 无 | ERROR | 执行失败次数 |
| ConfigurationError | ❌ 不重试 | ❌ 不触发 | ❌ 无 | ERROR | 配置错误次数 |
| TimeoutError | ✅ 重试 | ✅ 触发 | ❌ 无 | WARN | 超时次数、重试次数 |
| NotFoundError | ❌ 不重试 | ❌ 不触发 | ❌ 无 | WARN | 资源未找到次数 |
| NetworkError | ✅ 重试 | ✅ 触发 | ❌ 无 | WARN | 网络错误次数、重试次数 |
| HttpError | 429/5xx重试 | ✅ 触发 | ✅ 可降级 | WARN | HTTP错误次数、状态码分布 |
| LLMError | ✅ 可重试 | ✅ 触发 | ✅ 可降级 | ERROR | LLM失败次数、降级次数 |
| ToolError | ✅ 可重试 | ❌ 不触发 | ✅ 可降级 | ERROR | 工具失败次数、降级次数 |
| RateLimitError | ✅ 延迟重试 | ❌ 不触发 | ❌ 无 | INFO | 限流次数、等待时间 |
| CircuitBreakerOpenError | ❌ 不重试 | ✅ 已打开 | ✅ 触发降级 | WARN | 熔断打开次数、持续时间 |

## 五、错误系统可用于实现的功能

### 5.1 监控和告警系统
- **错误率监控**：按错误类型、节点、工作流统计错误率
- **熔断器状态监控**：监控熔断器打开频率和持续时间
- **性能监控**：超时错误、重试次数等指标
- **告警规则**：基于错误阈值触发告警

### 5.2 调试和诊断工具
- **错误链路追踪**：利用错误链追踪错误根源
- **上下文信息展示**：展示错误发生时的上下文数据
- **重放机制**：基于checkpoint和错误信息重放失败节点
- **错误模式分析**：分析常见错误模式和根因

### 5.3 自动化恢复机制
- **智能重试策略**：基于错误类型动态调整重试策略
- **熔断器自动恢复**：监控熔断器状态，自动测试恢复
- **降级处理**：LLM错误时切换到备用模型
- **资源清理**：错误时自动清理资源（线程、临时文件等）

### 5.4 用户体验优化
- **友好的错误提示**：将技术错误转换为用户友好的消息
- **错误建议**：基于错误类型提供解决方案建议
- **错误分类展示**：按严重程度、类型分类展示错误
- **错误搜索和过滤**：支持按时间、类型、节点等维度搜索

### 5.5 质量保证
- **自动化测试**：基于错误场景生成测试用例
- **错误注入测试**：模拟各种错误情况测试系统健壮性
- **覆盖率分析**：分析错误处理代码的覆盖率
- **回归测试**：确保错误修复不会引入新问题

### 5.6 数据分析和报告
- **错误趋势分析**：分析错误随时间的变化趋势
- **根因分析报告**：自动生成错误根因分析报告
- **性能影响评估**：评估错误对性能的影响
- **合规性报告**：生成错误处理合规性报告

## 六、错误系统的优势

1. **统一的错误格式**：所有错误都继承自`SDKError`，格式一致
2. **丰富的上下文**：包含错误码、上下文、错误链等信息
3. **分层处理**：不同层次有不同的错误处理策略
4. **可扩展性**：易于添加新的错误类型和处理逻辑
5. **可序列化**：支持JSON序列化，便于日志和传输
6. **精确的错误分类**：`HttpError`精确区分HTTP状态码，支持精细化处理
7. **类型安全**：使用`instanceof`代替字符串匹配，利用TypeScript类型系统

## 七、错误码枚举

```typescript
export enum ErrorCode {
  /** 验证错误 */
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  /** 执行错误 */
  EXECUTION_ERROR = 'EXECUTION_ERROR',
  /** 配置错误 */
  CONFIGURATION_ERROR = 'CONFIGURATION_ERROR',
  /** 超时错误 */
  TIMEOUT_ERROR = 'TIMEOUT_ERROR',
  /** 资源未找到错误 */
  NOT_FOUND_ERROR = 'NOT_FOUND_ERROR',
  /** 网络错误 */
  NETWORK_ERROR = 'NETWORK_ERROR',
  /** LLM调用错误 */
  LLM_ERROR = 'LLM_ERROR',
  /** 工具调用错误 */
  TOOL_ERROR = 'TOOL_ERROR',
  /** 限流错误 */
  RATE_LIMIT_ERROR = 'RATE_LIMIT_ERROR',
  /** 熔断器打开错误 */
  CIRCUIT_BREAKER_OPEN_ERROR = 'CIRCUIT_BREAKER_OPEN_ERROR'
}
```

## 八、相关文件

- **错误类型定义**：`sdk/types/errors.ts`
- **重试处理器**：`sdk/core/http/retry-handler.ts`
- **熔断器**：`sdk/core/http/circuit-breaker.ts`
- **HTTP客户端**：`sdk/core/http/http-client.ts`
- **LLM基础客户端**：`sdk/core/llm/base-client.ts`
- **工具执行器**：`sdk/core/tools/base-tool-executor.ts`

## 九、优化记录

### 9.1 已完成的优化

#### 1. 新增HttpError类
- **文件**：`sdk/types/errors.ts`
- **目的**：精确区分HTTP状态码，支持精细化重试策略
- **特性**：
  - 继承自`NetworkError`
  - 提供`statusCode`字段
  - 不提供判断方法，业务逻辑由调用方决定
- **影响**：
  - 提高重试策略精确性
  - 支持认证刷新（401、403）
  - 支持降级策略（429）

#### 2. 优化RetryHandler
- **文件**：`sdk/core/http/retry-handler.ts`
- **修改**：使用`instanceof`代替字符串匹配
- **改进**：
  - 利用TypeScript类型系统
  - 提高代码可维护性
  - 避免字符串匹配的不稳定性
- **重试策略**：
  - `TimeoutError` - 超时重试
  - `HttpError` - 429和5xx重试
  - `NetworkError` - 其他网络错误重试
  - `RateLimitError` - 限流重试

#### 3. 优化BaseToolExecutor
- **文件**：`sdk/core/tools/base-tool-executor.ts`
- **修改**：使用`instanceof`代替字符串匹配
- **改进**：
  - 与RetryHandler保持一致的重试逻辑
  - 提高代码可维护性

#### 4. 修改HttpClient
- **文件**：`sdk/core/http/http-client.ts`
- **修改**：创建`HttpError`代替`NetworkError`
- **改进**：
  - 精确区分HTTP状态码
  - 429状态码返回`RateLimitError`
  - 其他HTTP错误返回`HttpError`

### 9.2 设计原则

**错误类设计原则**：
- 错误类应该保持简单，只包含错误信息
- 业务逻辑应该由调用方决定
- 避免在错误类中添加判断方法
- 只提供必要的字段，让调用方根据需要处理

**为什么不在错误类中添加判断方法**：
1. **单一职责**：错误类只负责携带错误信息，不负责业务逻辑
2. **灵活性**：调用方可以根据实际需求自由判断，不受预定义方法限制
3. **可维护性**：业务逻辑变化时不需要修改错误类
4. **避免过度设计**：不是所有场景都需要这些判断方法

