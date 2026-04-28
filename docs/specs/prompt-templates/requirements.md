# 提示词模板包需求规格

## 介绍

本需求规格定义了 `packages/prompt-templates` 包的设计与实现要求。该包是 Modular Agent Framework 的一部分，旨在为 SDK 层和应用层提供统一的提示词模板定义。包的核心定位是**纯静态定义包**，仅包含类型定义和模板内容常量，所有处理逻辑、状态管理和配置加载都由 SDK 和应用层负责。

## 需求列表

### 1. 纯静态模板包

**用户故事**：作为 SDK 开发者，我希望 packages/prompt-templates 是一个纯静态包，不包含任何运行时逻辑，以便保持包的轻量化和可移植性。

**验收标准**：
1.1 当检查包的依赖时，包不应依赖任何运行时库（仅依赖 TypeScript 内置类型）。
1.2 当导入包时，包不应导出任何函数、类或状态管理器。
1.3 当构建包时，包应仅包含 `.ts` 类型定义文件和常量导出。
1.4 当包被使用时，不应产生任何副作用（如注册表初始化、配置加载）。

### 2. 类型定义导出

**用户故事**：作为应用开发者，我希望从 packages/prompt-templates 导入完整的类型定义，以便在代码中获得类型安全提示和自动补全。

**验收标准**：
2.1 当导入 `PromptTemplate` 类型时，应获得包含 `id`、`name`、`description`、`category`、`content`、`variables`、`fragments` 字段的接口定义。
2.2 当导入 `VariableDefinition` 类型时，应获得包含 `name`、`type`、`required`、`description`、`defaultValue` 字段的接口定义。
2.3 当导入 `PromptFragment` 类型时，应获得包含 `id`、`content`、`variables` 字段的接口定义。
2.4 当导入 `TemplateFillRule` 类型时，应获得包含 `templateId`、`variableMapping`、`fragmentMapping` 字段的接口定义。
2.5 当使用类型时，应能通过 TypeScript 类型检查，并支持泛型扩展。

### 3. 模板内容常量

**用户故事**：作为 SDK 开发者，我希望从 packages/prompt-templates 导入预定义的模板内容常量，以便在 SDK 内部直接使用这些模板构建消息。

**验收标准**：
3.1 当导入系统提示词模板时，应获得 `CODER_SYSTEM_TEMPLATE`、`ASSISTANT_SYSTEM_TEMPLATE` 等常量。
3.2 当导入规则模板时，应获得 `FORMAT_RULE_TEMPLATE`、`SAFETY_RULE_TEMPLATE` 等常量。
3.3 当导入用户指令模板时，应获得 `CODE_REVIEW_TEMPLATE`、`DATA_ANALYSIS_TEMPLATE` 等常量。
3.4 当导入工具相关模板时，应获得 `TOOL_VISIBILITY_DECLARATION_TEMPLATE`、`TOOL_DESCRIPTION_TABLE_TEMPLATE`、`TOOL_PARAMETERS_SCHEMA_TEMPLATE` 等常量。
3.5 每个模板常量必须符合 `PromptTemplate` 类型定义。

### 4. 工具相关模板

**用户故事**：作为 SDK 开发者，我希望获得专门用于工具可见性声明、工具描述和工具参数描述的模板，以便在工具服务、执行协调器等组件中统一生成提示词。

**验收标准**：
4.1 当需要构建工具可见性声明消息时，应能导入 `TOOL_VISIBILITY_DECLARATION_TEMPLATE` 模板，该模板包含 `timestamp`、`scope`、`scopeId`、`changeTypeText`、`toolDescriptions` 变量。
4.2 当需要生成工具描述表格行时，应能导入 `TOOL_TABLE_ROW_TEMPLATE` 字符串模板。
4.3 当需要映射变更类型文本时，应能导入 `VISIBILITY_CHANGE_TYPE_TEXTS` 常量映射（`init`、`enter_scope`、`add_tools`、`exit_scope`、`refresh`）。
4.4 当需要生成工具参数 Schema 描述时，应能导入 `TOOL_PARAMETERS_SCHEMA_TEMPLATE` 模板，该模板包含 `toolName`、`toolId`、`toolDescription`、`parametersSchema`、`parametersDescription` 变量。
4.5 当需要生成工具描述（单行、列表、表格格式）时，应能导入 `TOOL_DESCRIPTION_SINGLE_LINE_TEMPLATE`、`TOOL_DESCRIPTION_LIST_TEMPLATE`、`TOOL_DESCRIPTION_TABLE_TEMPLATE`。

### 5. 配置覆盖机制

**用户故事**：作为应用开发者，我希望能够通过配置文件覆盖 packages/prompt-templates 中的默认模板，以便根据应用需求定制提示词内容。

**验收标准**：
5.1 当在 `configs/prompts/` 目录下提供同名模板的配置文件（如 `system/coder.toml`）时，SDK 应能加载该配置并合并到默认模板。
5.2 当配置文件中定义了 `content` 字段时，应覆盖默认模板的 `content` 字段。
5.3 当配置文件中定义了额外的 `variables` 或 `fragments` 时，应合并到默认模板的对应字段。
5.4 当配置文件不存在时，SDK 应回退到使用 packages/prompt-templates 中的默认模板。
5.5 配置文件的格式支持 TOML 或 JSON，由 SDK 的配置加载器决定。

### 6. SDK 集成

**用户故事**：作为 SDK 开发者，我希望 SDK 内部提供模板渲染函数、工具描述生成函数等实用工具，以便使用 packages/prompt-templates 中的模板生成最终提示词。

**验收标准**：
6.1 当 SDK 需要渲染模板时，应调用 `renderTemplate(template: string, variables: Record<string, any>): string` 函数，该函数能够替换模板中的 `{{variable}}` 占位符。
6.2 当 SDK 需要生成工具描述时，应调用 `generateToolDescription(tool: Tool, format: 'table' | 'single-line' | 'list'): string` 函数，该函数使用 packages/prompt-templates 中的对应模板。
6.3 当 SDK 需要生成工具参数描述时，应调用 `generateToolParametersDescription(tool: Tool): string` 函数，该函数使用 `TOOL_PARAMETERS_SCHEMA_TEMPLATE` 模板。
6.4 当 SDK 需要构建工具可见性声明消息时，应改造现有的 `ToolVisibilityMessageBuilder` 类，使其使用 packages/prompt-templates 中的模板。
6.5 所有 SDK 集成代码应位于 `sdk/core/utils/` 和 `sdk/core/execution/utils/` 目录下，不修改 packages/prompt-templates 包。

### 7. 应用层定制

**用户故事**：作为应用开发者，我希望能够直接导入 packages/prompt-templates 中的定义，并创建自定义模板或扩展类型，以便满足特定业务场景。

**验收标准**：
7.1 当应用层需要创建自定义模板时，应能导入 `PromptTemplate` 类型并创建新的模板对象。
7.2 当应用层需要扩展模板变量类型时，应能使用 TypeScript 的泛型或交叉类型进行扩展。
7.3 当应用层需要覆盖默认模板时，应能通过配置方式（如 `configs/prompts/`）或直接编程方式（如创建新对象并传递给 SDK）实现。
7.4 当应用层需要组合多个模板时，应能引用 packages/prompt-templates 中的片段（fragments）进行组合。
7.5 应用层定制不应破坏 packages/prompt-templates 包的不可变性原则。

### 8. 目录结构与导出

**用户故事**：作为包维护者，我希望 packages/prompt-templates 具有清晰的目录结构和统一的导出入口，以便开发者快速找到所需定义。

**验收标准**：
8.1 当查看包目录时，应看到 `src/types/`、`src/templates/`、`src/fragments/`、`src/constants/` 等目录。
8.2 当导入包时，应能通过 `import { PromptTemplate } from '@modular-agent/prompt-templates'` 导入类型。
8.3 当导入包时，应能通过 `import { CODER_SYSTEM_TEMPLATE } from '@modular-agent/prompt-templates'` 导入模板常量。
8.4 当导入包时，应能通过 `import { TEMPLATE_IDS } from '@modular-agent/prompt-templates'` 导入 ID 常量。
8.5 包的 `package.json` 应正确配置 `exports` 字段，支持按需导入。
