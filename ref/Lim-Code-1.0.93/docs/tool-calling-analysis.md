# LimCode 工具调用处理分析

本文档详细分析了 LimCode backend 目录中工具调用的三种处理方式：XML、JSON 和 Function-calling。

## 目录结构

```
backend/
├── tools/
│   ├── types.ts              # 工具类型定义
│   ├── xmlFormatter.ts       # XML 格式转换器
│   ├── jsonFormatter.ts      # JSON 格式转换器
│   ├── ToolRegistry.ts       # 工具注册器
│   ├── taskManager.ts        # 任务管理器
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

---

## 一、XML 工具调用处理

### 1.1 核心文件
- `backend/tools/xmlFormatter.ts`

### 1.2 格式定义

**工具声明转换为 XML：**
```xml
<tool name="tool_name">
  <description>
    工具描述
  </description>
  <parameters>
    - param1 (required) [type]: 参数描述
    - param2 (optional) [type]: 参数描述
  </parameters>
</tool>
```

**工具调用格式：**
```xml
<tool_use>
  <tool_name>tool name here</tool_name>
  <parameters>
    <parameter_name>value</parameter_name>
    <array_param>
      <item>value1</item>
      <item>value2</item>
    </array_param>
    <object_param>
      <property1>value1</property1>
      <property2>value2</property2>
    </object_param>
  </parameters>
</tool_use>
```

**工具响应格式：**
```xml
<tool_result tool="tool_name">
{
  "success": true,
  "data": "..."
}
</tool_result>
```

### 1.3 解析流程

1. **解析工具调用** (`parseXMLToolCalls`)
   - 使用正则表达式匹配 `<tool_use>...</tool_use>` 块
   - 使用 `fast-xml-parser` 库解析 XML 结构
   - 递归处理参数值（支持数组和嵌套对象）
   - 返回结构化的工具调用信息 `{ name, args }`

2. **处理数组参数** (`processParameterValue`)
   - 识别 `item` 属性作为数组标记
   - 递归处理嵌套结构
   - 支持复杂嵌套对象

### 1.4 与 API 的集成

**Gemini 集成：**
- 在 `convertHistoryToXMLMode` 中将 functionCall/functionResponse 转换为 XML 文本
- 将工具声明转换为 XML 格式追加到系统提示词

**OpenAI/Anthropic 集成：**
- 在 XML 模式下，将工具调用转换为 XML 文本放入用户消息
- 支持从响应内容中解析 XML 工具调用

---

## 二、JSON 工具调用处理

### 2.1 核心文件
- `backend/tools/jsonFormatter.ts`

### 2.2 格式定义

**边界标记：**
```
<<<TOOL_CALL>>>  - 开始标记
<<<END_TOOL_CALL>>> - 结束标记
```

**工具调用格式：**
```json
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

### 2.3 设计特点

1. **动态边界标记**
   - 使用 `<<<TOOL_CALL>>>` 和 `<<<END_TOOL_CALL>>>` 作为边界
   - 避免内容中的代码块干扰解析
   - 类似于 heredoc 的语法

2. **流式解析支持**
   - `hasJSONToolCallStart`: 检查是否包含工具调用开始标记
   - `hasCompleteJSONBlock`: 检查工具调用块是否完整
   - `extractIncompleteToolCall`: 提取未完成的工具调用内容

### 2.4 解析流程

1. **提取工具调用** (`parseJSONToolCalls`)
   - 使用正则表达式匹配边界标记之间的内容
   - 解析 JSON 内容
   - 验证格式（必须包含 `tool` 字段）

2. **错误处理**
   - JSON 解析失败时跳过该块
   - 记录警告日志

### 2.5 与 API 的集成

与 XML 模式类似，但在 `convertHistoryToJSONMode` 中使用 JSON 格式转换。

---

## 三、Function-calling 处理

### 3.1 核心文件
- `backend/modules/api/chat/services/ToolCallParserService.ts`
- `backend/modules/channel/formatters/*.ts`

### 3.2 格式定义

**统一的 Function Call 格式（内部使用）：**
```typescript
interface FunctionCallInfo {
  name: string;           // 函数名称
  args: Record<string, unknown>;  // 函数参数
  id: string;             // 调用 ID (格式: fc_{timestamp}_{random})
}
```

**ContentPart 中的 functionCall：**
```typescript
part.functionCall = {
  name: "tool_name",
  args: { /* 参数 */ },
  id: "fc_1704628800000_a1b2c3d"
}
```

### 3.3 各 API 的 Function-calling 支持

#### 3.3.1 Gemini (Google)

**请求格式：**
```json
{
  "tools": [{
    "function_declarations": [
      {
        "name": "tool_name",
        "description": "...",
        "parameters": { /* JSON Schema */ }
      }
    ]
  }]
}
```

**响应格式：**
```json
{
  "candidates": [{
    "content": {
      "parts": [{
        "functionCall": {
          "name": "tool_name",
          "args": { /* 参数 */ }
        }
      }]
    }
  }]
}
```

**特点：**
- 原生支持 functionCall 和 functionResponse
- 支持多模态数据在 functionResponse.parts 中返回

#### 3.3.2 OpenAI

**请求格式：**
```json
{
  "tools": [{
    "type": "function",
    "function": {
      "name": "tool_name",
      "description": "...",
      "parameters": { /* JSON Schema */ }
    }
  }]
}
```

**响应格式：**
```json
{
  "choices": [{
    "message": {
      "tool_calls": [{
        "id": "call_xxx",
        "type": "function",
        "function": {
          "name": "tool_name",
          "arguments": "{...}"
        }
      }]
    }
  }]
}
```

**特点：**
- 使用 `tool_calls` 数组
- arguments 是 JSON 字符串，需要解析
- function_response 使用 `role: "tool"`

#### 3.3.3 Anthropic

**请求格式：**
```json
{
  "tools": [{
    "name": "tool_name",
    "description": "...",
    "input_schema": { /* JSON Schema */ }
  }]
}
```

**响应格式：**
```json
{
  "content": [{
    "type": "tool_use",
    "id": "toolu_xxx",
    "name": "tool_name",
    "input": { /* 参数 */ }
  }]
}
```

**特点：**
- 使用 `tool_use` 类型
- 支持 thinking 和 redacted_thinking
- function_response 使用 `type: "tool_result"`

### 3.4 格式转换流程

```
┌─────────────────┐
│  外部 API 响应   │
│ (Gemini/OpenAI/ │
│  Anthropic)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Formatter.parse │
│    Response()   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  统一的 Content  │
│  格式 (内部使用) │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ToolCallParser │
│ extractFunction │
│     Calls()     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ FunctionCallInfo│
│    数组         │
└─────────────────┘
```

---

## 四、工具模式选择

### 4.1 模式定义

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| `function_call` | 使用 API 原生的函数调用功能 | 支持 function-calling 的模型 |
| `xml` | 使用 XML 格式在提示词中描述工具 | 不原生支持 function-calling 的模型 |
| `json` | 使用 JSON 格式在提示词中描述工具 | 不原生支持 function-calling 的模型 |

### 4.2 模式切换逻辑

在 `ChannelManager.generate()` 中：
1. 从配置中读取 `toolMode` 字段
2. 根据 `toolMode` 决定如何处理工具声明和历史记录
3. 不同的 formatters 根据 `toolMode` 执行不同的转换逻辑

### 4.3 多模态支持差异

| 渠道 | function_call 模式 | xml/json 模式 |
|------|-------------------|---------------|
| Gemini | 支持图片和文档 | 支持图片和文档 |
| OpenAI | 不支持多模态工具 | 支持图片，不支持文档 |
| Anthropic | 支持图片和文档 | 支持图片和文档 |

---

## 五、工具执行流程

### 5.1 执行服务

**核心文件：** `ToolExecutionService.ts`

**主要方法：**
- `executeFunctionCalls()`: 执行函数调用列表
- `executeFunctionCallsWithResults()`: 执行并返回完整结果
- `executeFunctionCallsWithProgress()`: 带进度事件的执行（流式）

### 5.2 执行流程

```
1. 接收 FunctionCallInfo[] 数组
2. 创建检查点（before）
3. 遍历执行每个工具调用：
   a. 检查是否需要用户确认
   b. 检查是否是 MCP 工具
   c. 执行内置工具或 MCP 工具
   d. 处理多模态数据
   e. 构建 functionResponse
4. 创建检查点（after）
5. 返回响应 parts 和工具结果
```

### 5.3 工具注册与查找

**注册：** `ToolRegistry.ts`
- 使用 Map 存储工具注册信息
- 支持依赖检查（dependencies）
- 支持按名称过滤

**执行上下文：**
```typescript
interface ToolContext {
  config?: Record<string, unknown>;     // 工具配置
  multimodalEnabled?: boolean;          // 是否启用多模态
  capability?: MultimodalCapability;    // 多模态能力
  abortSignal?: AbortSignal;            // 取消信号
  toolId?: string;                      // 工具调用 ID
  toolOptions?: ToolOptions;            // 工具选项
  conversationId?: string;              // 对话 ID
  conversationStore?: ConversationStore; // 对话存储
}
```

---

## 六、流式处理

### 6.1 流式响应处理

**核心文件：** `StreamResponseProcessor.ts`

**处理流程：**
1. 接收原始流式块
2. 使用 Formatter 解析块
3. 累加到 Content
4. 发送增量数据到前端

### 6.2 流式工具调用

**OpenAI 流式特点：**
- tool_calls 分段返回（index, id, name, arguments）
- 需要使用累加器合并部分参数
- 在 `StreamAccumulator.ts` 中处理

**Anthropic 流式特点：**
- 使用 `input_json_delta` 返回工具参数增量
- 需要累加 partial_json

---

## 七、关键设计要点

### 7.1 统一内部格式

所有外部 API 的响应都转换为统一的内部 `Content` 格式：
- `role`: 'user' | 'model'
- `parts`: ContentPart[]
- `parts` 可以包含：text, functionCall, functionResponse, inlineData, fileData

### 7.2 工具调用 ID 管理

- 使用 `generateToolCallId()` 生成唯一 ID
- 格式：`fc_{timestamp}_{random}`
- 用于匹配 functionCall 和 functionResponse
- Gemini 不返回 ID，需要自行生成

### 7.3 历史记录转换

**Function Call 模式 → 文本模式：**
- `convertHistoryToXMLMode()`: 将 functionCall 转换为 XML 文本
- `convertHistoryToJSONMode()`: 将 functionCall 转换为 JSON 文本
- 保持历史记录格式与当前工具模式一致

### 7.4 多模态数据处理

**处理策略：**
1. function_call 模式：多模态数据放入 `functionResponse.parts`
2. xml/json 模式：多模态数据作为独立 parts 添加到用户消息

---

## 八、总结

LimCode 的工具调用系统支持三种模式：

1. **Function-calling 模式**：利用 API 原生功能，最可靠，支持流式
2. **XML 模式**：通过提示词工程实现，兼容性好，适合不支持 function-calling 的模型
3. **JSON 模式**：类似 XML 模式，但使用 JSON 格式，某些模型可能更擅长解析

三种模式通过统一的内部格式（Content/ContentPart）进行转换，确保了不同 API 和不同模式之间的兼容性。

工具执行通过 `ToolExecutionService` 统一管理，支持：
- 内置工具执行
- MCP 工具执行
- 多模态数据处理
- 用户确认机制
- 检查点管理
