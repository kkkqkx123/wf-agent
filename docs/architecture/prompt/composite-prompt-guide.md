# 复合提示词目录结构指南

## 概述

复合提示词目录结构允许将复杂的提示词拆分为多个部分，提高可维护性和复用性。每个复合提示词目录包含一个主提示词（`index.toml`）和多个部分提示词（编号的 `.toml` 文件）。

## 目录结构

### 基本结构

```
configs/prompts/
├── system/
│   ├── coder/                    # 复合提示词目录
│   │   ├── index.toml            # 主提示词（通过 system.coder 引用）
│   │   ├── 01_code_style.toml    # 部分提示词（通过 system.coder.code_style 引用）
│   │   └── 02_error_handling.toml
│   └── assistant.toml            # 简单提示词文件
├── rules/
│   └── format.toml
└── templates/
    └── coder.toml                # 使用复合提示词的模板
```

### 文件命名规范

- **主提示词**: `index.toml` - 复合提示词的入口文件
- **部分提示词**: `{序号}_{名称}.toml` - 使用数字前缀确保加载顺序
  - 序号格式: `01`, `02`, `03`, ...
  - 名称使用下划线分隔

## 引用格式

### 基本引用

引用简单提示词文件：
```toml
system_prompt = "system.assistant"
```

对应文件: `configs/prompts/system/assistant.toml`

### 复合提示词引用（单一处理逻辑）

#### 引用主提示词（显式使用 index）

```toml
system_index = "system.coder.index"
```

对应文件: `configs/prompts/system/coder/index.toml`

#### 引用部分提示词（使用序号前缀）

```toml
system_code_style = "system.coder.01_code_style"
system_error_handling = "system.coder.02_error_handling"
```

对应文件:
- `configs/prompts/system/coder/01_code_style.toml`
- `configs/prompts/system/coder/02_error_handling.toml`

**重要说明**：
- 采用单一处理逻辑，不尝试多种可能的路径
- 引用必须明确指定完整的文件名（包括序号前缀）
- 例如：`system.coder.01_code_style` 直接对应 `configs/prompts/system/coder/01_code_style.toml`
- 不支持自动查找 `code_style.toml` 或其他变体
- 这种设计确保执行逻辑可预测，避免歧义

## 使用示例

### 1. 创建复合提示词目录

#### 主提示词 (index.toml)

```toml
# configs/prompts/system/coder/index.toml
name = "coder"
description = "代码生成专家系统提示词"

content = """你是一个代码生成专家，负责生成高质量、可维护的代码。"""

[metadata]
role = "system"
priority = 1
```

#### 部分提示词 (01_code_style.toml)

```toml
# configs/prompts/system/coder/01_code_style.toml
name = "code_style"
description = "代码风格规范"

content = """请遵循以下代码风格：
- 使用PEP8规范
- 添加适当的注释
- 使用有意义的变量名。避免advanced, smart, enhanced等没有意义的命名"""

[metadata]
role = "system"
priority = 2
```

#### 部分提示词 (02_error_handling.toml)

```toml
# configs/prompts/system/coder/02_error_handling.toml
name = "error_handling"
description = "错误处理规范"

content = """代码中必须包含适当的错误处理逻辑：
- 提供有意义的错误信息
- 确保资源正确释放"""

[metadata]
role = "system"
priority = 3
```

### 2. 在模板中使用复合提示词

```toml
# configs/prompts/templates/coder.toml
name = "coder"
description = "代码编写模板"

[template]
# 引用主提示词
system_index = "system.coder"

# 引用部分提示词
system_code_style = "system.coder.code_style"
system_error_handling = "system.coder.error_handling"

# 引用其他提示词
rules = "rules.format"
user_command = "user_commands.code_review"

content = """
{{system_index}}

{{system_code_style}}

{{system_error_handling}}

{{rules}}

{{user_command}}

代码：
{{user_input}}
"""

[variables]
user_input = { required = true, description = "用户输入" }
```

### 3. 渲染结果

使用上述模板渲染后的内容：

```
你是一个代码生成专家，负责生成高质量、可维护的代码。

请遵循以下代码风格：
- 使用PEP8规范
- 添加适当的注释
- 使用有意义的变量名。避免advanced, smart, enhanced等没有意义的命名

代码中必须包含适当的错误处理逻辑：
- 提供有意义的错误信息
- 确保资源正确释放

请遵循以下输出格式规则：
- 使用清晰的段落结构
- 适当使用列表和标题组织内容
- 代码示例使用代码块格式
- 确保回答简洁明了，重点突出

请对提供的代码进行审查，并给出以下内容：
1. 代码质量评估
2. 潜在问题和改进建议
3. 最佳实践建议
4. 安全性检查结果

代码：
function test() { return true; }
```

## 引用解析规则

### PromptReferenceParser 解析逻辑

1. **基本引用** (`category.name`)
   - 优先查找: `configs/prompts/{category}/{name}/index.toml`
   - 备选查找: `configs/prompts/{category}/{name}.toml`

2. **复合引用** (`category.composite.part`)
   - 查找: `configs/prompts/{category}/{composite}/{序号}_{part}.toml`
   - 解析器会自动匹配序号前缀

### 文件名映射

| 引用格式 | 对应文件 |
|---------|---------|
| `system.coder` | `configs/prompts/system/coder/index.toml` |
| `system.coder.code_style` | `configs/prompts/system/coder/01_code_style.toml` |
| `system.coder.error_handling` | `configs/prompts/system/coder/02_error_handling.toml` |

## 最佳实践

### 1. 目录组织

- **按功能分组**: 将相关的提示词部分放在同一个目录中
- **使用序号**: 使用数字前缀确保加载顺序（01, 02, 03...）
- **清晰命名**: 部分提示词的名称应该清晰描述其内容

### 2. 引用命名

- **使用下划线**: 引用名称使用下划线分隔（如 `code_style`）
- **避免特殊字符**: 只使用字母、数字和下划线
- **保持一致性**: 在整个项目中使用一致的命名风格

### 3. 优先级设置

- **主提示词**: 优先级最低（如 1）
- **部分提示词**: 按重要性递增（如 2, 3, 4...）
- **确保顺序**: 通过优先级控制提示词的加载顺序

### 4. 内容组织

- **主提示词**: 定义整体角色和目标
- **部分提示词**: 定义具体的规则和约束
- **避免重复**: 确保各部分内容不重复

## 优势

### 1. 可维护性

- **模块化**: 将复杂提示词拆分为多个部分
- **独立修改**: 可以单独修改某个部分而不影响其他部分
- **清晰结构**: 目录结构直观反映提示词的组织方式

### 2. 可复用性

- **部分复用**: 可以在不同的模板中复用相同的部分提示词
- **灵活组合**: 可以根据需要组合不同的部分

### 3. 可扩展性

- **易于添加**: 可以轻松添加新的部分提示词
- **版本控制**: 每个部分都是独立的文件，便于版本控制

## 注意事项

### 1. 文件名一致性

- 部分提示词的文件名必须与引用名称匹配（去掉序号前缀）
- 例如: `01_code_style.toml` 对应引用 `system.coder.code_style`

### 2. 序号前缀

- 序号前缀是必需的，用于确保加载顺序
- 使用两位数字（01, 02, 03...）以支持超过9个部分

### 3. index.toml

- 每个复合提示词目录必须包含 `index.toml`
- `index.toml` 定义了该复合提示词的主内容

### 4. 引用验证

- 使用 `PromptReferenceValidator` 验证引用格式
- 确保引用的文件存在

## 迁移指南

### 从简单文件迁移到复合目录

1. **创建目录结构**
   ```bash
   mkdir -p configs/prompts/system/coder
   ```

2. **创建 index.toml**
   ```toml
   name = "coder"
   description = "代码生成专家系统提示词"
   content = """你是一个代码生成专家，负责生成高质量、可维护的代码。"""
   ```

3. **拆分内容到部分文件**
   - 将原有内容按功能拆分
   - 创建编号的部分文件（01_xxx.toml, 02_xxx.toml...）

4. **更新引用**
   - 将 `system.coder` 更新为引用主提示词
   - 添加对部分提示词的引用

## 示例场景

### 场景1: 代码审查模板

需要组合多个系统提示词、规则和用户指令：

```toml
[template]
# 主系统提示词
system_coder = "system.coder"

# 部分系统提示词
system_code_style = "system.coder.code_style"
system_error_handling = "system.coder.error_handling"

# 规则
rules_format = "rules.format"
rules_safety = "rules.safety"

# 用户指令
user_command = "user_commands.code_review"

content = """
{{system_coder}}

{{system_code_style}}

{{system_error_handling}}

{{rules_format}}

{{rules_safety}}

{{user_command}}

代码：
{{code}}
"""
```

### 场景2: 数据分析模板

使用不同的复合提示词组合：

```toml
[template]
system_analyzer = "system.analyzer"
system_data_quality = "system.analyzer.data_quality"
system_visualization = "system.analyzer.visualization"

rules_format = "rules.format"

user_command = "user_commands.data_analysis"

content = """
{{system_analyzer}}

{{system_data_quality}}

{{system_visualization}}

{{rules_format}}

{{user_command}}

数据：
{{data}}
"""
```

## 总结

复合提示词目录结构提供了一种灵活、可维护的方式来组织复杂的提示词。通过将提示词拆分为多个部分，可以提高代码的可读性、可维护性和可复用性。

**核心原则**:
- 使用目录结构组织相关提示词
- 使用序号前缀确保加载顺序
- 通过引用格式灵活组合提示词部分
- 保持命名一致性以提高可读性