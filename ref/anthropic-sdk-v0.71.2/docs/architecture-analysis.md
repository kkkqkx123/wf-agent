# Anthropic SDK 架构分析

## 项目概述

这是一个 Anthropic API 的官方 TypeScript SDK，提供了与 Anthropic 服务交互的能力。项目采用了模块化设计，包含主 SDK 和多个特定平台的 SDK（如 Bedrock、Foundry、Vertex）。

## src 目录架构分析

src 目录是主 SDK 的核心实现，采用分层架构设计：

### 核心目录结构

```
src/
├── _vendor/           # 第三方依赖或供应商代码
├── core/              # 核心功能实现
│   ├── api-promise.ts # API 调用的 Promise 封装
│   ├── error.ts       # 错误类型定义
│   ├── pagination.ts  # 分页功能实现
│   ├── resource.ts    # 资源基类定义
│   ├── streaming.ts   # 流式响应处理
│   └── uploads.ts     # 文件上传功能
├── helpers/           # 辅助函数
│   └── beta/          # Beta 功能相关辅助
├── internal/          # 内部工具和实用程序
│   ├── decoders/      # 数据解码器
│   ├── utils/         # 实用工具函数
│   └── ...            # 其他内部模块
├── lib/               # 库级功能
│   ├── tools/         # 工具相关功能
│   └── ...            # 其他库功能
├── resources/         # API 资源定义
│   ├── beta/          # Beta API 资源
│   ├── messages/      # 消息相关资源
│   └── ...            # 其他资源模块
├── client.ts          # 主客户端实现
├── index.ts           # 入口文件
└── ...                # 其他顶层模块
```

### 主要组件说明

1. **client.ts**: 主客户端类，负责处理 API 请求、认证、重试逻辑等。
2. **core/**: 包含核心功能如错误处理、分页、流式传输等。
3. **resources/**: 定义了所有可用的 API 资源，如消息、补全、模型等。
4. **internal/**: 包含内部使用的工具函数和类型定义，不对外暴露。
5. **lib/**: 提供高级功能，如工具支持、消息流处理等。

## packages 目录架构分析

packages 目录包含了针对不同平台的特定 SDK 实现：

### 平台特定 SDK 结构

```
packages/
├── bedrock-sdk/       # AWS Bedrock 平台的 Anthropic SDK
├── foundry-sdk/       # Foundry 平台的 Anthropic SDK
└── vertex-sdk/        # Google Vertex AI 平台的 Anthropic SDK
```

### 各 SDK 共同结构

每个平台特定的 SDK 都遵循相同的目录结构：

```
platform-sdk/
├── examples/          # 使用示例
├── scripts/           # 构建和开发脚本
├── src/               # 源代码
│   ├── core/          # 平台特定的核心功能
│   ├── client.ts      # 平台特定的客户端实现
│   └── index.ts       # 入口文件
├── tests/             # 测试文件
├── package.json       # 包配置
├── README.md          # 文档
└── ...                # 构建和配置文件
```

### 平台特定实现特点

1. **继承主 SDK**: 每个平台 SDK 都继承自主 SDK 的 BaseAnthropic 类
2. **认证适配**: 实现平台特定的认证机制（如 AWS 签名认证）
3. **请求转换**: 处理平台特定的请求格式转换
4. **功能裁剪**: 根据平台能力裁剪部分功能（如 Bedrock 不支持 token 计数）

## 设计模式和架构原则

1. **模块化设计**: 功能按职责分离到不同目录和模块
2. **继承机制**: 平台特定 SDK 继承主 SDK 基础功能
3. **配置驱动**: 通过 ClientOptions 进行灵活配置
4. **错误处理**: 统一的错误处理机制
5. **可扩展性**: 支持自定义 fetch 函数、超时、重试等
6. **类型安全**: 完整的 TypeScript 类型定义

## 关键特性

1. **多平台支持**: 支持标准 Anthropic API 及云平台集成（AWS Bedrock、Google Vertex、Foundry）
2. **流式响应**: 支持流式处理 API 响应
3. **自动重试**: 内置请求重试机制
4. **认证管理**: 支持多种认证方式
5. **分页处理**: 自动处理分页 API 响应
6. **类型安全**: 完整的 TypeScript 支持

## 总结

该 SDK 采用清晰的分层架构，将通用功能与平台特定功能分离，既保证了代码复用又支持了各平台的独特需求。src 目录提供核心功能，packages 目录提供平台特定的适配层，整体设计合理且易于维护和扩展。