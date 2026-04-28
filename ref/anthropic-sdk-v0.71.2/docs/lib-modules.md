# Anthropic SDK Lib 模块功能说明

## 概述

`lib` 目录包含 SDK 的高级功能实现，主要包括消息流处理、工具运行器、输出格式解析等功能。这些模块提供了更高级别的抽象，使开发者能够更容易地处理复杂的消息交互和工具执行流程。

## 目录结构

```
lib/
├── tools/                      # 工具相关功能
│   ├── BetaRunnableTool.ts     # Beta 可运行工具定义
│   ├── BetaToolRunner.ts       # Beta 工具运行器
│   ├── CompactionControl.ts    # 压缩控制
│   └── ToolRunner.ts           # 工具运行器
├── .keep                       # 占位文件
├── beta-parser.ts              # Beta 解析器
├── BetaMessageStream.ts        # Beta 消息流
├── MessageStream.ts            # 消息流
└── transform-json-schema.ts    # JSON Schema 转换
```

## 功能模块详解

### 1. 消息流处理 (MessageStream.ts)

提供标准消息流处理功能：

- `MessageStream`: 消息流处理类，实现异步迭代
- 支持多种事件监听：连接、流事件、文本、引用、JSON 输入、思考、签名、消息等
- 提供流控制方法：`on()`, `off()`, `once()`, `emitted()`, `done()`
- 支持流式响应和可读流转换
- 提供最终消息和文本获取方法
- 支持手动中止和错误处理

### 2. Beta 消息流处理 (BetaMessageStream.ts)

提供 Beta 版本的消息流处理功能：

- `BetaMessageStream<ParsedT>`: Beta 消息流处理类，支持解析类型
- 继承自标准消息流但增加了对 Beta API 的支持
- 支持结构化输出解析
- 提供 `finalMessage()` 方法返回解析后的消息

### 3. Beta 解析器 (beta-parser.ts)

处理 Beta API 的消息解析：

- `BetaParseableMessageCreateParams`: 可解析的消息创建参数类型
- `AutoParseableBetaOutputFormat<ParsedT>`: 自动可解析的 Beta 输出格式
- `ParsedBetaMessage<ParsedT>`: 解析后的 Beta 消息类型
- `maybeParseBetaMessage()`: 根据参数决定是否解析消息
- `parseBetaMessage()`: 解析消息内容

### 4. JSON Schema 转换 (transform-json-schema.ts)

转换和规范化 JSON Schema：

- `transformJSONSchema(jsonSchema)`: 转换 JSON Schema 为严格的格式
- 支持对象、字符串、数组类型的转换
- 移除额外属性，添加必需字段验证
- 支持特定字符串格式（日期时间、邮箱、URI等）
- 处理引用、定义、联合类型等复杂结构

### 5. 工具运行器 (tools/ToolRunner.ts)

处理工具执行的自动化循环：

- `BetaToolRunner<Stream>`: 工具运行器类，处理助手与工具之间的对话循环
- 支持同步和异步消息处理
- 自动处理工具使用和结果返回
- 提供迭代限制防止无限循环
- 实现异步迭代器协议
- 提供参数更新和消息推送功能
- 支持等待完成和运行直到完成的方法

### 6. Beta 可运行工具 (tools/BetaRunnableTool.ts)

定义可运行工具的接口：

- `BetaRunnableTool<Input>`: Beta 可运行工具接口
- 包含运行函数和解析函数
- 扩展自 `BetaToolUnion` 类型

### 7. Beta 工具运行器 (tools/BetaToolRunner.ts)

处理 Beta 版本的工具运行：

- 提供 Beta 版本的工具运行器实现
- 处理 Beta API 的特殊要求

## 设计原则

1. **异步处理**: 充分利用异步迭代器和生成器处理流式数据
2. **类型安全**: 使用泛型确保类型安全的解析和处理
3. **自动化**: 自动处理工具执行循环和消息流转
4. **可扩展**: 提供丰富的事件和钩子供用户自定义行为
5. **错误处理**: 全面的错误处理和恢复机制

## 使用场景

- MessageStream: 处理实时消息流，如聊天应用或实时通知
- BetaMessageStream: 处理 Beta API 的消息流，支持结构化输出
- ToolRunner: 自动化处理工具调用和结果处理的场景
- beta-parser: 需要解析结构化输出的场景
- transform-json-schema: 验证和转换 JSON Schema 的场景