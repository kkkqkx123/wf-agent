# 最终完整版手册：MCP 工具 / 普通固定工具 / 多 LLM 兼容 最佳实践

## 一、核心概念区分（一句话记住）
| 类型 | 定义 | 用途 |
|------|------|------|
| **普通固定工具** | 本地定义、Schema 固定、参数确定 | 内置函数、天气、计算器、数据库 |
| **MCP 动态工具** | 远程动态发现、参数不固定、透传调用 | Model Context Protocol 远程服务 |
| **LLM 兼容清理** | 按 OpenAI / Anthropic / Gemini 自动裁剪 Schema | 跨平台统一调用 |

---

# 二、最关键规则：`additionalProperties` 终极指南
## 1. 普通固定工具（必须）
```json
"additionalProperties": false
```
- 严格校验
- 禁止多余字段
- 与 OpenAI `strict: true` 配套
- RuntimeValidator 必须启用 `.strict()`

## 2. MCP 动态工具（必须）
```json
"additionalProperties": true
```
- 允许任意字段
- 本地不做参数校验
- 透传给 MCP Server 校验
- **不能加 strict: true**

## 3. 多 LLM 平台自动处理规则
| 平台 | additionalProperties | 处理 |
|------|----------------------|------|
| **OpenAI** | 保留 | 支持 true / false |
| **Anthropic** | 保留 | 支持 true / false |
| **Gemini** | **删除** | 不支持，必须清理 |

---

# 三、`strict: true` 终极规则（OpenAI 专属）
## 1. 普通固定工具 = 可以开
```json
"strict": true,
"additionalProperties": false
```
作用：
- 强制模型输出严格匹配 Schema
- 自动解析 parsed_arguments
- 不会出现多余字段、格式错误

## 2. MCP 工具 = **绝对不能开**
```json
"strict": false （或不写）
```
- MCP 是动态透传
- strict 会破坏参数传递
- 与 MCP 设计冲突

---

# 四、普通工具 vs MCP 工具 完整差异表
| 项目 | 普通固定工具 | MCP 动态工具 |
|------|--------------|-------------|
| additionalProperties | false | true |
| strict | true | false / 不设置 |
| 参数来源 | 本地预定义 Schema | 远程 MCP Server |
| 本地校验 | 全开（递归 + strict） | 关闭 / 仅基础校验 |
| 多余字段 | 拦截报错 | 允许透传 |
| 嵌套结构 | 完整递归校验 | 不校验 / 透传 |
| 适用 LLM | 所有平台 | 所有平台（Gemini 需清理） |
| 格式化 | 标准函数工具 | 标准函数工具（true） |

---

# 五、多 LLM 提供商 Schema 清理规则（你已实现）
## 1. OpenAI
保留：
- additionalProperties
- examples
- default
- pattern

## 2. Anthropic
保留：
- additionalProperties
清理：
- patternProperties
- allOf / oneOf / anyOf

## 3. Gemini
**删除：**
- additionalProperties
- default
- examples
- allOf / oneOf / anyOf
- if / then / else

---

# 六、RuntimeValidator 运行时校验最佳实践
## 1. 普通工具
```ts
z.object(shape).strict()  // 禁止多余字段
递归校验所有嵌套结构
完整校验：type、enum、format、min/max、regex
```

## 2. MCP 工具
```ts
z.object(shape).strip()  // 允许多余字段
不做严格校验
只做基础类型校验（可选）
```

## 3. 你的最终实现（正确）
```ts
return z.object(shape).strict();
```
→ 普通工具用
→ MCP 工具在调用前**关闭校验**即可

---

# 七、工具定义格式最佳实践（可直接复制）
## 1. 普通工具（标准）
```json
{
  "name": "get_weather",
  "description": "获取天气",
  "parameters": {
    "type": "object",
    "properties": {
      "city": { "type": "string", "description": "城市" }
    },
    "required": ["city"],
    "additionalProperties": false
  },
  "strict": true
}
```

## 2. MCP 工具（标准）
```json
{
  "name": "mcp_tool",
  "description": "MCP 动态工具",
  "parameters": {
    "type": "object",
    "properties": {},
    "required": [],
    "additionalProperties": true
  },
  "strict": false
}
```

---

# 八、你整个架构的最佳实践（最终版）
## 1. StaticValidator（注册时）
- 检查工具结构合法性
- **不强制 additionalProperties**
- **不强制 strict**
- 支持 boolean | object

## 2. ToolSchemaCleaner（发送前）
- OpenAI：保留 additionalProperties
- Anthropic：保留 additionalProperties
- Gemini：删除 additionalProperties

## 3. RuntimeValidator（执行前）
- 普通工具：严格递归校验 + strict()
- MCP 工具：关闭严格校验、允许任意字段

## 4. Formatter（发送格式）
- OpenAI 格式：标准 function tool
- Anthropic 格式：input_schema 直接透传
- Gemini 格式：清理后的精简 Schema

---

# 九、最容易踩的 5 个坑（避坑指南）
1. **MCP 工具开 strict: true → 直接失效**
2. **普通工具 additionalProperties: true → 不稳定**
3. **给 Gemini 传 additionalProperties → 报错**
4. **运行时校验没开递归 → 嵌套参数无法校验**
5. **Anthropic input_schema 重复包裹 type:object → 报错**

---

# 十、最终一句话总结
## 普通工具
**strict: true + additionalProperties: false + 严格校验**

## MCP 工具
**strict: false + additionalProperties: true + 不校验多余字段**

## 多平台
**OpenAI 保留、Anthropic 保留、Gemini 删除**
