# Anthropic SDK Helpers 模块功能说明

## 概述

`helpers` 目录包含 SDK 的辅助功能模块，主要用于简化复杂操作和提供便捷的工具函数。当前主要包含 beta 功能相关的辅助工具。

## 目录结构

```
helpers/
└── beta/              # Beta 功能辅助工具
    ├── json-schema.ts # JSON Schema 工具
    ├── memory.ts      # 记忆工具
    └── zod.ts         # Zod 集成工具
```

## 功能模块详解

### 1. JSON Schema 工具 (beta/json-schema.ts)

提供基于 JSON Schema 的工具和输出格式创建功能：

- `betaTool<Schema>()`: 创建一个带有 JSON Schema 的工具，可以传递给 `.toolRunner()` 方法
  - 参数验证：确保输入的 schema 是对象类型
  - 自动验证输入参数
  - 返回 `BetaRunnableTool` 实例

- `betaJSONSchemaOutputFormat<Schema>()`: 创建基于 JSON Schema 的输出格式对象
  - 如果传递给 `.parse()` 方法，响应消息将包含 `.parsed_output` 属性
  - 包含内容解析功能
  - 支持 schema 转换选项

### 2. Zod 集成工具 (beta/zod.ts)

提供与 Zod 验证库的集成：

- `betaZodOutputFormat<ZodInput>()`: 创建基于 Zod schema 的输出格式对象
  - 将 Zod schema 转换为 JSON Schema
  - 自动解析响应内容
  - 包含错误处理机制

- `betaZodTool<InputSchema>()`: 创建使用 Zod schema 的工具
  - 将 Zod schema 自动转换为 JSON Schema
  - 传递给 API 时进行输入参数验证
  - 返回 `BetaRunnableTool` 实例

### 3. 记忆工具 (beta/memory.ts)

提供记忆功能相关的工具：

- `MemoryToolHandlers`: 定义记忆工具处理器类型
  - 为每种命令类型提供对应的处理器函数

- `betaMemoryTool(handlers)`: 创建记忆工具
  - 接收处理器对象
  - 根据命令类型调用相应的处理器
  - 返回 `BetaRunnableTool` 实例

## 设计原则

1. **简化集成**: 提供便捷的方法来集成第三方库（如 Zod）
2. **类型安全**: 利用 TypeScript 泛型确保类型安全
3. **自动验证**: 在运行时自动验证输入参数
4. **易用性**: 提供简单的 API 来创建复杂的工具和输出格式

## 使用场景

- JSON Schema 工具: 当需要定义具有严格输入验证的自定义工具时
- Zod 集成: 当项目已使用 Zod 进行数据验证时，提供无缝集成
- 记忆工具: 在需要实现记忆相关功能的场景中使用
- 输出格式: 当需要对 API 响应进行结构化解析时