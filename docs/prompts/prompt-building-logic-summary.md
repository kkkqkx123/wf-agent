# 提示词构建逻辑总结

## 概述

本文档总结 SDK 中提示词（Prompt）的构建逻辑，涵盖从片段组合到最终消息数组的完整流程。

## 整体架构

提示词构建分为三个层次，自底向上：

```
┌─────────────────────────────────────────────────────────────┐
│                   调用层 (Callers)                           │
│  call-agent/handler.ts  message-context-utils.ts            │
│  (调用 resolveSystemPrompt / buildInitialMessages)          │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│             核心解析层 (core/messaging/prompt/)              │
│  resolveSystemPrompt()  ← 模板解析 + 变量渲染               │
│  buildInitialMessages() ← 构建 LLMMessage[] 消息数组        │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│           预定义片段组合层 (resources/predefined/prompts/)   │
│  buildSystemPrompt()  →  buildCompleteSystemPrompt()        │
│  composeSystemPrompt() → FragmentRegistry.render()          │
└─────────────────────────────────────────────────────────────┘
```

## 层次详解

### 1. 预定义片段组合层

**位置**: `sdk/resources/predefined/prompts/`

**职责**: 将多个 `SystemPromptFragment` 组合成完整的系统提示词字符串。

**核心组件**:

- [`FragmentRegistry`](sdk/resources/predefined/prompt-templates/fragment-registry.ts:4) — 独立的片段注册表，管理 `SystemPromptFragment` 对象（与 `PromptTemplateRegistry` 无关）
- [`composeSystemPrompt()`](sdk/resources/predefined/prompts/fragments/composer.ts:36) — 接收片段 ID 数组，从 `FragmentRegistry` 查找，渲染变量，用分隔符拼接
- [`buildCompleteSystemPrompt()`](sdk/resources/predefined/prompts/fragments/composer.ts:67) — 在 `composeSystemPrompt()` 基础上附加动态工具描述
- [`buildSystemPrompt()`](sdk/resources/predefined/prompts/system/system-prompt-builder.ts:82) — 高层入口，选择基础片段集（assistant/coder），支持增删片段，调用 `buildCompleteSystemPrompt()`

**数据流**:

```typescript
// 示例：构建编码助手系统提示词
const result = buildSystemPrompt({
  type: "coder",
  tools: [...],
  additionalFragments: ["fragments.task-instruction.code-review"],
  excludeFragments: ["fragments.constraint.code-safety"],
});
// 结果：单个字符串，由多个片段用 "\n\n" 拼接而成
```

**预定义片段分类**:

| 分类 | 示例 ID | 说明 |
|------|---------|------|
| role | `fragments.role.assistant` | 角色定义 |
| capability | `fragments.capability.coding` | 能力描述 |
| constraint | `fragments.constraint.general` | 约束规则 |
| tool-usage | `fragments.tool-usage.xml-summary` | 工具使用说明 |
| task-instruction | `fragments.task-instruction.code-review` | 任务指令（动态注入） |

### 2. 核心解析层

**位置**: `sdk/core/messaging/prompt/`

**职责**: 将提示词配置解析为字符串或消息数组，不关心片段组合逻辑。

#### 2.1 resolveSystemPrompt()

[`resolveSystemPrompt()`](sdk/core/messaging/prompt/system-prompt-resolver.ts:19) 接收 [`SystemPromptConfig`](sdk/core/messaging/prompt/system-prompt-resolver.ts:5)，返回解析后的提示词字符串：

```typescript
interface SystemPromptConfig {
  systemPrompt?: string;                    // 直接字符串
  systemPromptTemplateId?: string;          // 模板 ID（优先）
  systemPromptTemplateVariables?: Record<string, unknown>;  // 变量渲染
}
```

**解析逻辑**:

1. 如果提供了 `systemPromptTemplateId`，从 `PromptTemplateRegistry` 查找模板
   - 找到 → 使用模板内容
   - 未找到 → 日志警告，回退到 `systemPrompt` 或空字符串
2. 否则如果提供了 `systemPrompt`，直接使用
3. 否则返回空字符串
4. 如果提供了 `systemPromptTemplateVariables`，对结果进行变量渲染

#### 2.2 buildInitialMessages()

[`buildInitialMessages()`](sdk/core/messaging/prompt/initial-message-builder.ts:14) 接收 [`InitialMessagesConfig`](sdk/core/messaging/prompt/initial-message-builder.ts:6)，返回 `LLMMessage[]` 消息数组：

```typescript
interface InitialMessagesConfig extends SystemPromptConfig {
  initialUserMessage?: string;                    // 初始用户消息
  initialUserMessageTemplateId?: string;          // 用户消息模板 ID
  initialUserMessageTemplateVariables?: Record<string, unknown>;
  existingMessages?: LLMMessage[];                // 已有消息
  initialMessages?: LLMMessage[];                 // 完全自定义初始消息（最高优先级）
}
```

**构建逻辑**:

1. 如果提供了 `initialMessages`（非空），直接返回其副本
2. 否则按顺序构建：
   - 调用 `resolveSystemPrompt()` 解析系统提示词 → 如果非空，添加 `{ role: "system" }` 消息
   - 解析初始用户消息：
     - 如果提供了 `initialUserMessageTemplateId`，用 `templateRegistry.render()` 渲染
     - 渲染成功 → 使用渲染结果
     - 渲染失败（模板不存在）→ 日志警告，回退到 `initialUserMessage`
     - 如果都没有 → 不添加用户消息
   - 如果提供了 `existingMessages`，过滤掉 `role: "system"` 的消息后追加

### 3. 调用层

**位置**: 各业务模块

**实际调用场景**:

| 调用方 | 使用函数 | 用途 |
|--------|----------|------|
| [`call-agent/handler.ts`](sdk/resources/predefined/tools/builtin/agent/call-agent/handler.ts:178) | `resolveSystemPrompt()` | 解析 call-agent 工具的系统提示词 |
| [`message-context-utils.ts`](sdk/core/messaging/message-context-utils.ts:45) | `resolveSystemPrompt()` | 初始化工作流执行上下文的消息 |

## 设计要点

### 片段组合 vs 模板解析的分离

片段组合（`resources/predefined/prompts/`）和模板解析（`core/messaging/prompt/`）是独立的两个层次：

- **片段组合层**关注 "用哪些片段、按什么顺序、用什么分隔符"——属于内容组织
- **核心解析层**关注 "从模板还是直接字符串获取、变量如何渲染"——属于解析逻辑

这种分离是合理的，因为片段组合是预定义内容层的职责，而核心层只需处理解析后的字符串。

### 同步设计

`buildInitialMessages()` 和 `resolveInitialUserMessage()` 均为同步函数，因为所有底层操作（`templateRegistry.render()`、`resolveSystemPrompt()`）都是同步的。

### 消息去重

`buildInitialMessages()` 在追加 `existingMessages` 时会自动过滤 `role: "system"` 的消息，避免与通过 `resolveSystemPrompt()` 添加的系统消息重复。

## 完整流程示例

```typescript
// 1. 片段组合层：构建系统提示词字符串
const systemPrompt = buildSystemPrompt({
  type: "assistant",
  tools: toolList,
});
// → "You are a helpful assistant.\n\nGeneral capabilities...\n\n### Available Tools\n..."

// 2. 核心解析层：构建消息数组
const messages = buildInitialMessages({
  systemPrompt,                              // 来自片段组合层的结果
  initialUserMessage: "Hello",
  existingMessages: history,
});
// → [
//   { role: "system", content: "You are a helpful assistant..." },
//   { role: "user", content: "Hello" },
//   ...existingMessages (不含 system 角色)
// ]
```

## 相关文件

| 文件 | 说明 |
|------|------|
| `sdk/core/messaging/prompt/system-prompt-resolver.ts` | 系统提示词解析核心 |
| `sdk/core/messaging/prompt/initial-message-builder.ts` | 初始消息构建核心 |
| `sdk/core/messaging/prompt/index.ts` | 统一导出 |
| `sdk/resources/predefined/prompts/fragments/composer.ts` | 片段组合工具 |
| `sdk/resources/predefined/prompts/fragments/registry.ts` | 片段注册表 |
| `sdk/resources/predefined/prompts/system/system-prompt-builder.ts` | 系统提示词构建器 |
| `sdk/resources/predefined/prompt-templates/fragment-registry.ts` | FragmentRegistry 实现 |
| `sdk/resources/predefined/template-registry.ts` | PromptTemplateRegistry 实现 |
| `sdk/core/messaging/message-context-utils.ts` | 工作流上下文初始化（调用方） |
| `sdk/resources/predefined/tools/builtin/agent/call-agent/handler.ts` | call-agent 工具（调用方） |
