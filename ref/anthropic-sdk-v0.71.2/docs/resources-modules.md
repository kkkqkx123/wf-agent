# Anthropic SDK Resources 模块功能说明

## 概述

`resources` 目录包含 SDK 中与 Anthropic API 各个资源端点相对应的模块。这些模块提供了与 API 交互的高级接口，封装了 HTTP 请求和响应处理逻辑。

## 目录结构

```
resources/
├── beta/              # Beta API 资源
│   ├── messages/      # Beta 消息相关资源
│   ├── skills/        # Beta 技能相关资源
│   ├── beta.ts        # Beta API 主模块
│   ├── files.ts       # 文件相关资源
│   ├── index.ts       # Beta 资源导出
│   ├── messages.ts    # Beta 消息资源
│   └── models.ts      # Beta 模型资源
├── messages/          # 消息相关资源
│   ├── batches.ts     # 消息批处理
│   ├── index.ts       # 消息资源导出
│   └── messages.ts    # 消息资源实现
├── beta.ts            # Beta API 导出
├── completions.ts     # 补全资源
├── index.ts           # 主资源导出
├── messages.ts        # 消息资源导出
├── models.ts          # 模型资源
├── shared.ts          # 共享类型定义
└── top-level.ts       # 顶级类型定义
```

## 功能模块详解

### 1. 消息资源 (messages/messages.ts)

提供与 Anthropic 消息 API 交互的功能：

- `Messages` 类: 主要的消息资源类，包含创建消息、流式消息和计算令牌数的方法
- `create()`: 创建新消息，支持流式和非流式响应
- `stream()`: 创建消息流，返回 `MessageStream` 对象
- `countTokens()`: 计算消息中的令牌数量
- 定义了大量与消息相关的类型，如：
  - `Message`: 消息响应类型
  - `MessageParam`: 消息参数类型
  - `ContentBlock`: 内容块类型（文本、图像、工具使用等）
  - `ToolUnion`: 工具联合类型
  - `Model`: 支持的模型类型

### 2. 消息批处理 (messages/batches.ts)

处理批量消息操作：

- `Batches` 类: 批处理资源类
- `create()`: 创建消息批处理
- `retrieve()`: 检索批处理状态
- `list()`: 列出批处理
- `cancel()`: 取消批处理
- `delete()`: 删除批处理
- `results()`: 获取批处理结果

### 3. 补全资源 (completions.ts)

提供传统补全 API 支持：

- `Completions` 类: 补全资源类
- `create()`: 创建文本补全
- 定义了补全相关的类型，如：
  - `Completion`: 补全响应类型
  - `CompletionCreateParams`: 补全创建参数

### 4. 模型资源 (models.ts)

提供模型相关信息：

- `Models` 类: 模型资源类
- `retrieve()`: 获取模型详情
- `list()`: 列出可用模型
- 定义了模型相关的类型，如：
  - `ModelInfo`: 模型信息类型
  - `ModelListParams`: 模型列表参数
  - `ModelRetrieveParams`: 模型检索参数

### 5. Beta API 资源 (beta/)

提供实验性功能的 API 接口：

- `Beta` 类: Beta API 主类
- 支持多种 Beta 功能，如结构化输出、新的工具类型等
- `BetaMessages` 类: Beta 消息资源，扩展了标准消息功能
- 包含更多高级工具类型，如：
  - `BetaToolComputerUse`: 计算机使用工具
  - `BetaToolTextEditor`: 文本编辑工具
  - `BetaWebSearchTool`: 网络搜索工具
  - `BetaCodeExecutionTool`: 代码执行工具
  - `BetaMCPToolset`: MCP 工具集

### 6. 共享类型定义 (shared.ts)

定义在整个资源模块中共享的类型：

- `ResponseFormat`: 响应格式类型
- `ResponseFormatText`: 文本响应格式
- `ResponseFormatJSONObject`: JSON 对象响应格式
- `ResponseFormatJSONSchema`: JSON Schema 响应格式

### 7. 顶级类型定义 (top-level.ts)

定义 SDK 顶层的类型别名：

- 为各种资源类提供类型别名
- 包括各种错误类型和响应类型

## 设计原则

1. **资源导向**: 每个 API 资源都有对应的类来封装其功能
2. **类型安全**: 提供完整的 TypeScript 类型定义
3. **一致性**: 所有资源类都继承自 `APIResource` 基类
4. **灵活性**: 支持流式和非流式响应
5. **向后兼容**: 同时支持传统的补全 API 和现代的消息 API

## 使用场景

- Messages: 与 Anthropic 的消息 API 交互，适用于对话式 AI 应用
- Completions: 与传统的补全 API 交互，适用于文本生成任务
- Models: 查询可用的模型及其元数据
- Batches: 批量处理大量消息请求
- Beta: 访问实验性功能和高级工具