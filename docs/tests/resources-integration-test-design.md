# Resources Module Integration Test Design

## Overview

本文档描述 `packages/sdk/resources/` 模块的集成测试设计方案。Resources 模块是 SDK 中所有预定义、自定义和动态资源的统一管理和注册入口，分为四大子模块：

| 模块 | 定位 | 说明 |
|------|------|------|
| **Predefined** | 静态内置资源 | 工具、触发器、工作流、提示词模板、Agent 模板、Starter |
| **Custom** | 用户配置资源 | 从 JSON 配置文件加载的工具、触发器、提示词 |
| **Dynamic** | 运行时上下文 | 系统上下文（时间、环境、工具列表）和用户上下文（TODO、文件） |
| **Registration** | 统一注册编排 | 三管道注册编排器，协调所有资源的注册顺序 |

## 架构要点

### 三管道注册流程

```
registerAllResources()
  ├── Pipeline 1: Predefined Resources
  │   ├── 1.1 Fragments → Prompt Templates（依赖：fragments）
  │   ├── 1.2 Tool Descriptions
  │   ├── 1.3 Workflows → Triggers → Tools（依赖：workflow registry 先于 trigger）
  │   └── 1.4 Starters（可选，依赖：所有 registry 已就绪）
  │
  ├── Pipeline 2: Custom Resources（工具、触发器、提示词）
  │
  ├── Pipeline 3: Application Resources（预留）
  │
  └── Pipeline 4: Starter Activation（可选）
```

### 关键设计特征

- **skipIfExists**：每个注册函数都支持跳过已存在的资源，保证幂等性
- **Partial success**：允许部分成功，不要求全有或全无
- **allowList/blockList**：每个资源类别支持白名单/黑名单控制
- **Configurable**：通过 `PresetsConfig` 控制启用/禁用及自定义配置
- **No rollback**：各子注册独立，无强一致性约束

## 测试文件组织结构

根据项目约定，集成测试位于 `packages/sdk/__tests__/integration/resources/` 目录下：

```
packages/sdk/__tests__/integration/resources/
├── __shared/
│   ├── fixtures.ts                 # 共享测试夹具
│   └── mock-registries.ts          # Mock registry 实现
├── predefined-tools-registration.int.test.ts
├── predefined-triggers-registration.int.test.ts
├── predefined-workflows-registration.int.test.ts
├── predefined-prompts-registration.int.test.ts
├── predefined-tool-descriptions-registration.int.test.ts
├── predefined-builtin-tools.int.test.ts
├── predefined-risk-classification.int.test.ts
├── predefined-starters.int.test.ts
├── custom-resources-loading.int.test.ts
├── custom-resources-registration.int.test.ts
├── dynamic-system-context.int.test.ts
├── dynamic-user-context.int.test.ts
├── registration-orchestrator.int.test.ts
└── registration-end-to-end.int.test.ts
```

## 测试场景设计

### 1. Predefined Tools Registration

文件：`predefined-tools-registration.int.test.ts`

**测试目标**：验证 `registerPredefinedTools()` 和 `unregisterPredefinedTools()` 的完整行为。

| 测试场景 | 验证点 |
|----------|--------|
| **注册所有预定义工具** | 成功注册 10 个工具（read_file, write_file, edit_file, run_shell, record_note, recall_notes, list_categories, backend_shell, shell_output, shell_kill） |
| **skipIfExists=true 时跳过已注册工具** | 先注册全部，再重复注册，不报错且不重复注册 |
| **skipIfExists=false 时重复注册报错** | 先注册全部，再重复注册，预期失败列表包含所有工具 ID |
| **allowList 白名单** | 只注册 allowList 中的工具，其余不注册 |
| **blockList 黑名单** | 注册全部工具，但 blockList 中的工具跳过 |
| **allowList + blockList 互斥** | allowList 优先级高于 blockList |
| **带配置注册** | 传入 readFile/runShell 等配置，验证工具创建时使用配置 |
| **取消注册所有工具** | 成功取消注册所有预定义工具 |
| **取消注册指定工具** | 只取消注册指定的工具 ID |
| **取消注册不存在的工具** | 不报错，成功列表为空 |
| **isPredefinedToolRegistered** | 注册前返回 false，注册后返回 true，取消后返回 false |
| **空工具列表** | 所有工具已注册时再注册，结果为 0 成功 |

### 2. Predefined Triggers Registration

文件：`predefined-triggers-registration.int.test.ts`

**测试目标**：验证 `registerPredefinedTriggers()` 和 `unregisterPredefinedTriggers()`。

| 测试场景 | 验证点 |
|----------|--------|
| **注册默认 context_compression_trigger** | 成功注册，验证模板字段（name, condition, action, enabled, maxTriggers） |
| **注册自定义配置的 trigger** | 传入 timeout、maxTriggers、compressionPrompt，验证模板字段被覆盖 |
| **skipIfExists 行为** | 同 tools 测试 |
| **allowList/blockList** | 白名单/黑名单过滤 |
| **取消注册** | 成功取消注册，registry 中不再存在 |
| **取消注册不存在的 trigger** | 不会报错 |
| **isPredefinedTriggerRegistered** | 验证注册状态查询 |

### 3. Predefined Workflows Registration

文件：`predefined-workflows-registration.int.test.ts`

**测试目标**：验证 `registerPredefinedWorkflows()` 和 `unregisterPredefinedWorkflows()`。

| 测试场景 | 验证点 |
|----------|--------|
| **注册默认 llm_summary_workflow** | 成功注册，验证 workflow 字段（id, name, type, nodes, edges） |
| **注册自定义配置的 workflow** | 传入 compressionPrompt、timeout、maxTriggers，验证配置被应用 |
| **验证 workflow 结构** | 4 个节点（START_FROM_TRIGGER, LLM, CONTEXT_PROCESSOR, CONTINUE_FROM_TRIGGER）和 3 条边 |
| **skipIfExists 行为** | 同 tools 测试 |
| **allowList/blockList** | 白名单/黑名单过滤 |
| **取消注册** | 成功取消注册 |
| **isPredefinedWorkflowRegistered** | 验证注册状态查询 |

### 4. Predefined Prompts Registration

文件：`predefined-prompts-registration.int.test.ts`

**测试目标**：验证 `registerAllPredefinedPrompts()` 及其子函数。

| 测试场景 | 验证点 |
|----------|--------|
| **注册所有 fragments** | 16 个 fragment 全部注册成功（role, capability, constraint, tool-usage, user-commands） |
| **注册后 fragments 可查询** | 每个 fragment 可通过 FragmentRegistry.has() 查询 |
| **registerPredefinedPromptTemplates** | 注册所有 prompt template（当前为空列表，验证空安全） |
| **registerAllPredefinedPrompts 整体流程** | 先注册 fragment，再设置 cross-reference，再注册 template |
| **skipIfExists 行为** | 重复注册跳过 |
| **areFragmentsRegistered** | 全部注册后返回 true，部分注册后返回 false |
| **arePromptTemplatesRegistered** | 全部注册后返回 true |
| **fragment 内容验证** | 验证关键 fragment 的内容格式（ASSISTANT_ROLE, CODER_ROLE 等） |
| **cross-reference 设置** | 注册后 fragmentRegistry 已关联到 promptTemplateRegistry |

### 5. Predefined Tool Descriptions Registration

文件：`predefined-tool-descriptions-registration.int.test.ts`

**测试目标**：验证 `registerAllPredefinedToolDescriptions()`。

| 测试场景 | 验证点 |
|----------|--------|
| **注册所有工具描述** | 20 个工具描述全部注册成功 |
| **注册后描述可查询** | 每个工具描述可通过 ToolDescriptionRegistry.has() 和 get() 查询 |
| **skipIfExists 行为** | 重复注册跳过 |
| **arePredefinedToolDescriptionsRegistered** | 全部注册后返回 true，未注册时返回 false |
| **描述内容完整性** | 每个描述包含 id、summary、parameters 等字段 |

### 6. Predefined Builtin Tools

文件：`predefined-builtin-tools.int.test.ts`

**测试目标**：验证 `createBuiltinTools()` 和 `createPredefinedTools()` 的工具创建逻辑。

| 测试场景 | 验证点 |
|----------|--------|
| **创建所有内置工具** | execute_workflow, query_workflow_status, cancel_workflow, call_agent, ask_followup_question, attempt_completion 全部创建 |
| **allowList 过滤** | 只创建 allowList 中的工具 |
| **blockList 过滤** | 跳过 blockList 中的工具 |
| **allowList 优先级高于 blockList** | 同时设置时 allowList 生效 |
| **workflow 配置** | 传入 workflow loader，工具描述包含可用工作流列表 |
| **agent 配置** | 传入 agent loader，工具描述包含可用 Agent 列表 |
| **createAllPredefinedTools** | 返回所有工具（含 stateless + stateful + builtin），类型为 Tool[] |
| **工具描述渲染** | 验证工具描述通过 renderToolDescription 渲染 |

### 7. Risk Classification

文件：`predefined-risk-classification.int.test.ts`

**测试目标**：验证 `TOOL_RISK_CLASSIFICATION` 和相关查询函数。

| 测试场景 | 验证点 |
|----------|--------|
| **所有预定义工具都有风险等级** | 每个工具在 `PREDEFINED_TOOL_IDS` 和 `TOOL_RISK_CLASSIFICATION` 中都有对应条目 |
| **getToolRiskLevel 查询** | 已知工具返回正确等级，未知工具默认 WRITE |
| **hasKnownRiskLevel** | 已知工具返回 true，未知工具返回 false |
| **getToolsByRiskLevel** | 按风险等级过滤返回正确工具列表 |
| **getRiskLevelStats** | 返回各等级统计信息，总和等于工具总数 |
| **SECURITY_PRESETS 完整性** | SAFE, BALANCED, PERMISSIVE 三个预设每个字段都有值 |

### 8. Predefined Starters

文件：`predefined-starters.int.test.ts`

**测试目标**：验证 Starter 注册、激活和停用流程。

| 测试场景 | 验证点 |
|----------|--------|
| **StarterRegistry 注册 Starter** | GoalReviewStarter 注册成功，重复注册抛异常 |
| **StarterRegistry 列出 Starter** | list() 返回所有注册的 Starter |
| **Starter 激活** | activate() 调用 onBeforeAssemble → assemble → 注册到 registries → onAfterInstall |
| **Starter 停用** | deactivate() 调用 onBeforeUninstall → 取消注册 → onAfterUninstall |
| **Starter 获取** | get(id) 返回正确的 Starter 实例 |
| **Starter 注册/取消注册** | unregister(id) 后 get() 返回 undefined |
| **激活不存在的 Starter** | 抛出错误 |
| **BaseStarter 生命周期** | 验证 onBeforeAssemble/onAfterInstall/onBeforeUninstall/onAfterUninstall 被正确调用 |

### 9. Custom Resources - Loading

文件：`custom-resources-loading.int.test.ts`

**测试目标**：验证从 JSON 配置文件加载自定义资源。

| 测试场景 | 验证点 |
|----------|--------|
| **加载自定义工具（有效 JSON）** | 成功解析并返回 CustomToolDefinition[] |
| **加载自定义触发器（有效 JSON）** | 成功解析并返回 CustomTriggerDefinition[] |
| **加载自定义提示词（有效 JSON）** | 成功解析并返回 CustomPromptDefinition[] |
| **loadCustomResourcesFromConfig 加载全部** | 同时加载工具、触发器、提示词 |
| **文件不存在时返回空数组** | 不抛出异常，返回空数组 |
| **JSON 解析错误时抛出异常** | SyntaxError 被捕获并抛出有意义错误信息 |
| **缺少必要字段时验证失败** | 缺少 id/type/description 等字段时抛出验证错误 |
| **enabled=false 时不加载** | 直接返回空资源 |
| **部分路径有效时部分加载** | 一个路径有效、一个路径无效，errors 数组包含失败信息 |
| **相对路径/绝对路径解析** | 验证相对路径基于 baseDir 解析，绝对路径直接使用 |

### 10. Custom Resources - Registration

文件：`custom-resources-registration.int.test.ts`

**测试目标**：验证自定义资源注册到对应 registry。

| 测试场景 | 验证点 |
|----------|--------|
| **registerCustomTools** | 自定义工具注册到 ToolRegistry，验证可查询 |
| **registerCustomTriggers** | 自定义触发器注册到 TriggerTemplateRegistry |
| **registerCustomPrompts** | 自定义提示词注册到 PromptTemplateRegistry |
| **registerCustomResources 批量注册** | 一次调用注册所有三种资源，返回 `CustomResourcesRegistrationResult` |
| **空资源列表注册** | 传入空数组，所有结果 success=0 |
| **重复注册处理** | 重复 ID 注册时记录到 failures |
| **handler 类型转换** | file/inline/rpc 三种 handler 类型都正确转换 |
| **注册结果格式** | 验证 success 和 failures 数组格式正确 |

### 11. Dynamic System Context

文件：`dynamic-system-context.int.test.ts`

**测试目标**：验证 `buildSystemContextPrompt()` 和 context fragments。

| 测试场景 | 验证点 |
|----------|--------|
| **无配置时返回空字符串** | 不传 config 返回 "" |
| **includeCurrentTime 启用** | 返回包含当前时间 ISO 字符串的片段 |
| **includeEnvironmentInfo 启用** | 返回包含 OS、时区、语言信息的片段 |
| **customSections** | 自定义段被正确追加 |
| **缓存机制 - 命中缓存** | 相同配置在 TTL 内返回缓存内容 |
| **缓存机制 - TTL 过期** | 超时后重新生成（跳过或用较短 TTL 验证） |
| **缓存键隔离** | 不同配置使用不同缓存键 |
| **disableCache (TTL=0)** | 每次都重新生成，不缓存 |
| **generateCurrentTimeSection** | 格式正确，包含 "CURRENT TIME" 标题 |
| **generateEnvironmentSection** | 格式正确，包含 "ENVIRONMENT" 标题 |
| **getDefaultEnvironmentInfo** | 返回正确的 OS、时区、语言值 |
| **generateAvailableToolsContent** | 传入工具数组返回格式化描述，空数组返回 null |
| **generateSkillsContent** | 传入技能数组返回格式化内容，空数组返回 "" |
| **generateWorkflowsContent** | 传入工作流数组返回格式化内容，非法输入返回 "" |
| **generateToolDescriptionMessage** | 返回 LLM 消息对象 |

### 12. Dynamic User Context

文件：`dynamic-user-context.int.test.ts`

**测试目标**：验证 `buildUserContextContent()`。

| 测试场景 | 验证点 |
|----------|--------|
| **无 context 时返回空字符串** | 不传参数返回 "" |
| **TODO 列表注入** | 格式化为 "## Current TODOs" 列表，completed 标记 [x] |
| **Pinned files 注入** | 文件存在时包含内容，文件不存在时跳过 |
| **Workspace file tree 注入** | 包含 "## Workspace File Tree" 部分 |
| **Current time 注入** | 包含 ISO 格式时间戳 |
| **Custom data 注入** | 每个 key/value 格式化为 "## key" 和 JSON 值 |
| **多部分组合** | 多个部分同时存在时用空行分隔 |
| **空 section 过滤** | 不输出空的部分 |

### 13. Registration Orchestrator

文件：`registration-orchestrator.int.test.ts`

**测试目标**：验证 `registerAllResources()` 核心编排逻辑。

| 测试场景 | 验证点 |
|----------|--------|
| **默认配置注册所有资源** | 所有管道成功执行，返回完整的 `RegistrationResult` |
| **promptsEnabled=false 跳过提示词** | 不注册 fragments 和 prompt templates |
| **toolDescriptionsEnabled=false 跳过描述** | 不注册工具描述 |
| **contextCompression.enabled=false 跳过工作流** | 不注册 llm_summary_workflow 和 context_compression_trigger |
| **predefinedTools.enabled=false 跳过工具** | 不注册预定义工具 |
| **customResources 传入** | 自定义资源被注册，结果写入 custom 字段 |
| **applicationResources 传入** | 预留管道执行（不报错） |
| **starterConfig 传入** | GoalReviewStarter 被激活，工作流注册到 workflowRegistry |
| **空 presets 配置** | 所有资源默认启用，全部注册 |
| **部分注册失败** | 某个 registry 抛出异常时，不影响其他资源的注册 |
| **返回结果格式** | 验证 `RegistrationResult` 的嵌套结果结构完整 |

### 14. End-to-End Resource Registration

文件：`registration-end-to-end.int.test.ts`

**测试目标**：完整的端到端集成测试，模拟真实应用场景。

| 测试场景 | 验证点 |
|----------|--------|
| **完整注册流程** | 从创建所有 registry → 调用 registerAllResources → 验证所有注册结果 |
| **注册后验证** | 每个 registry 中验证所有资源存在 |
| **取消注册流程** | 调用 unregisterPredefinedContent 取消所有资源 |
| **取消后验证** | 每个 registry 中验证所有资源已移除 |
| **重复注册幂等性** | 连续调用两次 registerAllResources，第二次不报错 |
| **自定义配置注册** | 传入自定义 presets 配置，验证注册行为受配置控制 |
| **Starter 激活完整流程** | 从注册 Starter 到安装 WorkflowBundle 到停用 |
| **资源隔离** | 不同 registry 之间的资源不会相互影响 |

## 测试夹具设计

### mock-registries.ts

提供所有 registry 的 mock/fake 实现，用于隔离测试：

```typescript
// 核心接口：为每个 registry 提供真实实现（或轻量 fake）
export function createMockRegistries(): ResourceRegistries {
  return {
    triggerRegistry: new TriggerTemplateRegistry(),
    workflowRegistry: new WorkflowRegistry(),
    toolRegistry: new ToolRegistry(),
    promptTemplateRegistry: new PromptTemplateRegistry(),
    fragmentRegistry: new FragmentRegistry(),
    toolDescriptionRegistry: new ToolDescriptionRegistry(),
    nodeTemplateRegistry: new NodeTemplateRegistry(),
    hookTemplateRegistry: new HookTemplateRegistry(),
    agentLoopRegistry: new AgentLoopRegistry(),
  };
}
```

### fixtures.ts

提供测试数据和工厂函数：

- `createTestCustomToolDefinition()` - 创建有效的自定义工具定义
- `createTestCustomTriggerDefinition()` - 创建有效的自定义触发器定义
- `createTestCustomPromptDefinition()` - 创建有效的自定义提示词定义
- `createTestPresetsConfig()` - 创建完整的 PresetsConfig
- `createCustomResourcesJsonFile()` - 在临时目录创建 JSON 配置文件
- `createTempDir()` - 创建临时目录（测试后清理）

## 测试数据模板

### 自定义工具 JSON 模板

```json
{
  "tools": [
    {
      "id": "my_custom_tool",
      "type": "STATELESS",
      "description": {
        "summary": "My custom tool",
        "details": "Detailed description",
        "examples": ["Example 1"]
      },
      "schema": {
        "type": "object",
        "properties": {
          "param1": { "type": "string", "description": "A parameter" }
      },
      "handler": {
        "type": "inline",
        "code": "export default async (params) => { return { result: 'ok' }; }"
      }
    }
  ]
}
```

### 自定义触发器 JSON 模板

```json
{
  "triggers": [
    {
      "name": "my_custom_trigger",
      "description": "My custom trigger",
      "condition": {
        "type": "event",
        "value": "CUSTOM_EVENT"
      }
    }
  ]
}
```

### 自定义提示词 JSON 模板

```json
{
  "prompts": [
    {
      "id": "my_custom_prompt",
      "name": "My Custom Prompt",
      "content": "You are a helpful assistant.",
      "type": "system",
      "variables": ["language"]
    }
  ]
}
```

## 优先级建议

### P0（必须优先实现）

1. `registration-orchestrator.int.test.ts` - 核心编排逻辑，影响面最广
2. `predefined-tools-registration.int.test.ts` - 工具注册是最常用的功能
3. `predefined-prompts-registration.int.test.ts` - 提示词注册是 AI 交互基础
4. `registration-end-to-end.int.test.ts` - 端到端验证最完整

### P1（重要）

5. `custom-resources-loading.int.test.ts` - 用户自定义资源加载
6. `custom-resources-registration.int.test.ts` - 用户自定义资源注册
7. `predefined-tool-descriptions-registration.int.test.ts` - 工具描述注册
8. `dynamic-system-context.int.test.ts` - 系统上下文动态生成

### P2（补充验证）

9. `predefined-triggers-registration.int.test.ts`
10. `predefined-workflows-registration.int.test.ts`
11. `predefined-builtin-tools.int.test.ts`
12. `predefined-risk-classification.int.test.ts`
13. `predefined-starters.int.test.ts`
14. `dynamic-user-context.int.test.ts`

## 执行方法

```bash
# 运行所有 resources 集成测试
cd packages/sdk
pnpm test __tests__/integration/resources/

# 运行单个测试文件
pnpm test __tests__/integration/resources/registration-orchestrator.int.test.ts

# 运行特定测试
pnpm test __tests__/integration/resources/registration-orchestrator.int.test.ts -- --grep "default config"
```

## 注意事项

1. **Registry 隔离**：每个测试文件应使用独立的 registry 实例，避免测试间相互影响
2. **临时文件清理**：自定义资源加载测试需在 `afterEach` 中清理临时 JSON 文件
3. **时间敏感测试**：系统上下文测试中的时间断言需使用近似匹配（如 toBeCloseTo），避免毫秒级偏差
4. **缓存测试**：系统上下文缓存测试需注意 TTL，可使用短 TTL 或直接操作缓存 Map
5. **异步安全**：所有注册函数（尤其是 Starter 激活）都需 await 完成
6. **skipIfExists 默认 true**：测试中显式传参，不依赖默认值