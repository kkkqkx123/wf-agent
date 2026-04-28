# 提示词模板包实现任务

## 概述

本任务列表将设计文档转化为具体的编码步骤，供代码生成 LLM 执行。每个任务都专注于编写、修改或测试代码，并引用需求文档中的具体需求。任务按逻辑顺序排列，确保每个步骤都建立在前一步的基础上，最终将所有组件集成在一起。

## 任务列表

### 1. 创建 packages/prompt-templates 包基础结构

- [ ] **1.1 初始化包目录和配置文件**
  - 在 `packages/prompt-templates/` 目录下创建包基础结构
  - 创建 `package.json`，设置正确的名称、版本和导出配置
  - 创建 `tsconfig.json`，配置 TypeScript 编译选项
  - 创建 `vitest.config.mjs`，配置测试环境
  - **引用需求**：8.1 目录结构与导出，9.5 类型定义文件

- [ ] **1.2 创建类型定义文件**
  - 创建 `src/types/template.ts`，定义 `PromptTemplate`、`VariableDefinition`、`TemplateFillRule` 接口
  - 创建 `src/types/fragment.ts`，定义 `PromptFragment` 接口
  - 创建 `src/types/composition.ts`，定义 `TemplateComposition` 接口
  - 创建 `src/types/index.ts`，统一导出所有类型
  - **引用需求**：2.1-2.5 类型定义导出，10.1 类型验证

- [ ] **1.3 创建常量定义文件**
  - 创建 `src/constants/template-ids.ts`，定义 `TEMPLATE_IDS` 常量
  - 创建 `src/constants/variable-names.ts`，定义 `VARIABLE_NAMES` 常量
  - 创建 `src/constants/index.ts`，统一导出所有常量
  - **引用需求**：8.3-8.4 ID 常量导出，10.2 常量验证

### 2. 实现工具相关模板

- [ ] **2.1 创建工具可见性声明模板**
  - 创建 `src/templates/tools/visibility/declaration.ts`，定义 `TOOL_VISIBILITY_DECLARATION_TEMPLATE`
  - 包含 `TOOL_TABLE_ROW_TEMPLATE` 字符串常量和 `VISIBILITY_CHANGE_TYPE_TEXTS` 映射
  - 确保模板符合 `PromptTemplate` 类型，包含所有必需变量
  - **引用需求**：4.1-4.3 工具可见性声明模板，3.4 工具相关模板

- [ ] **2.2 创建工具描述模板**
  - 创建 `src/templates/tools/descriptions/table-format.ts`，定义 `TOOL_DESCRIPTION_TABLE_TEMPLATE`
  - 创建 `src/templates/tools/descriptions/single-line.ts`，定义 `TOOL_DESCRIPTION_SINGLE_LINE_TEMPLATE`
  - 创建 `src/templates/tools/descriptions/list-format.ts`，定义 `TOOL_DESCRIPTION_LIST_TEMPLATE`
  - **引用需求**：4.5 工具描述模板，3.4 工具相关模板

- [ ] **2.3 创建工具参数 Schema 模板**
  - 创建 `src/templates/tools/parameters/schema.ts`，定义 `TOOL_PARAMETERS_SCHEMA_TEMPLATE`
  - 包含 `PARAMETER_DESCRIPTION_LINE_TEMPLATE` 字符串常量
  - **引用需求**：4.4 工具参数 Schema 描述模板，3.4 工具相关模板

- [ ] **2.4 创建片段定义**
  - 创建 `src/fragments/tool-visibility.ts`，定义 `TOOL_VISIBILITY_FRAGMENT`
  - 创建 `src/fragments/tool-descriptions.ts`，定义 `TOOL_DESCRIPTION_FRAGMENT`
  - 创建 `src/fragments/index.ts`，统一导出所有片段
  - **引用需求**：3.5 片段定义，10.3 片段引用测试

### 3. 实现系统、规则和用户指令模板

- [ ] **3.1 创建系统提示词模板**
  - 创建 `src/templates/system/coder.ts`，定义 `CODER_SYSTEM_TEMPLATE`
  - 创建 `src/templates/system/assistant.ts`，定义 `ASSISTANT_SYSTEM_TEMPLATE`
  - 创建 `src/templates/system/index.ts`，统一导出系统模板
  - **引用需求**：3.1 系统提示词模板，2.1 类型定义

- [ ] **3.2 创建规则模板**
  - 创建 `src/templates/rules/format.ts`，定义 `FORMAT_RULE_TEMPLATE`
  - 创建 `src/templates/rules/safety.ts`，定义 `SAFETY_RULE_TEMPLATE`
  - 创建 `src/templates/rules/index.ts`，统一导出规则模板
  - **引用需求**：3.2 规则模板，2.1 类型定义

- [ ] **3.3 创建用户指令模板**
  - 创建 `src/templates/user-commands/code-review.ts`，定义 `CODE_REVIEW_TEMPLATE`
  - 创建 `src/templates/user-commands/data-analysis.ts`，定义 `DATA_ANALYSIS_TEMPLATE`
  - 创建 `src/templates/user-commands/index.ts`，统一导出用户指令模板
  - **引用需求**：3.3 用户指令模板，2.1 类型定义

### 4. 创建统一导出入口

- [ ] **4.1 创建主导出文件**
  - 创建 `src/index.ts`，统一导出所有类型、模板、片段和常量
  - 配置 `package.json` 的 `exports` 字段，支持按需导入
  - 确保导出结构清晰，支持 `import { PromptTemplate } from '@modular-agent/prompt-templates'`
  - **引用需求**：8.2-8.5 统一导出入口，1.2 纯静态包

- [ ] **4.2 构建和类型生成**
  - 配置构建脚本，生成 `dist` 目录和 `.d.ts` 声明文件
  - 确保构建产物不包含任何运行时逻辑，仅为类型和常量
  - **引用需求**：1.1-1.4 纯静态包，9.5 类型定义文件

### 5. 实现 SDK 模板渲染器

- [ ] **5.1 创建模板渲染函数**
  - 在 `sdk/core/utils/template-renderer.ts` 中实现 `renderTemplate` 函数
  - 支持 `{{variable}}` 占位符替换，包含嵌套路径解析
  - 实现 `getVariableValue` 辅助函数，使用 `@modular-agent/common-utils` 的 `resolvePath`
  - **引用需求**：6.1 模板渲染函数，5.5 变量替换语法

- [ ] **5.2 创建工具描述生成函数**
  - 在 `sdk/core/utils/tool-description-generator.ts` 中实现 `generateToolDescription` 函数
  - 支持 'table'、'single-line'、'list' 三种格式
  - 实现 `generateToolListDescription` 函数，用于生成工具列表描述
  - **引用需求**：6.2 工具描述生成函数，4.5 工具描述模板

- [ ] **5.3 创建工具参数描述生成函数**
  - 在 `sdk/core/utils/tool-parameters-describer.ts` 中实现 `generateToolParametersDescription` 函数
  - 使用 `TOOL_PARAMETERS_SCHEMA_TEMPLATE` 模板生成参数描述
  - 解析工具参数的 `properties` 和 `required` 字段，生成参数说明列表
  - **引用需求**：6.3 工具参数描述生成函数，4.4 工具参数 Schema 描述

### 6. 实现 SDK 配置加载器

- [ ] **6.1 创建配置加载函数**
  - 在 `sdk/api/config/prompt-template-loader.ts` 中实现 `loadPromptTemplateConfig` 函数
  - 支持 TOML 和 JSON 格式的配置文件解析
  - 实现配置合并逻辑，优先使用应用层配置，回退到默认模板
  - **引用需求**：5.1-5.5 配置覆盖机制，6.4 配置加载

- [ ] **6.2 创建��置验证函数**
  - 在 `sdk/api/config/prompt-template-validator.ts` 中实现 `validatePromptTemplate` 函数
  - 验证配置是否符合 `PromptTemplate` 类型定义
  - 提供详细的错误信息，便于调试
  - **引用需求**：5.4 配置验证，10.1 类型验证

### 7. 改造现有 SDK 组件使用模板

- [ ] **7.1 改造工具可见性消息构建器**
  - 修改 `sdk/core/execution/utils/tool-visibility-message-builder.ts` 中的 `buildVisibilityDeclarationMessage` 方法
  - 导入 `@modular-agent/prompt-templates` 中的模板常量
  - 使用 `renderTemplate` 函数和 `TOOL_VISIBILITY_DECLARATION_TEMPLATE` 生成消息
  - 保持原有接口不变，确保向后兼容
  - **引用需求**：6.4 工具可见性消息构建器改造，4.1 工具可见性声明模板

- [ ] **7.2 改造工具可见性消息服务**
  - 修改 `sdk/core/execution/services/tool-visibility-message-service.ts` 中的 `buildVisibilityDeclarationMessage` 方法
  - 使用模板渲染器替代硬编码消息
  - 确保测试用例继续通过
  - **引用需求**：6.4 SDK 集成，4.1 工具可见性声明模板

- [ ] **7.3 更新消息构建器中的工具描述生成**
  - 修改 `sdk/core/messages/message-builder.ts` 中的 `buildToolDescriptionMessage` 方法
  - 使用 `generateToolListDescription` 函数生成工具描述文本
  - 确保空描述处理逻辑不变
  - **引用需求**：6.2 工具描述生成函数，3.5 工具描述模板
