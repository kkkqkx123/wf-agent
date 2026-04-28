# 工具提示词处理架构

## 概述

本文档描述 Modular Agent Framework 中工具提示词的处理架构，包括工具描述生成、参数 Schema 处理、格式转换和可见性声明等功能。

## 架构设计

### 分层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    packages/prompt-templates                     │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  静态模板定义（纯数据，无逻辑）                                ││
│  │  - 工具描述模板（表格/单行/列表）                              ││
│  │  - 参数 Schema 模板                                          ││
│  │  - 可见性声明模板                                             ││
│  │  - 格式转换模板（XML/JSON）                                   ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                              ↓ 使用模板
┌─────────────────────────────────────────────────────────────────┐
│                           SDK 层                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  核心能力（通用、可复用）                                      ││
│  │  - ToolService: 工具注册、验证、执行                          ││
│  │  - ToolDescriptionGenerator: 描述生成（使用模板）              ││
│  │  - ToolVisibilityCoordinator: 可见性管理                      ││
│  │  - ToolSchemaFormatter: 格式转换（新增）                      ││
│  │    - toFunctionCallSchema(): 标准 JSON Schema                ││
│  │    - toXMLFormat(): XML 格式（使用模板）                      ││
│  │    - toJSONFormat(): JSON 文本格式（使用模板）                ││
│  │  - ToolSchemaCleaner: Schema 清理（适配不同 LLM）            ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                              ↓ 调用 SDK 能力
┌─────────────────────────────────────────────────────────────────┐
│                          Apps 层                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  应用定制（业务逻辑）                                         ││
│  │  - ToolPolicyManager: 工具策略（模式级过滤）                   ││
│  │  - PromptManager: 提示词组装（使用 SDK 能力）                 ││
│  │  - ChannelManager: 渠道适配（选择格式、清理 Schema）          ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## 职责划分

### 1. prompt-templates 包

**定位**：纯静态模板定义，不包含任何运行时逻辑。

**内容**：
- 工具描述模板（表格/单行/列表格式）
- 参数 Schema 模板
- 可见性声明模板
- 格式转换模板（XML/JSON）

### 2. SDK 层

**定位**：提供通用的核心能力，不包含业务逻辑。

**现有能力**：
- `ToolService`: 工具注册、验证、执行
- `ToolDescriptionGenerator`: 描述生成
- `ToolVisibilityCoordinator`: 可见性管理
- `ToolParametersDescriber`: 参数描述生成

**新增能力**：
- `ToolSchemaFormatter`: 格式转换
  - `toFunctionCallSchema()`: 转换为 Function Call 格式
  - `toXMLFormat()`: 转换为 XML 格式
  - `toJSONFormat()`: 转换为 JSON 文本格式
- `ToolSchemaCleaner`: Schema 清理
  - `cleanForGemini()`: 清理 Gemini 不兼容字段
  - `cleanForAnthropic()`: 清理 Anthropic 不兼容字段
  - `cleanForOpenAI()`: 清理 OpenAI 不兼容字段

### 3. Apps 层

**定位**：业务逻辑处理，灵活组合 SDK 能力。

**职责**：
- 工具策略过滤（模式级 toolPolicy）
- 渠道适配（选择格式、清理 Schema）
- 提示词组装（组装最终消息）

## 现有实现分析

### prompt-templates 包

| 模板类型 | 文件位置 | 用途 |
|---------|---------|------|
| 工具描述表格 | `templates/tools/descriptions/table-format.ts` | 生成工具表格 |
| 工具描述单行 | `templates/tools/descriptions/single-line.ts` | 生成单行描述 |
| 工具描述列表 | `templates/tools/descriptions/list-format.ts` | 生成列表描述 |
| 参数 Schema | `templates/tools/parameters/schema.ts` | 参数 Schema 描述 |
| 可见性声明 | `templates/tools/visibility/declaration.ts` | 工具可见性声明 |

### SDK 层

| 组件 | 文件位置 | 功能 |
|-----|---------|------|
| ToolService | `sdk/core/services/tool-service.ts` | 工具注册、验证、执行 |
| ToolDescriptionGenerator | `sdk/core/utils/tool-description-generator.ts` | 工具描述生成 |
| ToolParametersDescriber | `sdk/core/utils/tool-parameters-describer.ts` | 参数描述生成 |
| ToolVisibilityCoordinator | `sdk/core/execution/coordinators/tool-visibility-coordinator.ts` | 可见性管理 |
| ToolVisibilityMessageBuilder | `sdk/core/execution/utils/tool-visibility-message-builder.ts` | 可见性消息构建 |

### ref/Lim-Code 参考实现

| 组件 | 文件位置 | 功能 |
|-----|---------|------|
| PromptManager | `backend/modules/prompt/PromptManager.ts` | 提示词管理 |
| ChannelManager | `backend/modules/channel/ChannelManager.ts` | 渠道管理 |
| SettingsManager | `backend/modules/settings/SettingsManager.ts` | 设置管理 |
| xmlFormatter | `backend/tools/xmlFormatter.ts` | XML 格式转换 |
| jsonFormatter | `backend/tools/jsonFormatter.ts` | JSON 格式转换 |

## 数据流

### 工具调用流程

```
用户请求
    ↓
Apps 层: ToolPolicyManager.filterTools() → 获取允许的工具
    ↓
Apps 层: ChannelManager.buildRequest()
    ├── 选择格式转换器（FunctionCall/XML/JSON）
    ├── 调用 SDK: ToolSchemaCleaner.cleanForXxx()
    └── 调用 SDK: ToolSchemaFormatter.toXxxFormat()
    ↓
SDK 层: LLMExecutor.executeLLMCall() → 发送给 LLM
    ↓
SDK 层: ToolCallExecutor.executeToolCall() → 执行工具
    ↓
返回结果
```

### 可见性声明流程

```
作用域切换
    ↓
SDK 层: ToolVisibilityCoordinator.updateVisibilityOnScopeChange()
    ↓
SDK 层: ToolVisibilityMessageBuilder.buildVisibilityDeclarationMessage()
    ├── 使用 prompt-templates 模板
    └── 调用 ToolDescriptionGenerator 生成工具表格
    ↓
SDK 层: 添加声明消息到对话历史
    ↓
LLM 收到可见性声明
```

## 实现计划

### Phase 1: 补充 prompt-templates 模板

1. 添加 XML 格式转换模板
2. 添加 JSON 格式转换模板
3. 添加工具调用结果模板

### Phase 2: SDK 层新增能力

1. 实现 `ToolSchemaFormatter`
2. 实现 `ToolSchemaCleaner`
3. 编写单元测试

### Phase 3: 集成与文档

1. 更新 SDK 导出
2. 编写使用文档
3. 编写集成测试

## 设计原则

1. **单一职责**：每层只负责自己的职责
2. **依赖倒置**：SDK 依赖抽象（模板），Apps 依赖 SDK
3. **开闭原则**：新增格式只需添加模板和对应方法
4. **接口隔离**：不同格式转换使用独立方法

## 注意事项

1. **模板变量命名**：使用 `{{variableName}}` 格式
2. **Schema 清理**：不同 LLM 对 JSON Schema 支持不同，需要针对性清理
3. **可见性声明**：避免频繁声明导致上下文膨胀
4. **格式兼容性**：XML/JSON 格式主要用于不支持 Function Call 的模型
