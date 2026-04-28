# Anthropic SDK Core 模块功能说明

## 概述

`core` 目录包含 SDK 的核心功能实现，这些模块实现了非特定资源的 SDK 功能，为整个 SDK 提供基础支持。

## 目录结构

```
core/
├── api-promise.ts     # API Promise 封装
├── error.ts           # 错误类型定义
├── pagination.ts      # 分页功能实现
├── README.md          # 核心模块说明
├── resource.ts        # 资源基类定义
├── streaming.ts       # 流式响应处理
└── uploads.ts         # 上传功能实现
```

## 功能模块详解

### 1. API Promise 封装 (api-promise.ts)

提供了一个增强版的 Promise 实现，用于处理 API 调用：

- `APIPromise<T>`: 继承自 Promise，提供额外的辅助方法
- `asResponse()`: 获取原始 Response 实例
- `withResponse()`: 获取解析后的响应数据、原始 Response 实例和请求 ID
- `_thenUnwrap<U>()`: 允许在解析响应后应用转换函数

### 2. 错误处理 (error.ts)

定义了 SDK 中使用的各种错误类型：

- `AnthropicError`: 基础错误类
- `APIError`: API 相关错误，包含状态码、头部和错误体
- `APIUserAbortError`: 用户中止请求错误
- `APIConnectionError`: 连接错误
- `APIConnectionTimeoutError`: 连接超时错误
- 特定 HTTP 状态码错误：
  - `BadRequestError` (400)
  - `AuthenticationError` (401)
  - `PermissionDeniedError` (403)
  - `NotFoundError` (404)
  - `ConflictError` (409)
  - `UnprocessableEntityError` (422)
  - `RateLimitError` (429)
  - `InternalServerError` (>=500)

### 3. 分页功能 (pagination.ts)

提供分页 API 的支持：

- `AbstractPage<Item>`: 抽象分页类，实现异步迭代
- `PagePromise<PageClass, Item>`: 分页请求的 Promise 封装
- `Page<Item>`: 标准分页实现，支持游标分页
- `TokenPage<Item>`: 基于令牌的分页实现
- `PageCursor<Item>`: 基于游标的分页实现
- 支持自动分页迭代：`for await (const item of client.items.list())`

### 4. 资源基类 (resource.ts)

定义了所有 API 资源的基类：

- `APIResource`: 所有 API 资源的基类，包含对客户端的引用

### 5. 流式处理 (streaming.ts)

提供流式响应处理功能：

- `Stream<Item>`: 流式数据处理类，实现异步迭代
- `fromSSEResponse()`: 从服务器发送事件 (SSE) 响应创建流
- `fromReadableStream()`: 从 ReadableStream 创建流
- `tee()`: 将流拆分为两个独立的流
- `_iterSSEMessages()`: 迭代 SSE 消息的辅助函数
- 支持 React Native 等特殊环境的兼容性处理

### 6. 上传功能 (uploads.ts)

处理文件上传相关的功能（具体实现需查看文件内容）。

## 设计原则

1. **可扩展性**: 通过抽象类允许扩展和自定义实现
2. **类型安全**: 完整的 TypeScript 类型定义
3. **异步友好**: 充分利用异步迭代器和生成器
4. **错误处理**: 全面的错误分类和处理机制
5. **性能优化**: 高效的数据处理和内存管理

## 使用场景

- APIPromise: 用于所有 API 调用的返回值封装
- Error: 提供统一的错误处理机制
- Pagination: 处理分页 API 响应
- Resource: 作为所有 API 资源的基类
- Streaming: 处理流式 API 响应，如实时聊天或事件流