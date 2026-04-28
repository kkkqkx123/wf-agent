# 项目提示词构建逻辑分析

本文档详细分析 Modular Agent Framework 项目中提示词（Prompt）的构建逻辑和架构设计。

## 一、整体架构概览

项目的提示词系统采用**分层架构**，分为三个主要层次：

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
│  (apps/cli-app, apps/web-app-frontend, etc.)               │
├─────────────────────────────────────────────────────────────┤
│                      SDK Layer                              │
│  ┌─────────────────┐  ┌─────────────────────────────────┐  │
│  │  Core Prompt    │  │  Resources                      │  │
│  │  - Resolver     │  │  - Predefined Prompts           │  │
│  │  - Builder      │  │  - Dynamic Prompts              │  │
│  │  - Registry     │  │  - Template Registry            │  │
│  └─────────────────┘  └─────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│              Packages Layer (prompt-templates)              │
│  - Type Definitions                                         │
│  - Fragment Structures                                      │
│  - Tool Description Templates                               │
│  - Dynamic Context Templates                                │
└─────────────────────────────────────────────────────────────┘
```

## 二、核心组件详解

### 2.1 prompt-templates 包 (packages/prompt-templates)

这是一个**纯静态定义包**，仅包含类型定义和模板常量，无运行时逻辑。

#### 2.1.1 类型定义 (src/types/)

**PromptTemplate 接口** - 提示词模板的核心结构：

```typescript
export interface PromptTemplate {
  id: string;                    // 模板唯一标识
  name: string;                  // 模板名称
  description: string;           // 模板描述
  category: "system" | "rules" | "user-command" | "tools" | "composite" | "fragments" | "dynamic";
  content: string;               // 模板内容，包含 {{variable}} 占位符
  variables?: VariableDefinition[];  // 变量定义
  fragments?: string[];          // 引用的片段ID列表
}
```

**VariableDefinition 接口** - 变量定义：

```typescript
export interface VariableDefinition {
  name: string;                  // 变量名
  type: "string" | "number" | "boolean" | "object" | "array";
  required: boolean;             // 是否必填
  description?: string;          // 变量描述
  defaultValue?: unknown;        // 默认值
}
```

**TemplateComposition 接口** - 模板组合：

```typescript
export interface TemplateComposition {
  baseTemplateId: string;        // 基础模板ID
  overrides: Partial<PromptTemplate>;  // 覆盖字段
  fragmentReplacements?: Record<string, string>;  // 片段替换映射
}
```

#### 2.1.2 片段模板 (src/templates/fragments/)

片段（Fragment）是系统提示词的可复用组成部分，分为四类：

| 类别 | 文件路径 | 用途 |
|------|----------|------|
| Role | `fragments/role/index.ts` | 角色定义（如程序员助手、通用助手） |
| Capability | `fragments/capability/index.ts` | 能力描述 |
| Constraint | `fragments/constraint/index.ts` | 约束条件 |
| Tool Usage | `fragments/tool-usage/index.ts` | 工具使用规范 |

**片段组合器 (composer.ts)**：

```typescript
export interface SystemPromptFragment {
  id: string;
  category: "role" | "capability" | "constraint" | "tool-usage";
  content: string;
  description?: string;
}

export class FragmentRegistry {
  private fragments = new Map<string, SystemPromptFragment>();
  
  register(fragment: SystemPromptFragment): void;
  get(id: string): SystemPromptFragment | undefined;
  getByCategory(category): SystemPromptFragment[];
}

export function composeSystemPrompt(config: FragmentCompositionConfig): string;
```

#### 2.1.3 动态上下文模板 (src/templates/dynamic/)

用于构建动态上下文的模板：

- `context.ts` - 动态上下文前缀和段落包装器
- `environment.ts` - 环境信息模板
- `todo-list.ts` - TODO 列表模板
- `workspace-files.ts` - 工作区文件树模板
- `open-tabs.ts` - 打开的标签页模板
- `active-editor.ts` - 活动编辑器模板
- `diagnostics.ts` - 诊断信息模板
- `pinned-files.ts` - 固定文件模板
- `skills.ts` - 技能模板

#### 2.1.4 工具描述模板 (src/templates/tools/)

- `tool-description.ts` - 工具描述渲染函数
- `formatters/` - XML/JSON 格式模板
- `descriptions/` - 工具描述模板
- `parameters/schema.ts` - 参数模式模板
- `visibility/declaration.ts` - 工具可见性声明模板

### 2.2 SDK 层提示词处理

#### 2.2.1 系统提示词解析器 (sdk/core/prompt/system-prompt-resolver.ts)

**SystemPromptConfig 接口**：

```typescript
export interface SystemPromptConfig {
  systemPrompt?: string;                    // 直接系统提示词字符串
  systemPromptTemplateId?: string;          // 模板ID（优先级更高）
  systemPromptTemplateVariables?: Record<string, unknown>;  // 模板变量
}
```

**解析优先级**：
1. `systemPromptTemplateId` - 使用模板注册表渲染
2. `systemPrompt` - 直接使用字符串
3. 空字符串 - 未配置系统提示词

```typescript
export function resolveSystemPrompt(config: SystemPromptConfig): string;
export function hasSystemPrompt(config: SystemPromptConfig): boolean;
export function buildSystemPromptMessage(config: SystemPromptConfig): { role: "system"; content: string } | null;
```

#### 2.2.2 初始消息构建器 (sdk/core/prompt/build-initial-messages.ts)

**InitialMessagesConfig 接口**：

```typescript
export interface InitialMessagesConfig extends SystemPromptConfig {
  initialUserMessage?: string;                    // 初始用户消息
  initialUserMessageTemplateId?: string;          // 初始消息模板ID
  initialUserMessageTemplateVariables?: Record<string, unknown>;
  existingMessages?: LLMMessage[];                // 已有消息历史
  initialMessages?: LLMMessage[];                 // 完整初始消息（最高优先级）
}
```

**消息构建顺序**：
1. 如果提供了 `initialMessages`，直接使用
2. 添加系统提示词（如果配置）
3. 添加初始用户消息（如果配置）
4. 追加已有消息历史（过滤掉系统消息避免重复）

```typescript
export async function buildInitialMessages(config: InitialMessagesConfig): Promise<LLMMessage[]>;
export function mergeWithHistory(messages: LLMMessage[], history: LLMMessage[]): LLMMessage[];
```

#### 2.2.3 模板注册表 (sdk/resources/predefined/template-registry.ts)

**单例模式的模板注册表**：

```typescript
export class PromptTemplateRegistry {
  private static instance: PromptTemplateRegistry | null = null;
  private templates = new Map<string, PromptTemplate>();
  
  static getInstance(): PromptTemplateRegistry;
  register(template: PromptTemplate): void;
  registerAll(templates: PromptTemplate[]): void;
  get(id: string): PromptTemplate | undefined;
  has(id: string): boolean;
  render(id: string, variables: Record<string, unknown>): string | null;
  renderSafe(id: string, variables: Record<string, unknown>, defaultValue?: string): string;
}

export const templateRegistry = PromptTemplateRegistry.getInstance();
```

#### 2.2.4 预定义提示词 (sdk/resources/predefined/prompts/)

**系统提示词模板** (`system/`):

- `assistant.ts` - 通用助手系统提示词
- `coder.ts` - 程序员助手系统提示词

**片段定义** (`fragments/`):

片段注册表 (`registry.ts`) 管理所有预定义片段：

```typescript
export const ALL_PREDEFINED_FRAGMENTS: SystemPromptFragment[] = [
  // Role fragments
  ASSISTANT_ROLE_FRAGMENT,
  CODER_ROLE_FRAGMENT,
  ANALYST_ROLE_FRAGMENT,
  
  // Capability fragments
  GENERAL_CAPABILITY_FRAGMENT,
  GENERAL_WORK_PRINCIPLES_FRAGMENT,
  CODING_CAPABILITY_FRAGMENT,
  CODING_WORK_PRINCIPLES_FRAGMENT,
  CODING_INTERACTION_FRAGMENT,
  
  // Constraint fragments
  GENERAL_CONSTRAINTS_FRAGMENT,
  GENERAL_INTERACTION_FRAGMENT,
  CODING_CONSTRAINTS_FRAGMENT,
  CODE_SAFETY_FRAGMENT,
  
  // Tool usage fragments
  TOOL_USAGE_XML_SUMMARY_FRAGMENT,
  TOOL_USAGE_JSON_SUMMARY_FRAGMENT,
];

export const fragmentRegistry = new FragmentRegistry();
export function initializeFragmentRegistry(): void;
```

片段组合器 (`composer.ts`)：

```typescript
export interface CompleteSystemPromptConfig {
  fragmentIds: string[];           // 基础片段ID列表
  toolListDescription?: string;    // 动态工具描述
  separator?: string;
}

export function buildCompleteSystemPrompt(config: CompleteSystemPromptConfig): string;

// 预定义的片段组合
export const ASSISTANT_SYSTEM_PROMPT_FRAGMENTS = [
  "sdk.fragments.role.assistant",
  "sdk.fragments.capability.general",
  "sdk.fragments.capability.general-principles",
  "sdk.fragments.constraint.general-interaction",
  "sdk.fragments.constraint.general",
];

export const CODER_SYSTEM_PROMPT_FRAGMENTS = [
  "sdk.fragments.role.coder",
  "sdk.fragments.capability.coding",
  "sdk.fragments.capability.coding-principles",
  "sdk.fragments.capability.coding-interaction",
  "sdk.fragments.constraint.coding",
  "sdk.fragments.constraint.code-safety",
  "sdk.fragments.tool-usage.xml-summary",
];
```

**用户指令模板** (`user-commands/`):

- `code-review.ts` - 代码审查指令
- `data-analysis.ts` - 数据分析指令

#### 2.2.5 动态提示词 (sdk/resources/dynamic/prompts/)

**动态上下文管理** (`context.ts`)：

```typescript
// 全局环境信息存储
let globalEnvironmentInfo: EnvironmentInfo | null = null;

export function setEnvironmentInfo(info: EnvironmentInfo): void;
export function getEnvironmentInfo(): EnvironmentInfo;
export function buildEnvironmentPrompt(): string;

export function buildDynamicContextMessages(
  config: DynamicContextConfig,
  runtime?: DynamicRuntimeContext
): DynamicContextMessage[];
```

**动态上下文组合器** (`fragments/composer.ts`)：

```typescript
export function generateDynamicContextContent(
  config: DynamicContextConfig,
  runtime?: DynamicRuntimeContext
): string;

export function hasDynamicContent(config: DynamicContextConfig): boolean;
```

动态上下文包含：
- 当前时间
- TODO 列表
- 工作区文件树
- 固定文件
- 技能列表

**可用工具片段** (`fragments/available-tools.ts`)：

```typescript
export interface AvailableToolsConfig {
  tools: Tool[];
  format?: ToolDescriptionFormat;  // "list" | "table" | "xml" | "json"
}

export function generateAvailableToolsContent(config: AvailableToolsConfig): string | null;
export function generateToolDescriptionMessage(tools: Tool[]): { role: "system"; content: string } | null;
```

### 2.3 模板渲染器 (packages/common-utils/src/template/template-renderer.ts)

提供模板变量替换功能：

```typescript
export function renderTemplate(
  template: string,
  variables: Record<string, unknown>
): string;

export function getVariableValue(
  variableName: string,
  variables: Record<string, unknown>
): unknown;
```

支持的功能：
- `{{variable}}` 占位符替换
- 嵌套路径解析（如 `user.name`）
- 数组索引访问（如 `items[0].name`）
- 条件渲染 `{{#if variable}}...{{/if}}`
- 循环渲染 `{{#each array}}...{{/each}}`

## 三、提示词构建流程

### 3.1 系统提示词构建流程

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Configuration  │────▶│  Fragment        │────▶│  Complete       │
│  (fragmentIds)  │     │  Registry        │     │  System Prompt  │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌──────────────────┐
                        │  Tool List       │
                        │  Description     │
                        │  (optional)      │
                        └──────────────────┘
```

**代码示例**：

```typescript
// 1. 初始化片段注册表
initializeFragmentRegistry();

// 2. 构建系统提示词内容
const systemPrompt = buildCoderSystemPromptContent(toolListDescription);

// 3. 或使用通用的组合函数
const prompt = buildCompleteSystemPrompt({
  fragmentIds: CODER_SYSTEM_PROMPT_FRAGMENTS,
  toolListDescription: generateToolListDescription(tools, "xml"),
});
```

### 3.2 初始消息构建流程

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Initial Messages Config                          │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                │
│  │ System       │ │ Initial User │ │ Existing     │                │
│  │ Prompt       │ │ Message      │ │ Messages     │                │
│  └──────────────┘ └──────────────┘ └──────────────┘                │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Build Process                               │
│  1. Check initialMessages (highest priority)                        │
│  2. Resolve system prompt (templateId > string)                     │
│  3. Resolve initial user message                                    │
│  4. Filter and append existing messages                             │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      LLMMessage Array                               │
│  [{role: "system", content: ...}, {role: "user", content: ...}, ...] │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.3 动态上下文构建流程

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Dynamic Context Config                           │
│  - includeCurrentTime: true                                         │
│  - includeTodoList: true                                            │
│  - includeWorkspaceFiles: true                                      │
│  - includePinnedFiles: true                                         │
│  - includeSkills: true                                              │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Dynamic Runtime Context                          │
│  - todoList: TodoItem[]                                             │
│  - workspaceFileTree: FileTreeNode[]                                │
│  - pinnedFiles: PinnedFile[]                                        │
│  - skills: Skill[]                                                  │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Generate Content                                 │
│  1. Add prefix explanation                                          │
│  2. Generate current time section                                   │
│  3. Generate TODO list section                                      │
│  4. Generate workspace files section                                │
│  5. Generate pinned files section                                   │
│  6. Generate skills section                                         │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Dynamic Context Message                          │
│  {role: "system", content: "...", metadata: {isDynamicContext: true}}│
└─────────────────────────────────────────────────────────────────────┘
```

## 四、配置覆盖机制

### 4.1 配置目录结构 (configs/prompts/)

```
configs/prompts/
├── README.md
├── rules/
│   ├── format.toml          # 格式规则
│   └── safety.toml          # 安全规则
├── system/
│   ├── assistant.toml       # 通用助手提示词
│   └── coder/
│       ├── index.toml       # 主配置
│       ├── 01_code_style.toml
│       └── 02_error_handling.toml
├── templates/
│   ├── coder.toml           # 代码模板
│   ├── code_fix.toml        # 代码修复模板
│   └── code_review.toml     # 代码审查模板
└── user_commands/
    ├── code_review.toml     # 代码审查指令
    ├── data_analysis.toml   # 数据分析指令
    └── fix_code.toml        # 修复代码指令
```

### 4.2 模板覆盖流程

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Template Loading Process                         │
│                                                                     │
│  1. Load default templates from @wf-agent/prompt-templates       │
│  2. Load configs/prompts/ configurations                            │
│  3. Merge configurations with defaults                              │
│  4. Register merged templates to PromptTemplateRegistry             │
│  5. Initialize fragment registry                                    │
│  6. Update system prompt templates with initialized fragments       │
└─────────────────────────────────────────────────────────────────────┘
```

## 五、关键设计原则

### 5.1 关注点分离

| 层级 | 职责 | 示例 |
|------|------|------|
| prompt-templates | 纯静态定义 | 类型、模板结构、常量 |
| SDK Resources | 业务内容定义 | 片段内容、预定义模板 |
| SDK Core | 运行时逻辑 | 解析、渲染、构建 |
| Application | 配置和调用 | 配置覆盖、业务调用 |

### 5.2 可扩展性设计

1. **片段化设计**：系统提示词由多个片段组合而成，便于复用和定制
2. **模板注册表**：支持动态注册和覆盖模板
3. **配置驱动**：通过配置文件覆盖默认行为
4. **类型安全**：完整的 TypeScript 类型定义

### 5.3 优先级规则

**系统提示词解析优先级**：
1. `systemPromptTemplateId` + `systemPromptTemplateVariables`
2. `systemPrompt` (直接字符串)
3. 空字符串

**初始消息构建优先级**：
1. `initialMessages` (完整消息数组)
2. `systemPrompt` + `initialUserMessage` + `existingMessages`

**模板渲染优先级**：
1. 应用层配置模板
2. SDK 预定义模板
3. prompt-templates 默认模板

## 六、使用示例

### 6.1 构建系统提示词

```typescript
import { initializeFragmentRegistry, buildCoderSystemPromptContent } from "@wf-agent/sdk";
import { generateToolListDescription } from "@wf-agent/sdk/core/utils/tools";

// 初始化
initializeFragmentRegistry();

// 生成工具描述
const toolListDescription = generateToolListDescription(tools, "xml");

// 构建系统提示词
const systemPrompt = buildCoderSystemPromptContent(toolListDescription);
```

### 6.2 构建初始消息

```typescript
import { buildInitialMessages } from "@wf-agent/sdk/core/prompt";

const messages = await buildInitialMessages({
  systemPromptTemplateId: "system.coder",
  initialUserMessage: "Help me refactor this code",
  existingMessages: history,
});
```

### 6.3 构建动态上下文

```typescript
import { buildDynamicContextMessages } from "@wf-agent/sdk/resources/dynamic/prompts";

const dynamicMessages = buildDynamicContextMessages(
  {
    includeCurrentTime: true,
    includeTodoList: true,
    includeWorkspaceFiles: true,
  },
  {
    todoList: [{ id: "1", content: "Refactor auth module", status: "in_progress" }],
    workspaceFileTree: [...],
  }
);
```

## 七、总结

Modular Agent Framework 的提示词系统采用分层、模块化的设计：

1. **prompt-templates 包**提供纯静态的类型定义和模板结构
2. **SDK Resources 层**定义业务相关的片段内容和预定义模板
3. **SDK Core 层**提供运行时解析、渲染和构建逻辑
4. **配置系统**支持应用层覆盖默认行为

这种设计实现了关注点分离、可扩展性和类型安全，同时保持了灵活性和可定制性。
