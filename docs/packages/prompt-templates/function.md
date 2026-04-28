## SDK 层提示词模板使用场景分析及功能设计

基于对 SDK 代码的深入分析，我梳理了 SDK 层实际使用提示词模板的地方，并设计了相应的功能。

### 一、SDK 层提示词模板使用场景

#### 1. **工具可见性声明消息**
**位置**：`sdk/core/execution/utils/tool-visibility-message-builder.ts`

**当前实现**：
```typescript
buildVisibilityDeclarationMessage(
  scope: ToolScope,
  scopeId: string,
  toolIds: string[],
  changeType: VisibilityChangeType
): string {
  const toolDescriptions = toolIds
    .map(id => {
      const tool = this.toolService.getTool(id);
      if (!tool) return null;
      return `| ${tool.name} | ${id} | ${tool.description} |`;
    })
    .filter(Boolean)
    .join('\n');

  return `## 工具可见性声明
**生效时间**：${timestamp}
**当前作用域**：${scope}(${scopeId})
**变更类型**：${changeTypeText}

### 当前可用工具清单
| 工具名称 | 工具ID | 说明 |
|----------|--------|------|
${toolDescriptions || '无可用工具'}

### 重要提示
1. **仅可使用上述清单中的工具**，其他工具调用将被拒绝
2. 工具参数必须符合schema定义
3. 如需更多工具，请完成当前任务后退出当前作用域`;
}
```

**模板需求**：
- 工具可见性声明模板（包含 Markdown 表格格式）
- 变更类型文本映射（init/enter_scope/add_tools/exit_scope/refresh）
- 工具描述表格行模板

---

#### 2. **工具描述消息**
**位置**：`sdk/core/messages/message-builder.ts`

**当前实现**：
```typescript
static buildToolDescriptionMessage(descriptionText: string): LLMMessage | null {
  if (!descriptionText) return null;
  return {
    role: 'system' as 'system',
    content: descriptionText
  };
}
```

**模板需求**：
- 工具描述模板（用于动态生成工具描述文本）

---

#### 3. **LLM 执行协调器中的工具参数描述**
**位置**：`sdk/core/execution/coordinators/llm-execution-coordinator.ts`

**当前实现**：
```typescript
// 在工具调用时包含工具描述和参数
{
  id: tool.id,
  description: tool.description,
  parameters: tool.parameters
}
```

**模板需求**：
- 工具参数描述模板（用于生成工具参数的文本描述）
- 工具调用结果模板（用于格式化工具调用结果）

---

#### 4. **工具服务中的工具查询**
**位置**：`sdk/core/services/tool-service.ts`

**当前实现**：
```typescript
searchTools(query: string): Tool[] {
  const lowerQuery = query.toLowerCase();
  return this.tools.filter(tool =>
    tool.id.toLowerCase().includes(lowerQuery) ||
    tool.description.toLowerCase().includes(lowerQuery) ||
    tool.metadata?.tags?.some(tag => tag.toLowerCase().includes(lowerQuery))
  );
}
```

**模板需求**：
- 工具搜索结果描述模板

---

#### 5. **对话管理器中的工具描述**
**位置**：`sdk/core/execution/managers/conversation-manager.ts`

**当前实现**：
```typescript
const toolDescriptions = toolIds
  .map(id => {
    const tool = this.toolService.getTool(id);
    if (!tool) return null;
    return `- ${tool.name}: ${tool.description}`;
  })
  .filter(Boolean)
  .join('\n');
```

**模板需求**：
- 工具列表描述模板（用于生成工具列表的文本描述）

---

### 二、packages/prompt-templates 结构设计

基于上述场景，设计如下结构：

```
packages/prompt-templates/
├── src/
│   ├── index.ts                      # 统一导出
│   ├── types/                        # 类型定义
│   │   ├── index.ts
│   │   ├── template.ts               # 模板结构类型
│   │   ├── fragment.ts               # 片段结构类型
│   │   ├── variable.ts               # 变量定义类型
│   │   └── tool-prompts.ts           # 工具相关提示词类型
│   ├── templates/                    # 模板内容
│   │   ├── system/                   # 系统提示词
│   │   │   ├── assistant.ts
│   │   │   └── coder.ts
│   │   ├── rules/                    # 规则提示词
│   │   │   ├── format.ts
│   │   │   └── safety.ts
│   │   ├── user-commands/            # 用户指令
│   │   │   └── code-review.ts
│   │   └── tools/                    # 工具相关模板【新增】
│   │       ├── visibility/           # 可见性声明
│   │       │   ├── declaration.ts    # 主声明模板
│   │       │   ├── table-row.ts      # 表格行模板
│   │       │   └── change-type.ts    # 变更类型文本
│   │       ├── descriptions/         # 工具描述
│   │       │   ├── single-line.ts    # 单行描述
│   │       │   ├── table-format.ts   # 表格格式
│   │       │   └── list-format.ts    # 列表格式
│   │       └── parameters/           # 参数描述
│   │           ├── schema.ts         # Schema 描述模板
│   │           └── validation.ts     # 验证规则描述
│   ├── fragments/                    # 可复用片段
│   │   ├── tool-visibility.ts        # 工具可见性片段
│   │   └── tool-descriptions.ts      # 工具描述片段
│   └── constants/                    # 常量
│       ├── template-ids.ts           # 模板ID常量
│       └── variable-names.ts         # 变量名称常量
```

### 三、核心模板定义示例

#### 1. 工具可见性声明模板

**文件**：`src/templates/tools/visibility/declaration.ts`

```typescript
import type { PromptTemplate } from '../../types/template';

export const TOOL_VISIBILITY_DECLARATION_TEMPLATE: PromptTemplate = {
  id: 'tools.visibility.declaration',
  name: 'Tool Visibility Declaration',
  description: '工具可见性声明主模板',
  category: 'system',
  content: `## 工具可见性声明

**生效时间**: {{timestamp}}
**当前作用域**: {{scope}}({{scopeId}})
**变更类型**: {{changeTypeText}}

### 当前可用工具清单

| 工具名称 | 工具ID | 说明 |
|----------|--------|------|
{{toolDescriptions}}

### 重要提示

1. **仅可使用上述清单中的工具**，其他工具调用将被拒绝
2. 工具参数必须符合schema定义
3. 如需更多工具，请完成当前任务后退出当前作用域`,
  variables: [
    { name: 'timestamp', type: 'string', required: true, description: '生效时间' },
    { name: 'scope', type: 'string', required: true, description: '作用域类型' },
    { name: 'scopeId', type: 'string', required: true, description: '作用域ID' },
    { name: 'changeTypeText', type: 'string', required: true, description: '变更类型文本' },
    { name: 'toolDescriptions', type: 'string', required: true, description: '工具描述表格行' }
  ]
};

// 表格行模板
export const TOOL_TABLE_ROW_TEMPLATE = '| {{toolName}} | {{toolId}} | {{toolDescription}} |';

// 变更类型文本映射
export const VISIBILITY_CHANGE_TYPE_TEXTS = {
  init: '初始化',
  enter_scope: '进入作用域',
  add_tools: '新增工具',
  exit_scope: '退出作用域',
  refresh: '刷新声明'
} as const;
```

#### 2. 工具描述模板

**文件**：`src/templates/tools/descriptions/table-format.ts`

```typescript
import type { PromptTemplate } from '../../../types/template';

export const TOOL_DESCRIPTION_TABLE_TEMPLATE: PromptTemplate = {
  id: 'tools.description.table',
  name: 'Tool Description Table Format',
  description: '工具描述表格格式模板',
  category: 'system',
  content: '| {{toolName}} | {{toolId}} | {{toolDescription}} |',
  variables: [
    { name: 'toolName', type: 'string', required: true },
    { name: 'toolId', type: 'string', required: true },
    { name: 'toolDescription', type: 'string', required: true }
  ]
};

// 单行描述模板
export const TOOL_DESCRIPTION_SINGLE_LINE_TEMPLATE = '{{toolName}}: {{toolDescription}}';

// 列表格式模板
export const TOOL_DESCRIPTION_LIST_TEMPLATE = '- {{toolName}}: {{toolDescription}}';
```

#### 3. 工具参数 Schema 描述模板

**文件**：`src/templates/tools/parameters/schema.ts`

```typescript
import type { PromptTemplate } from '../../../types/template';

export const TOOL_PARAMETERS_SCHEMA_TEMPLATE: PromptTemplate = {
  id: 'tools.parameters.schema',
  name: 'Tool Parameters Schema Description',
  description: '工具参数Schema描述模板',
  category: 'system',
  content: `工具名称: {{toolName}}
工具ID: {{toolId}}
工具描述: {{toolDescription}}

参数Schema:
\`\`\`json
{{parametersSchema}}
\`\`\`

参数说明:
{{parametersDescription}}`,
  variables: [
    { name: 'toolName', type: 'string', required: true },
    { name: 'toolId', type: 'string', required: true },
    { name: 'toolDescription', type: 'string', required: true },
    { name: 'parametersSchema', type: 'string', required: true },
    { name: 'parametersDescription', type: 'string', required: false }
  ]
};

// 参数描述行模板
export const PARAMETER_DESCRIPTION_LINE_TEMPLATE = '- {{paramName}} ({{paramType}}): {{paramDescription}} {{required}}';
```

#### 4. 工具可见性片段

**文件**：`src/fragments/tool-visibility.ts`

```typescript
import type { PromptFragment } from '../types/fragment';

export const TOOL_VISIBILITY_FRAGMENT: PromptFragment = {
  id: 'fragment.tool_visibility',
  content: `当前可用工具范围已更新为: {{scopeName}}
可用工具列表:
{{toolList}}`,
  variables: [
    { name: 'scopeName', type: 'string', required: true },
    { name: 'toolList', type: 'string', required: true }
  ]
};
```

### 四、SDK 层集成方式

#### 1. 工具可见性消息构建器改造

**文件**：`sdk/core/execution/utils/tool-visibility-message-builder.ts`

```typescript
import { 
  TOOL_VISIBILITY_DECLARATION_TEMPLATE,
  TOOL_TABLE_ROW_TEMPLATE,
  VISIBILITY_CHANGE_TYPE_TEXTS
} from '@modular-agent/prompt-templates';
import { renderTemplate } from '../../utils/template-renderer';  // SDK 内部实现

export class ToolVisibilityMessageBuilder {
  constructor(private toolService: ToolService) { }

  buildVisibilityDeclarationMessage(
    scope: ToolScope,
    scopeId: string,
    toolIds: string[],
    changeType: VisibilityChangeType
  ): string {
    // 构建工具描述表格行
    const toolDescriptions = toolIds
      .map(id => {
        const tool = this.toolService.getTool(id);
        if (!tool) return null;
        
        // 使用模板渲染每一行
        return renderTemplate(TOOL_TABLE_ROW_TEMPLATE, {
          toolName: tool.name,
          toolId: id,
          toolDescription: tool.description
        });
      })
      .filter(Boolean)
      .join('\n');

    // 使用主模板渲染完整消息
    return renderTemplate(TOOL_VISIBILITY_DECLARATION_TEMPLATE.content, {
      timestamp: new Date().toISOString(),
      scope,
      scopeId,
      changeTypeText: VISIBILITY_CHANGE_TYPE_TEXTS[changeType],
      toolDescriptions: toolDescriptions || '无可用工具'
    });
  }
}
```

#### 2. 工具描述生成函数

**文件**：`sdk/core/utils/tool-description-generator.ts`（新建）

```typescript
import {
  TOOL_DESCRIPTION_TABLE_TEMPLATE,
  TOOL_DESCRIPTION_SINGLE_LINE_TEMPLATE,
  TOOL_DESCRIPTION_LIST_TEMPLATE
} from '@modular-agent/prompt-templates';
import { renderTemplate } from './template-renderer';

export function generateToolDescription(
  tool: Tool,
  format: 'table' | 'single-line' | 'list' = 'table'
): string {
  const variables = {
    toolName: tool.name,
    toolId: tool.id,
    toolDescription: tool.description
  };

  switch (format) {
    case 'table':
      return renderTemplate(TOOL_DESCRIPTION_TABLE_TEMPLATE.content, variables);
    case 'single-line':
      return renderTemplate(TOOL_DESCRIPTION_SINGLE_LINE_TEMPLATE, variables);
    case 'list':
      return renderTemplate(TOOL_DESCRIPTION_LIST_TEMPLATE, variables);
    default:
      return renderTemplate(TOOL_DESCRIPTION_TABLE_TEMPLATE.content, variables);
  }
}

export function generateToolListDescription(tools: Tool[]): string {
  return tools
    .map(tool => generateToolDescription(tool, 'list'))
    .join('\n');
}
```

#### 3. 工具参数 Schema 描述生成

**文件**：`sdk/core/utils/tool-parameters-describer.ts`（新建）

```typescript
import {
  TOOL_PARAMETERS_SCHEMA_TEMPLATE,
  PARAMETER_DESCRIPTION_LINE_TEMPLATE
} from '@modular-agent/prompt-templates';
import { renderTemplate } from './template-renderer';

export function generateToolParametersDescription(tool: Tool): string {
  const parametersSchema = JSON.stringify(tool.parameters, null, 2);
  
  const parametersDescription = Object.entries(tool.parameters?.properties || {})
    .map(([paramName, paramConfig]) => {
      return renderTemplate(PARAMETER_DESCRIPTION_LINE_TEMPLATE, {
        paramName,
        paramType: paramConfig.type,
        paramDescription: paramConfig.description || '无描述',
        required: tool.parameters?.required?.includes(paramName) ? '(必填)' : '(可选)'
      });
    })
    .join('\n');

  return renderTemplate(TOOL_PARAMETERS_SCHEMA_TEMPLATE.content, {
    toolName: tool.name,
    toolId: tool.id,
    toolDescription: tool.description,
    parametersSchema,
    parametersDescription: parametersDescription || '无参数'
  });
}
```

### 五、配置与覆盖机制

#### 1. 默认模板（packages 包内）

```typescript
// packages/prompt-templates/src/templates/tools/visibility/declaration.ts
export const TOOL_VISIBILITY_DECLARATION_TEMPLATE = {
  id: 'tools.visibility.declaration',
  content: '...默认内容...'
};
```

#### 2. 应用层配置覆盖（configs 目录）

**文件**：`configs/prompts/tools/visibility/declaration.toml`

```toml
# 应用层覆盖默认模板
id = "tools.visibility.declaration"
name = "Custom Tool Visibility Declaration"

content = """
## 自定义工具可见性声明

**生效时间**: {{timestamp}}
**当前作用域**: {{scope}}({{scopeId}})
**变更类型**: {{changeTypeText}}

### 可用工具

{{toolDescriptions}}

### 注意事项
- 只能使用上述工具
- 参数必须符合 schema
- 需要更多工具请完成任务后退出
"""
```

#### 3. SDK 加载与合并逻辑

**文件**：`sdk/api/config/prompt-template-loader.ts`（新建）

```typescript
import { PromptTemplate } from '@modular-agent/prompt-templates';
import * as fs from 'fs/promises';
import { parse } from 'toml';

export async function loadPromptTemplateConfig(
  configPath: string,
  defaultTemplate: PromptTemplate
): Promise<PromptTemplate> {
  try {
    const content = await fs.readFile(configPath, 'utf-8');
    const config = parse(content);
    
    // 合并配置和默认模板
    return {
      ...defaultTemplate,
      ...config,
      // 深度合并其他字段
    };
  } catch (error) {
    // 配置文件不存在，返回默认模板
    return defaultTemplate;
  }
}
```

### 六、模板渲染函数（SDK 内部实现）

**文件**：`sdk/core/utils/template-renderer.ts`（新建）

```typescript
import { resolvePath } from '@modular-agent/common-utils';

/**
 * 渲染模板字符串
 * @param template 模板字符串，含 {{variable}} 占位符
 * @param variables 变量上下文
 * @returns 渲染后的字符串
 */
export function renderTemplate(
  template: string,
  variables: Record<string, any>
): string {
  // 匹配 {{variable}} 格式的模板变量
  const regex = /\{\{([^}]+)\}\}/g;
  const matches = template.matchAll(regex);
  
  let result = template;
  for (const match of matches) {
    if (match[1]) {
      const variablePath = match[1].trim();
      const value = getVariableValue(variablePath, variables);
      result = result.replace(match[0], String(value ?? ''));
    }
  }
  
  return result;
}

function getVariableValue(path: string, variables: Record<string, any>): any {
  return resolvePath(path, variables);
}
```

### 七、总结

**packages/prompt-templates 包内容**：
- ✅ 纯类型定义（Template、Fragment、Variable 等）
- ✅ 纯模板内容常量（系统提示词、规则、用户指令、工具相关）
- ✅ 纯常量（模板ID、变量名称、变更类型文本等）
- ❌ 无渲染逻辑、无状态管理、无配置加载

**SDK 层实现**：
- ✅ 模板渲染函数（`renderTemplate`）
- ✅ 工具描述生成函数（`generateToolDescription`）
- ✅ 参数 Schema 描述生成（`generateToolParametersDescription`）
- ✅ 配置加载和合并逻辑（`loadPromptTemplateConfig`）
- ✅ 改造现有 `ToolVisibilityMessageBuilder` 使用模板

**应用层能力**：
- ✅ 直接导入默认模板使用
- ✅ 通过 `configs/prompts/` 目录覆盖模板内容
- ✅ 覆盖类型定义扩展功能

这个设计完全符合您的要求：**packages 作为纯静态定义包，SDK 负责所有处理逻辑，应用层可通过配置和导入灵活定制**。