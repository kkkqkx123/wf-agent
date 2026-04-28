# Anthropic SDK Internal 模块功能说明

## 概述

`internal` 目录包含 SDK 内部使用的工具和实用程序，这些模块不会在包外部导出，可能会在版本之间发生变化。该目录提供了底层功能支持，包括类型定义、环境检测、头部处理、请求选项、实用工具函数等。

## 目录结构

```
internal/
├── decoders/          # 数据解码器
│   ├── jsonl.ts       # JSONL 格式解码
│   └── line.ts        # 行解码器
├── utils/             # 实用工具函数
│   ├── bytes.ts       # 字节操作工具
│   ├── env.ts         # 环境变量读取
│   ├── log.ts         # 日志记录工具
│   ├── path.ts        # 路径处理工具
│   ├── sleep.ts       # 延迟工具
│   ├── uuid.ts        # UUID 生成工具
│   └── values.ts      # 值验证和转换工具
├── builtin-types.ts   # 内置类型定义
├── constants.ts       # 常量定义
├── detect-platform.ts # 平台检测
├── errors.ts          # 错误处理工具
├── headers.ts         # HTTP 头部处理
├── parse.ts           # 解析工具
├── README.md          # 内部模块说明
├── request-options.ts # 请求选项处理
├── shim-types.ts      # Shim 类型定义
├── shims.ts           # 平台兼容性垫片
├── stream-utils.ts    # 流处理工具
├── to-file.ts         # 文件转换工具
├── types.ts           # 类型定义
└── uploads.ts         # 上传处理工具
```

## 功能模块详解

### 1. 类型定义模块

#### types.ts
- 定义通用类型如 `PromiseOrValue<T>`、`HTTPMethod` 等
- 提供合并多种运行时 RequestInit 类型的 `MergedRequestInit`
- 定义 `KeysEnum<T>` 等辅助类型

#### builtin-types.ts
- 定义跨平台兼容的内置类型，如 `RequestInfo`、`RequestInit`、`Response` 等

#### shim-types.ts
- 提供平台兼容性的类型定义

### 2. 实用工具模块

#### utils/env.ts
- `readEnv(env: string)`: 从环境中读取变量，支持 Node.js 和 Deno 环境

#### utils/values.ts
- `isAbsoluteURL(url: string)`: 检查 URL 是否为绝对路径
- `isArray(val: unknown)`: 检查值是否为数组
- `isObj(obj: unknown)`: 检查值是否为对象
- `isEmptyObj(obj: Object | null | undefined)`: 检查对象是否为空
- `validatePositiveInteger(name: string, n: unknown)`: 验证正整数
- `coerceInteger(value: unknown)`: 强制转换为整数
- `safeJSON(text: string)`: 安全解析 JSON，失败时返回 undefined

#### utils/uuid.ts
- `uuid4()`: 生成 UUID v4

#### utils/sleep.ts
- `sleep(ms: number)`: 异步延迟指定毫秒数

#### utils/log.ts
- 提供日志记录功能，包括日志级别控制
- `loggerFor(obj: any)`: 获取对象的日志记录器
- `parseLogLevel(level: string | undefined, source: string, obj: any)`: 解析日志级别

#### utils/path.ts
- `path(strings: TemplateStringsArray, ...values: any[])`: 模板字符串路径构建器

#### utils/bytes.ts
- `encodeUTF8(str: string)`: UTF-8 编码字符串为字节数组
- `decodeUTF8(buffer: Uint8Array)`: 解码字节数组为 UTF-8 字符串
- `concatBytes(buffers: Uint8Array[])`: 连接字节数组

### 3. HTTP 相关模块

#### headers.ts
- `buildHeaders(newHeaders: HeadersLike[]): NullableHeaders`: 构建 HTTP 头部，支持显式设置 null 来取消默认头部
- `isEmptyHeaders(headers: HeadersLike)`: 检查头部是否为空
- 定义 `HeadersLike` 类型，支持多种头部格式

#### request-options.ts
- 定义 `RequestOptions` 接口，包含请求的各种选项
- 定义 `FinalRequestOptions` 接口，包含方法和路径
- 提供 `FallbackEncoder` 用于编码请求体

#### parse.ts
- 提供 API 响应解析功能
- `createResponseHeaders(headers: Headers): Record<string, string>`: 创建响应头部对象
- `castToError(error: any): Error`: 将任意值转换为错误对象

#### stream-utils.ts
- `ReadableStreamToAsyncIterable<T>(stream: any)`: 将 ReadableStream 转换为异步可迭代对象，提供浏览器和 Node.js 兼容性

### 4. 平台检测与兼容性

#### detect-platform.ts
- `isRunningInBrowser()`: 检测是否在浏览器环境中运行
- `getPlatformHeaders()`: 获取平台特定的头部信息

#### shims.ts
- 提供跨平台兼容的 fetch 实现
- `getDefaultFetch()`: 获取默认的 fetch 函数
- `CancelReadableStream(stream: any)`: 取消可读流

### 5. 数据解码模块

#### decoders/line.ts
- `LineDecoder`: 逐行解码文本数据，处理增量读取
- `findDoubleNewlineIndex(buffer: Uint8Array)`: 查找双换行符索引

#### decoders/jsonl.ts
- 提供 JSONL (JSON Lines) 格式的解码功能

### 6. 错误处理

#### errors.ts
- `castToError(error: any): Error`: 将任意值转换为错误对象
- `isAbortError(error: any): boolean`: 检查是否为中止错误

### 7. 上传处理

#### uploads.ts
- `isUploadable(value: any): boolean`: 检查值是否可上传
- `isBlobLike(value: any): boolean`: 检查值是否类似 Blob
- `isFileLike(value: any): boolean`: 检查值是否类似 File
- `createForm(data: UploadBody)`: 创建表单数据

#### to-file.ts
- `toFile`: 将各种输入转换为文件对象

### 8. 常量定义

#### constants.ts
- 定义 SDK 使用的常量值

## 设计原则

1. **内部使用**: 所有模块仅在 SDK 内部使用，不对外暴露
2. **跨平台兼容**: 提供对不同 JavaScript 运行时环境的支持
3. **类型安全**: 提供完整的 TypeScript 类型定义
4. **性能优化**: 针对常见操作进行优化
5. **错误处理**: 提供健壮的错误处理机制

## 注意事项

- 这些模块是内部实现细节，不应在外部代码中直接使用
- 在不同版本之间，这些模块的接口可能会发生变化
- 修改这些模块可能会影响整个 SDK 的功能