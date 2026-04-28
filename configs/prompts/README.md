# 提示词配置说明

本目录包含工作流中使用的提示词模板和配置。

## 架构设计

### 目录结构

```
prompts/
├── templates/           # 模板定义文件（TOML格式）
│   ├── system.toml      # 系统提示词模板
│   └── composite.toml   # 复合提示词模板
├── system/             # 具体系统提示词（TOML格式）
│   ├── assistant.toml
│   └── coder.toml
├── rules/              # 具体规则提示词（TOML格式）
│   ├── format.toml
│   └── safety.toml
├── user_commands/      # 具体用户指令（TOML格式）
│   ├── code_review.toml
│   └── data_analysis.toml
└── README.md
```

### 架构理念

新的架构采用**模板编排**模式：
- **模板目录**：定义模板结构和编排规则
- **具体实现目录**：提供实际的提示词内容
- **元数据驱动**：通过元数据标记顺序和层次

## 模板定义格式

### 模板定义文件（templates 目录）

模板文件定义如何组织不同类型的提示词部分：

```toml
# templates/system.toml
name = "system_template"
description = "系统提示词模板"
category = "templates"

[template]
content = """{{system_prompt}}"""

[parts]
system_prompt = { type = "prompt", category = "system", required = true }

[variables]
system_prompt_name = { type = "string", description = "系统提示词名称", required = true }
```

### 复合模板定义

```toml
# templates/composite.toml
name = "composite_template"
description = "复合提示词模板，支持多部分组合"
category = "templates"

[template]
content = """{{system_prompt}}

{{rules}}

{{user_command}}"""

[parts]
system_prompt = { type = "prompt", category = "system", required = true }
rules = { type = "prompt", category = "rules", required = false }
user_command = { type = "prompt", category = "user_commands", required = true }
```

## 具体提示词格式

### 简单提示词文件

```toml
# system/assistant.toml
name = "assistant"
description = "通用助手提示词，定义Agent基础角色"
category = "system"

[content]
text = """你是一个通用助手，负责解答用户问题，语言简洁明了。"""

[metadata]
role = "system"
priority = 1
```

### 复合提示词文件（支持模板编排）

```toml
# system/coder.toml
name = "coder"
description = "代码生成专家系统提示词"
category = "system"

[content]
role_definition = "你是一个代码生成专家，负责生成高质量、可维护的代码。"
code_style = """请遵循以下代码风格：..."""
error_handling = """代码中必须包含适当的错误处理逻辑：..."""

# 完整的提示词模板（通过模板编排）
full_template = """{{role_definition}}

{{code_style}}

{{error_handling}}"""

[metadata]
role = "system"
priority = 1

# 定义模板部分和它们的顺序
[template_parts]
parts = [
    { name = "role_definition", order = 1, description = "基础角色定义" },
    { name = "code_style", order = 2, description = "代码风格规范" },
    { name = "error_handling", order = 3, description = "错误处理规范" }
]

# 模板编排选项
[template_options]
default_template = "full"
variants = [
    { name = "basic", parts = ["role_definition"], description = "基础版本" },
    { name = "with_style", parts = ["role_definition", "code_style"], description = "包含代码风格" },
    { name = "full", parts = ["role_definition", "code_style", "error_handling"], description = "完整版本" }
]
```

## 在 LLM 节点中使用

### 使用简单模板

```typescript
{
  wrapperName: "openai:gpt-4",
  prompt: {
    type: "template",
    category: "system",
    name: "assistant"
  }
}
```

### 使用复合模板（指定变体）

```typescript
{
  wrapperName: "openai:gpt-4",
  prompt: {
    type: "template",
    category: "system",
    name: "coder",
    variant: "with_style"  // 指定使用包含代码风格的变体
  }
}
```

### 使用模板定义

```typescript
{
  wrapperName: "openai:gpt-4",
  prompt: {
    type: "template",
    category: "templates",
    name: "composite",
    variables: {
      system_prompt_name: "assistant",
      rules_names: ["format", "safety"],
      user_command_name: "code_review"
    }
  }
}
```

### 使用直接内容

```typescript
{
  wrapperName: "openai:gpt-4",
  prompt: {
    type: "direct",
    content: "请帮我分析这段代码：{{code}}"
  },
  variables: {
    code: "function example() { return 42; }"
  }
}
```

## 上下文处理器

LLM 节点支持上下文处理器来过滤和转换变量：

```typescript
{
  wrapperName: "openai:gpt-4",
  prompt: {
    type: "template",
    category: "system",
    name: "coder"
  },
  contextProcessor: "llm",  // 使用 llm 上下文处理器
  variables: {
    code: "..."
  }
}
```

内置的上下文处理器：
- `llm` - 过滤 LLM 相关变量（llm.*, prompt.*, model.*）
- `system` - 保留系统级变量（system.*, config.*, env.*）
- `tool` - 过滤工具相关变量
- `human` - 过滤人工交互相关变量
- `pass-through` - 不做任何过滤
- `isolate` - 隔离模式，不传递任何变量

## 变量替换

模板支持 `{{variable}}` 格式的变量替换。变量来源：
1. 节点配置中的 `variables` 字段
2. 工作流执行上下文中的变量
3. 上下文处理器过滤后的变量

## 架构优势

1. **模板编排**：支持灵活的模板组合和变体选择
2. **元数据驱动**：通过元数据定义顺序和层次结构
3. **单一文件管理**：复杂提示词可以合并到单一文件中
4. **类型安全**：完整的 TOML 格式支持结构化配置
5. **向后兼容**：支持现有的直接内容和简单模板

## 最佳实践

1. **使用复合模板**：对于复杂的提示词，使用复合模板编排
2. **定义模板变体**：为不同场景提供多个模板版本
3. **使用元数据**：明确定义模板部分的顺序和描述
4. **分类管理**：按功能分类组织模板
5. **版本控制**：使用 created_at 和 updated_at 跟踪变更