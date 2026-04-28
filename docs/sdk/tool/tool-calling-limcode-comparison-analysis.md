# SDK 工具调用实现与 LimCode 对比分析

## 概述

本文档对比分析当前 SDK 的工具调用实现与 LimCode 的实现差异，识别功能缺口并提出改进建议。

---

## 一、LimCode 工具调用架构

### 1.1 三种工具调用模式

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| `function_call` | 使用 API 原生的函数调用功能 | 支持 function-calling 的模型 |
| `xml` | 使用 XML 格式在提示词中描述工具 | 不原生支持 function-calling 的模型 |
| `json` | 使用 JSON 格式在提示词中描述工具 | 不原生支持 function-calling 的模型 |

### 1.2 核心组件

```
backend/
├── tools/
│   ├── xmlFormatter.ts       # XML 格式转换器
│   ├── jsonFormatter.ts      # JSON 格式转换器
│   ├── ToolRegistry.ts       # 工具注册器
│   └── utils.ts              # 工具辅助函数
└── modules/
    ├── api/
    │   └── chat/
    │       └── services/
    │           ├── ToolCallParserService.ts      # 工具调用解析服务
    │           ├── ToolExecutionService.ts       # 工具执行服务
    │           └── ToolIterationLoopService.ts   # 工具迭代循环服务
    └── channel/
        └── formatters/
            ├── gemini.ts       # Gemini 格式转换器
            ├── openai.ts       # OpenAI 格式转换器
            └── anthropic.ts    # Anthropic 格式转换器
```

### 1.3 XML 格式定义

**工具声明：**
```xml
<tool name="tool_name">
  <description>工具描述</description>
  <parameters>
    - param1 (required) [type]: 参数描述
    - param2 (optional) [type]: 参数描述
  </parameters>
</tool>
```

**工具调用：**
```xml
<tool_use>
  <tool_name>tool name here</tool_name>
  <parameters>
    <parameter_name>value</parameter_name>
    <array_param>
      <item>value1</item>
      <item>value2</item>
    </array_param>
  </parameters>
</tool_use>
```

**工具响应：**
```xml
<tool_result tool="tool_name">
{
  "success": true,
  "data": "..."
}
</tool_result>
```

### 1.4 JSON 格式定义

```
<<<TOOL_CALL>>>
{
  "tool": "tool_name",
  "parameters": {
    "param1": "value1",
    "param2": ["item1", "item2"]
  }
}
<<<END_TOOL_CALL>>>
```

---

## 二、当前 SDK 实现概览

### 2.1 核心组件

| 组件 | 文件路径 | 功能 |
|------|----------|------|
| ToolCallParser | `sdk/core/llm/formatters/tool-call-parser.ts` | XML/JSON 格式解析 |
| BaseFormatter | `sdk/core/llm/formatters/base.ts` | 委托解析方法给 ToolCallParser |
| OpenAIChatFormatter | `sdk/core/llm/formatters/openai-chat.ts` | 仅支持原生 function-calling |
| AnthropicFormatter | `sdk/core/llm/formatters/anthropic.ts` | 仅支持原生 function-calling |
| GeminiNativeFormatter | `sdk/core/llm/formatters/gemini-native.ts` | 仅支持原生 function-calling |
| ToolConverter | `packages/common-utils/src/tool/converter.ts` | 仅转换原生格式 |
| ToolCallExecutor | `sdk/core/execution/executors/tool-call-executor.ts` | 工具执行逻辑 |
| ToolDescriptionGenerator | `sdk/core/utils/tool-description-generator.ts` | 生成文本格式工具描述 |

### 2.2 已实现功能

✅ **工具调用解析** (`ToolCallParser`)
- XML 格式解析 (`parseXMLToolCalls`)
- JSON 格式解析 (`parseJSONToolCalls`)
- 自动格式检测 (`parseFromText`)

✅ **原生 Function-calling 支持**
- OpenAI Chat API 格式
- Anthropic Messages API 格式
- Gemini Native API 格式

✅ **工具执行**
- 并行工具调用执行
- 检查点管理
- 事件触发
- 错误处理

---

## 三、功能缺口对比

### 3.1 缺少工具模式 (toolMode) 配置

**LimCode 实现：**
```typescript
interface ChannelConfig {
  toolMode: 'function_call' | 'xml' | 'json'
}
```

**当前 SDK 问题：**
- `LLMProfile` 中没有 `toolMode` 字段
- `FormatterConfig` 中没有 `toolMode` 选项
- 无法配置使用 XML/JSON 模式进行工具调用

**影响：**
- 所有工具调用只能通过原生 function-calling
- 不支持不支持原生 function-calling 的模型

### 3.2 缺少工具声明格式生成

**LimCode 实现：**
```typescript
// xmlFormatter.ts
function convertToolsToXML(tools: Tool[]): string
function convertHistoryToXMLMode(history: Message[]): Message[]

// jsonFormatter.ts
function convertToolsToJSON(tools: Tool[]): string
function convertHistoryToJSONMode(history: Message[]): Message[]
```

**当前 SDK 问题：**
- `ToolDescriptionGenerator` 只生成表格/列表格式的文本描述
- `convertToolsTo*Format` 只支持原生 API 格式 (OpenAI/Anthropic/Gemini)
- 没有生成 XML/JSON 格式工具声明的功能

**示例对比：**

当前 SDK 生成的描述（表格格式）：
```
| 工具名称 | 工具ID | 说明 |
|----------|--------|------|
| Calculator | calculator | Performs calculations |
```

LimCode XML 格式（模型可直接使用）：
```xml
<tool name="calculator">
  <description>Performs calculations</description>
  <parameters>
    - a (required) [number]: First number
    - b (required) [number]: Second number
  </parameters>
</tool>
```

### 3.3 缺少历史记录转换

**LimCode 实现：**
```typescript
// 将 function_call 历史记录转换为 XML 文本
function convertHistoryToXMLMode(history: Message[]): Message[]

// 将 function_call 历史记录转换为 JSON 文本
function convertHistoryToJSONMode(history: Message[]): Message[]
```

**当前 SDK 问题：**
- 没有历史记录格式转换功能
- 无法在 XML/JSON 模式下正确传递历史工具调用
- `MessageBuilder` 只构建标准消息格式

**转换示例：**
```typescript
// 原始消息（function_call 模式）
{
  role: 'assistant',
  content: '',
  toolCalls: [{
    id: 'call_123',
    function: { name: 'calculator', arguments: '{"a":1,"b":2}' }
  }]
}

// 转换为 XML 模式
{
  role: 'assistant',
  content: '<tool_use>\n  <tool_name>calculator</tool_name>\n  <parameters>\n    <a>1</a>\n    <b>2</b>\n  </parameters>\n</tool_use>'
}
```

### 3.4 Formatter 缺少模式感知

**LimCode 实现：**
```typescript
class OpenAIFormatter {
  buildRequest(request, config) {
    if (config.toolMode === 'xml') {
      // 1. 将工具声明转为 XML
      // 2. 追加到系统提示词
      // 3. 不使用原生 tools 字段
    } else {
      // 使用原生 function-calling
    }
  }
  
  parseResponse(data, config) {
    if (config.toolMode === 'xml') {
      // 从响应文本中解析 XML 工具调用
      return parseXMLToolCalls(data.content)
    }
    // 原生解析
  }
}
```

**当前 SDK 问题：**
- 所有 Formatters 只实现原生 function-calling
- 没有根据 toolMode 切换逻辑
- 响应解析只依赖原生 API 结构

### 3.5 流式处理差异

**LimCode 实现：**
- `StreamAccumulator.ts`: 累加部分 tool_calls
- `input_json_delta` 处理 (Anthropic)
- 部分参数合并

**当前 SDK 问题：**
- 流式响应主要处理原生格式
- XML/JSON 模式的流式解析不完善

---

## 四、改进建议

### 阶段 1: 基础配置（高优先级）

#### 4.1 添加 toolMode 到 LLMProfile

```typescript
// packages/types/src/llm/profile.ts
export interface LLMProfile {
  // ... 现有字段
  
  /**
   * 工具调用模式
   * - 'function_call': 使用 API 原生函数调用
   * - 'xml': 使用 XML 格式在提示词中描述工具
   * - 'json': 使用 JSON 格式在提示词中描述工具
   * @default 'function_call'
   */
  toolMode?: 'function_call' | 'xml' | 'json';
}
```

#### 4.2 更新 FormatterConfig

```typescript
// sdk/core/llm/formatters/types.ts
export interface FormatterConfig {
  // ... 现有字段
  
  /** 工具调用模式 */
  toolMode?: 'function_call' | 'xml' | 'json';
}
```

### 阶段 2: 工具声明格式生成

#### 4.3 创建 ToolDeclarationFormatter

```typescript
// packages/common-utils/src/tool/declaration-formatter.ts

export interface ToolDeclarationFormatOptions {
  format: 'xml' | 'json' | 'native';
  includeDescription?: boolean;
  includeParameters?: boolean;
}

export class ToolDeclarationFormatter {
  /**
   * 将工具声明转换为 XML 格式
   */
  static toXML(tools: ToolSchema[]): string;
  
  /**
   * 将工具声明转换为 JSON 格式
   */
  static toJSON(tools: ToolSchema[]): string;
  
  /**
   * 根据提供商转换
   */
  static toNative(tools: ToolSchema[], provider: string): any;
}
```

#### 4.4 实现 XML 格式生成

```typescript
// 生成示例
function generateXMLToolDeclaration(tool: ToolSchema): string {
  return `<tool name="${tool.id}">
  <description>${tool.description}</description>
  <parameters>
${generateXMLParameters(tool.parameters)}
  </parameters>
</tool>`;
}
```

### 阶段 3: Formatter 集成

#### 4.5 增强 BaseFormatter

```typescript
abstract class BaseFormatter {
  buildRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    switch (config.toolMode) {
      case 'xml':
        return this.buildXMLModeRequest(request, config);
      case 'json':
        return this.buildJSONModeRequest(request, config);
      default:
        return this.buildNativeRequest(request, config);
    }
  }
  
  parseResponse(data: any, config: FormatterConfig): LLMResult {
    switch (config.toolMode) {
      case 'xml':
        return this.parseXMLModeResponse(data, config);
      case 'json':
        return this.parseJSONModeResponse(data, config);
      default:
        return this.parseNativeResponse(data, config);
    }
  }
  
  // 子类实现
  protected abstract buildXMLModeRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult;
  protected abstract parseXMLModeResponse(data: any, config: FormatterConfig): LLMResult;
}
```

#### 4.6 实现 OpenAI XML 模式

```typescript
class OpenAIChatFormatter extends BaseFormatter {
  protected buildXMLModeRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    // 1. 转换工具声明为 XML
    const toolsXML = ToolDeclarationFormatter.toXML(request.tools);
    
    // 2. 追加到系统提示词
    const messages = this.injectToolsToSystemMessage(request.messages, toolsXML);
    
    // 3. 转换历史记录
    const convertedMessages = this.convertHistoryToXMLMode(messages);
    
    // 4. 构建请求（不使用 tools 字段）
    return {
      httpRequest: {
        // ... 不使用 tools 字段
      }
    };
  }
  
  protected parseXMLModeResponse(data: any, config: FormatterConfig): LLMResult {
    const content = data.choices[0].message.content;
    
    // 从内容中解析 XML 工具调用
    const toolCalls = ToolCallParser.parseXMLToolCalls(content);
    
    return {
      content,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      // ...
    };
  }
}
```

### 阶段 4: 历史记录转换

#### 4.7 实现历史记录转换

```typescript
// sdk/core/messages/history-converter.ts

export class HistoryConverter {
  /**
   * 将历史记录转换为 XML 模式
   */
  static toXMLMode(messages: LLMMessage[]): LLMMessage[] {
    return messages.map(msg => {
      if (msg.toolCalls) {
        // 将 toolCalls 转为 XML 文本
        return {
          role: 'assistant',
          content: this.toolCallsToXML(msg.toolCalls)
        };
      }
      if (msg.role === 'tool') {
        // 将 tool 结果转为 XML
        return {
          role: 'user',
          content: this.toolResultToXML(msg)
        };
      }
      return msg;
    });
  }
  
  private static toolCallsToXML(toolCalls: LLMToolCall[]): string {
    return toolCalls.map(call => `<tool_use>
  <tool_name>${call.function.name}</tool_name>
  <parameters>
${this.argumentsToXML(call.function.arguments)}
  </parameters>
</tool_use>`).join('\n');
  }
}
```

---

## 五、实施计划

| 阶段 | 任务 | 预计时间 | 依赖 |
|------|------|----------|------|
| 1 | 添加 toolMode 配置到类型定义 | 0.5 天 | 无 |
| 1 | 更新 FormatterConfig | 0.5 天 | 阶段 1 |
| 2 | 实现 ToolDeclarationFormatter | 1 天 | 阶段 1 |
| 2 | 实现 XML 格式生成 | 1 天 | 阶段 2 |
| 2 | 实现 JSON 格式生成 | 0.5 天 | 阶段 2 |
| 3 | OpenAIFormatter XML/JSON 支持 | 1 天 | 阶段 2 |
| 3 | AnthropicFormatter XML/JSON 支持 | 1 天 | 阶段 3 |
| 3 | GeminiFormatter XML/JSON 支持 | 1 天 | 阶段 3 |
| 4 | 历史记录转换功能 | 1 天 | 阶段 3 |
| 4 | 流式响应增强 | 1 天 | 阶段 4 |
| 5 | 单元测试 | 1 天 | 全部 |
| 5 | 集成测试 | 1 天 | 阶段 5 |

**总计：约 10 个工作日**

---

## 六、风险与注意事项

### 6.1 兼容性风险
- 修改 `LLMProfile` 需要检查所有使用位置
- Formatter 基类修改可能影响现有子类

### 6.2 测试覆盖
- 需要为 XML/JSON 模式编写完整测试用例
- 需要验证与现有 function-calling 模式的兼容性

### 6.3 性能考虑
- XML/JSON 模式会增加提示词长度
- 解析 XML/JSON 比原生格式更耗时

---

## 七、总结

### 7.1 当前状态
- SDK 已实现 XML/JSON 格式的**解析**能力 (`ToolCallParser`)
- 缺少 XML/JSON 格式的**生成**能力
- 缺少**历史记录转换**能力
- Formatter 缺少**模式感知**能力

### 7.2 改进必要性
- **功能完整性**：当前只实现了 LimCode 的 1/3 功能
- **模型兼容性**：许多模型不支持原生 function-calling
- **生产验证**：LimCode 的 XML/JSON 模式已被验证有效
- **实现成本**：`ToolCallParser` 已提供基础，只需补充集成

### 7.3 建议优先级
1. **高**：添加 toolMode 配置、工具声明格式生成
2. **中**：Formatter 模式感知、历史记录转换
3. **低**：流式处理增强

---

*分析日期：2026-03-08*
*基于 LimCode 1.0.93 文档*

---

## 八、实施记录

### 2026-03-08 已完成的修改

#### 1. 核心类型定义（已完成）

**修改文件：**
- `packages/types/src/llm/profile.ts` - 添加 `toolMode` 字段到 `LLMProfile`
- `sdk/core/llm/formatters/types.ts` - 添加 `toolMode` 字段到 `FormatterConfig`

**说明：**
- 仅添加类型定义，不涉及任何逻辑修改
- 保持向后兼容，`toolMode` 为可选字段，默认为 `'function_call'`

#### 2. 工具声明格式转换器（已完成）

**新增文件：**
- `packages/common-utils/src/tool/declaration-formatter.ts`

**功能：**
- `ToolDeclarationFormatter.toXML()` - 将工具声明转换为 XML 格式
- `ToolDeclarationFormatter.toJSON()` - 将工具声明转换为 JSON 格式
- `ToolDeclarationFormatter.toolCallToXML()` - 将工具调用转换为 XML 格式
- `ToolDeclarationFormatter.toolCallToJSON()` - 将工具调用转换为 JSON 格式
- 相关的工具结果转换方法

**说明：**
- 纯粹的格式转换工具，不修改 Formatter 逻辑
- 为后续提示词组装提供基础设施

#### 3. 明确未在 Formatter 中实现的内容（架构考虑）

**不在 Formatter 中实现：**
- ❌ 工具声明注入到系统消息
- ❌ 历史记录格式转换在 buildRequest 中进行
- ❌ 在 Formatter 中组装提示词

**原因：**
- 保持 Formatter 职责单一（仅格式转换）
- 提示词组装应在更高层（如 ExecutionCoordinator 或专门的 PromptAssembler）完成
- 避免破坏现有架构的分层设计

### 下一步建议

如需完整支持 XML/JSON 工具调用模式，建议在以下位置实现：

1. **提示词组装层**（推荐）
   - 创建 `ToolPromptAssembler` 类
   - 在调用 Formatter 之前组装包含工具声明的消息

2. **ExecutionCoordinator 层**
   - 在准备 LLM 请求时，根据 `toolMode` 处理消息

3. **保持 Formatter 不变**
   - Formatter 继续只负责 API 格式转换
   - 接收已经组装好的消息
