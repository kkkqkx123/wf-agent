我理解您的指正了。让我重新设计一个符合要求的方案：**packages/prompt-templates 应当是一个纯静态定义包**，只包含类型和模板内容，所有处理逻辑、状态管理和配置加载都由 SDK 和应用层负责。

## 重新设计方案

### 一、packages/prompt-templates 定位

**核心原则**：
- ✅ **纯静态包**：仅导出类型定义和模板内容常量
- ✅ **无状态**：不包含任何运行时状态、注册表或管理器
- ✅ **无逻辑**：不包含渲染引擎、加载器、变量解析等处理逻辑
- ✅ **可导入**：SDK 和应用层通过 `import` 直接获取定义
- ✅ **可覆盖**：应用层可导入后覆盖类型或内容

### 二、目录结构设计

```
packages/prompt-templates/
├── package.json
├── tsconfig.json
├── src/
│   ├── index.ts                      # 统一导出
│   ├── types/                        # 类型定义层（纯类型）
│   │   ├── template.ts               # 模板结构类型
│   │   ├── fragment.ts               # 片段结构类型
│   │   ├── variable.ts               # 变量定义类型
│   │   ├── composition.ts            # 组合规则类型
│   │   └── index.ts
│   ├── templates/                    # 模板内容定义（纯常量）
│   │   ├── system/                   # 系统提示词模板
│   │   │   ├── assistant.ts          # 助手系统提示词
│   │   │   ├── coder.ts              # 程序员系统提示词
│   │   │   └── index.ts
│   │   ├── rules/                    # 规则模板
│   │   │   ├── format.ts             # 格式规则
│   │   │   ├── safety.ts             # 安全规则
│   │   │   └── index.ts
│   │   ├── user-commands/            # 用户指令模板
│   │   │   ├── code-review.ts        # 代码审查指令
│   │   │   └── data-analysis.ts      # 数据分析指令
│   │   └── index.ts
│   ├── fragments/                    # 可复用片段定义
│   │   ├── tool-descriptions.ts      # 工具描述片段
│   │   ├── visibility-messages.ts    # 可见性声明片段
│   │   └── index.ts
│   └── constants/                    # 常量定义
│       ├── template-ids.ts           # 模板ID常量
│       ├── variable-names.ts         # 变量名称常量
│       └── index.ts
```

### 三、文件职责与内容示例

#### 1. `src/types/template.ts` - 模板类型定义

```typescript
// 纯类型定义，无逻辑
export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  category: 'system' | 'rules' | 'user-command' | 'composite';
  content: string;  // 模板内容，含 {{variable}} 占位符
  variables?: VariableDefinition[];  // 所需变量定义
  fragments?: string[];  // 引用的片段ID
}

export interface VariableDefinition {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'object';
  required: boolean;
  description?: string;
  defaultValue?: any;
}

// 填充规则类型 - 定义如何填充模板
export interface TemplateFillRule {
  templateId: string;
  variableMapping: Record<string, string>;  // 变量名 -> 上下文路径映射
  fragmentMapping?: Record<string, string>; // 片段ID -> 实际内容映射
}
```

#### 2. `src/templates/system/coder.ts` - 模板内容定义

```typescript
// 纯常量定义，无逻辑
export const CODER_SYSTEM_TEMPLATE: PromptTemplate = {
  id: 'system.coder',
  name: 'Coder System Prompt',
  description: '程序员助手系统提示词',
  category: 'system',
  content: `{{system.coder.index}}

{{system.coder.code_style}}

{{system.coder.error_handling}}

{{rules.format}}

代码：
{{user_input}}`,
  variables: [
    { name: 'user_input', type: 'string', required: true, description: '用户输入的代码需求' }
  ],
  fragments: [
    'system.coder.index',
    'system.coder.code_style',
    'system.coder.error_handling',
    'rules.format'
  ]
};

// 导出所有系统模板
export const SYSTEM_TEMPLATES = {
  CODER_SYSTEM_TEMPLATE,
  ASSISTANT_SYSTEM_TEMPLATE,
  // ...
};
```

#### 3. `src/fragments/tool-descriptions.ts` - 片段定义

```typescript
// 工具描述片段 - 用于动态生成工具可见性声明
export const TOOL_VISIBILITY_FRAGMENT = {
  id: 'tool.visibility.declaration',
  content: `当前可用工具范围已更新为: {{scopeName}}
可用工具列表:
{{toolList}}`,
  variables: [
    { name: 'scopeName', type: 'string', required: true },
    { name: 'toolList', type: 'string', required: true }
  ]
};

// 导出所有片段
export const FRAGMENTS = {
  TOOL_VISIBILITY_FRAGMENT,
  // ...
};
```

#### 4. `src/constants/template-ids.ts` - ID 常量

```typescript
// 模板ID常量，避免硬编码字符串
export const TEMPLATE_IDS = {
  SYSTEM: {
    CODER: 'system.coder',
    ASSISTANT: 'system.assistant',
  },
  RULES: {
    FORMAT: 'rules.format',
    SAFETY: 'rules.safety',
  },
  USER_COMMANDS: {
    CODE_REVIEW: 'user_commands.code_review',
    DATA_ANALYSIS: 'user_commands.data_analysis',
  },
  FRAGMENTS: {
    TOOL_VISIBILITY: 'fragment.tool_visibility',
  }
} as const;
```

### 四、使用方式与集成

#### 1. **SDK 层使用（直接导入）**

在 `sdk/core/execution/coordinators/tool-visibility-coordinator.ts` 中：

```typescript
// 直接导入片段定义
import { TOOL_VISIBILITY_FRAGMENT } from '@modular-agent/prompt-templates';
import { TEMPLATE_IDS } from '@modular-agent/prompt-templates';

// 使用片段内容构建消息
const fragmentContent = TOOL_VISIBILITY_FRAGMENT.content;
const variables = {
  scopeName: newScope,
  toolList: availableTools.map(t => `- ${t.name}: ${t.description}`).join('\n')
};

// 调用 SDK 内部的渲染函数（在 sdk/core 中实现）
const renderedContent = renderTemplate(fragmentContent, variables);

const message: LLMMessage = {
  role: 'system',
  content: renderedContent,
  metadata: {
    templateId: TEMPLATE_IDS.FRAGMENTS.TOOL_VISIBILITY,
    scope: newScope,
    tools: availableTools
  }
};
```

#### 2. **应用层覆盖（类型或内容）**

在 `apps/web-app/src/custom-prompts.ts` 中：

```typescript
// 导入原始定义
import { CODER_SYSTEM_TEMPLATE } from '@modular-agent/prompt-templates';
import type { PromptTemplate } from '@modular-agent/prompt-templates';

// 覆盖内容
export const CUSTOM_CODER_TEMPLATE: PromptTemplate = {
  ...CODER_SYSTEM_TEMPLATE,
  content: `{{system.coder.index}}

{{system.coder.code_style}}

// 添加自定义规则
{{custom_rules.strict_types}}

{{rules.format}}

代码：
{{user_input}}`,
  fragments: [
    ...CODER_SYSTEM_TEMPLATE.fragments,
    'custom_rules.strict_types'  // 添加自定义片段
  ]
};
```

#### 3. **配置加载（由 SDK 负责）**

在 `sdk/api/config/parsers.ts` 中：

```typescript
import { PromptTemplate } from '@modular-agent/prompt-templates';
import * as fs from 'fs/promises';

// 从配置文件加载模板（应用层提供的配置）
export async function loadPromptTemplateFromConfig(
  configPath: string
): Promise<PromptTemplate> {
  const content = await fs.readFile(configPath, 'utf-8');
  const config = parseToml(content);  // 或其他格式解析
  
  // 验证并转换为 PromptTemplate 类型
  return validatePromptTemplate(config);
}
```

### 五、与现有配置的协作

**configs/prompts/** 目录的角色：

该目录作为**应用层配置入口**，与 `packages/prompt-templates` 包协作：

1. **默认模板**：`packages/prompt-templates` 提供默认模板定义
2. **配置覆盖**：应用层在 `configs/prompts/` 中提供配置文件，覆盖默认模板
3. **动态加载**：SDK 的 `sdk/api/config` 模块负责读取配置，合并到默认模板
4. **优先级**：配置 > 包内默认定义

**示例协作流程**：

```typescript
// 1. SDK 导入默认模板
import { CODER_SYSTEM_TEMPLATE } from '@modular-agent/prompt-templates';

// 2. SDK 加载应用层配置
const configOverride = await loadConfig('./configs/prompts/system/coder.toml');

// 3. SDK 合并配置
const finalTemplate = {
  ...CODER_SYSTEM_TEMPLATE,
  ...configOverride,
  // 深度合并 content 等字段
};

// 4. SDK 使用最终模板
const rendered = renderTemplate(finalTemplate.content, variables);
```

### 六、关键设计原则

1. **零运行时依赖**：`packages/prompt-templates` 不依赖任何其他包（仅依赖 TypeScript 内置类型）

2. **纯静态导出**：所有导出均为常量或类型，无函数、无类、无状态

3. **不可变性**：模板定义一旦导出，不可被修改。应用层覆盖需创建新对象

4. **自文档化**：通过 TypeScript 类型系统，IDE 能提供完整的类型提示和自动补全

5. **版本化管理**：模板定义跟随包版本发布，应用层可通过版本控制管理模板变更

### 七、实现步骤

1. **创建 packages/prompt-templates 包**
   - 初始化 package.json，设置正确的导出路径
   - 创建类型定义文件
   - 迁移现有模板内容到该包

2. **SDK 改造**
   - 在 `sdk/core` 中实现模板渲染函数（纯函数，无状态）
   - 在 `sdk/api/config` 中实现配置加载和合并逻辑
   - 修改各 handler 使用新的模板导入方式

3. **应用层适配**
   - 更新 apps 层代码，导入新的模板包
   - 提供配置覆盖示例
   - 测试覆盖场景
