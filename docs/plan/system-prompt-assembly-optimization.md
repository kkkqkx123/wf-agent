# System Prompt Assembly Optimization Plan

> Analysis date: 2026-07-16
> Scope: System prompt injection pipeline across `packages/sdk/` (agent, resources, shared/messaging, services/executors/mcp) and `apps/cli-app/`

---

## 1. Current Architecture Overview

The system prompt is assembled through a **5-layer injection pipeline**:

```
Layer 1: Base System Prompt — AgentLoopFactory.create()
    ↓
Layer 2: Skill Metadata — {SKILLS_METADATA} placeholder replacement (CLI-only)
    ↓
Layer 3: Dynamic Context (transformContext) — append to system/user message
    ↓
Layer 4: MCP Tools Context — independent user message injection
    ↓
Layer 5: Tool Schemas — LLM API function-calling format (not in prompt text)
```

Each layer has its own injection mechanism, type system, and integration point.

---

## 2. Issues Found

### 🔴 Critical

#### 1. MCP `createTransformContextFn` type incompatible with core `TransformContextFn`

**Files**:
- `packages/sdk/services/executors/mcp/features/metadata/dynamic-context-provider.ts:247-277`
- `packages/types/src/agent-execution/context.ts:99-101`

**Problem**:
`McpToolsDynamicContextProvider.createTransformContextFn()` returns `(messages: LLMMessage[]) => Promise<LLMMessage[]>` — a message-level preprocessor. But the core coordinator uses `TransformContextFn` (defined in `packages/types/src/agent-execution/context.ts`):

```typescript
export type TransformContextFn = (
  context: DynamicPromptContext,
) => Promise<DynamicPromptInjection>;
// DynamicPromptInjection = { staticSystem?, dynamicUserContext? }
```

These two types are **completely incompatible**. MCP's context provider cannot be plugged into the standard `transformContext` mechanism. If a user wants both MCP context and dynamic context (time, environment), they must implement two separate injection paths.

**Impact**: MCP context injection bypasses the standard `transformContext` pipeline, making it impossible to compose MCP context with other dynamic injections through a unified mechanism.

#### 2. Skill metadata injection is CLI-layer only, not in SDK core

**Files**:
- `apps/cli-app/src/adapters/skill-adapter.ts:224-235`
- `packages/sdk/agent/execution/factories/agent-loop-factory.ts:138-225`

**Problem**:
`{SKILLS_METADATA}` placeholder replacement is implemented in `SkillAdapter.injectSkillsMetadata()` which is a CLI adapter method. The SDK core's `AgentLoopFactory.create()` does not process this placeholder. SDK API users who don't go through the CLI will not get skill metadata injected.

**Impact**: The `{SKILLS_METADATA}` placeholder in system prompts is effectively a no-op when using the SDK programmatically.

### 🟠 Medium

#### 3. `buildSystemContextPrompt` ignores `DynamicContextConfig`

**File**: `packages/sdk/resources/dynamic/system-context/builder.ts:29-47`

**Problem**:
The function accepts `_config?: DynamicContextConfig` but ignores it entirely. It always generates the current time section and environment section. `DynamicContextConfig` defines flags like `includeCurrentTime`, `includeEnvironmentInfo` etc. that are supposed to control what gets included, but they are never checked.

**Impact**: Users cannot disable the time/environment info injection even if they set `includeCurrentTime: false` or `includeEnvironmentInfo: false` in their config. The entire `DynamicContextConfig` is dead configuration.

#### 4. `buildUserContextContent` data source not connected to execution flow

**File**: `packages/sdk/resources/dynamic/user-context/builder.ts:28-101`

**Problem**:
The function implements TODO list, pinned files, workspace file tree, current time, and custom data rendering. However, the `DynamicRuntimeContext` data that feeds into it has no integrated pipeline to populate it from the execution environment. The metadata passed from `context.metadata` in `applyDynamicContextToConfig` (cli-app) is generic and not structured as `DynamicRuntimeContext`.

**Impact**: The user context builder effectively always returns an empty string at runtime (also noted in `docs/plan/residual-todo-analysis.md` as "all features not implemented").

#### 5. `systemPromptTemplateId` not resolved in main AgentLoop path

**Files**:
- `packages/sdk/agent/execution/factories/agent-loop-factory.ts:138-225`
- `packages/sdk/shared/messaging/prompt/system-prompt-resolver.ts:20-49`

**Problem**:
`AgentLoopFactory.create()` only checks `config.systemPrompt` (a direct string). The `config.systemPromptTemplateId` field is not resolved in this path. `resolveSystemPrompt()` is only called from the workflow handler (`agent-loop-handler.ts`) and the `call-agent` tool handler.

**Impact**: When creating an AgentLoop via `AgentLoopCoordinator.execute()`, `systemPromptTemplateId` is silently ignored — the template is never resolved, and no system prompt is set.

#### 6. No merge/priority strategy for multiple injection sources

**File**: `packages/sdk/shared/messaging/dynamic-injection.ts:26-99`

**Problem**:
When multiple injection sources exist (Skill metadata, dynamic context, MCP context), `injectDynamicPrompts` simply concatenates strings. There is no defined ordering, deduplication, or conflict resolution strategy. For example, if both skill metadata and dynamic context append to the system message, the order depends on injection timing, which is not guaranteed.

### 🟢 Low

#### 7. Text-form tool description not used in agent loop iteration

**Files**:
- `packages/sdk/shared/messaging/conversation-session.ts:298-303`
- `packages/sdk/shared/tools/tool-description-generator.ts:53-56`

**Problem**:
`getToolDescriptionMessage()` and `generateToolListSummary()` exist but are never called in the agent loop iteration path. Tool descriptions are only sent via LLM API's function-calling `tools` parameter. Some LLMs benefit from redundant text-form tool descriptions in the system prompt.

#### 8. `buildSystemContextPrompt` called every iteration without caching

**File**: `packages/sdk/resources/dynamic/system-context/builder.ts:29-47`

**Problem**:
Current time and environment info are stable content that rarely changes. However, `buildSystemContextPrompt` is called on every LLM iteration (via `transformContext`). While the impact is small, it wastes CPU cycles generating the same content repeatedly.

---

## 3. Proposed Changes

### 3.1 Unify MCP transform context with core `TransformContextFn` [🔴 Critical]

**Goal**: Make `McpToolsDynamicContextProvider` compatible with the standard `TransformContextFn` type so MCP context can be injected through the same `transformContext` mechanism.

**Changes**:

1. **Add a new method to `McpToolsDynamicContextProvider`** that returns a proper `TransformContextFn`:

   In `packages/sdk/services/executors/mcp/features/metadata/dynamic-context-provider.ts`, add:
   ```typescript
   createDynamicPromptInjectionFn(options?: McpToolsContextOptions): TransformContextFn {
     return async (_context: DynamicPromptContext): Promise<DynamicPromptInjection> => {
       const mcpContext = this.generateContext(options);
       if (!mcpContext.hasServers) {
         return { staticSystem: undefined, dynamicUserContext: undefined };
       }
       return {
         staticSystem: mcpContext.content,  // Inject as system prompt suffix
         dynamicUserContext: undefined,
       };
     };
   }
   ```

2. **Keep the existing `createTransformContextFn`** for backward compatibility, but deprecate it in documentation.

3. **Update `TransformContextFn` signature** (optional): Add a `messages?: LLMMessage[]` parameter to the `DynamicPromptContext` so the MCP function can inspect existing messages if needed.

**Integration**: Now users can compose MCP context with other dynamic injections:
```typescript
config.transformContext = async (ctx) => {
  const mcpInjection = await mcpProvider.createDynamicPromptInjectionFn()(ctx);
  const systemInjection = await buildSystemContextPrompt(config.dynamicContextConfig);
  return {
    staticSystem: [mcpInjection.staticSystem, systemInjection].filter(Boolean).join("\n\n"),
    dynamicUserContext: undefined,
  };
};
```

### 3.2 Move Skill metadata injection into SDK core [🔴 Critical]

**Goal**: `{SKILLS_METADATA}` placeholder replacement should be handled in `AgentLoopFactory.create()` or `AgentLoopEntity` initialization, not in the CLI adapter.

**Changes**:

1. **In `AgentLoopFactory.create()`** (`packages/sdk/agent/execution/factories/agent-loop-factory.ts`), after setting the system prompt message, call `SkillRegistry.injectSkillsMetadata()` if the registry is available:

   ```typescript
   // After: initialMessages.push({ role: "system", content: config.systemPrompt });
   if (config.systemPrompt) {
     let systemContent = config.systemPrompt;
     // Resolve template if systemPromptTemplateId is set
     if (config.systemPromptTemplateId) {
       const registry = globalContext.container.get(Identifiers.PromptTemplateRegistry);
       systemContent = resolveSystemPrompt(
         { systemPrompt: systemContent, systemPromptTemplateId: config.systemPromptTemplateId, systemPromptTemplateVariables: config.systemPromptTemplateVariables },
         registry,
       );
     }
     // Inject skill metadata if placeholder exists
     try {
       const skillRegistry = globalContext.container.get(Identifiers.SkillRegistry);
       systemContent = skillRegistry.injectSkillsMetadata(systemContent);
     } catch { /* skill registry not available */ }
     initialMessages.push({ role: "system", content: systemContent });
   }
   ```

2. **Remove the CLI-only `injectSkillsMetadata`** from `SkillAdapter` (or keep it as a thin wrapper that calls the SDK core method).

3. **Update `SkillRegistry.injectSkillsMetadata()`** to accept `systemPrompt` and return the modified prompt, making it self-contained.

### 3.3 Make `buildSystemContextPrompt` respect `DynamicContextConfig` [🟠 Medium]

**File**: `packages/sdk/resources/dynamic/system-context/builder.ts:29-47`

**Changes**:

```typescript
export async function buildSystemContextPrompt(
  config?: DynamicContextConfig,
): Promise<string> {
  const sections: string[] = [];

  // 1. Current time (only if enabled or no config specified for backward compatibility)
  if (!config || config.includeCurrentTime !== false) {
    sections.push(generateCurrentTimeSection());
  }

  // 2. Environment information (only if enabled or no config specified)
  if (!config || config.includeEnvironmentInfo !== false) {
    const envInfo = getDefaultEnvironmentInfo();
    const envSection = generateEnvironmentSection(envInfo);
    if (envSection) {
      sections.push(envSection);
    }
  }

  // 3. Custom sections
  if (config?.customSections) {
    for (const [key, value] of Object.entries(config.customSections)) {
      sections.push(wrapSection(key.toUpperCase(), value));
    }
  }

  const combinedPrompt = sections.filter(Boolean).join("\n\n");
  return cleanupEmptyLines(combinedPrompt);
}
```

### 3.4 Connect `buildUserContextContent` data source to execution flow [🟠 Medium]

**Goal**: Provide a real `DynamicRuntimeContext` to `buildUserContextContent` instead of the current empty metadata.

**Changes**:

1. **In `applyDynamicContextToConfig`** (`apps/cli-app/src/adapters/agent-loop-adapter.ts:56-93`), restructure the `transformContext` to properly build `DynamicRuntimeContext` from available runtime data:

   ```typescript
   config.transformContext = async (context) => {
     const staticSystem = await buildSystemContextPrompt(mergedConfig);

     // Build DynamicRuntimeContext from execution metadata
     const runtimeContext: DynamicRuntimeContext = {
       currentTime: Date.now(),
       todoList: context.metadata?.todoList as TodoItem[] | undefined,
       pinnedFiles: context.metadata?.pinnedFiles as PinnedFileItem[] | undefined,
       workspaceFileTree: context.metadata?.workspaceFileTree as string | undefined,
       customData: context.metadata?.customData as Record<string, unknown> | undefined,
     };

     const dynamicUserContext = Object.keys(runtimeContext).length > 0
       ? await buildUserContextContent(runtimeContext)
       : undefined;

     return {
       staticSystem: staticSystem || undefined,
       dynamicUserContext: dynamicUserContext || undefined,
     };
   };
   ```

2. **Provide a metadata pipeline**: The metadata in `context.metadata` should be populated by the caller (CLI or SDK user) before execution. Document the expected metadata shape.

### 3.5 Resolve `systemPromptTemplateId` in `AgentLoopFactory.create()` [🟠 Medium]

**File**: `packages/sdk/agent/execution/factories/agent-loop-factory.ts`

**Changes**:

In the `create()` method, before building initial messages, resolve the system prompt:

```typescript
// Resolve system prompt (supports template rendering)
const resolvedSystemPrompt = resolveSystemPrompt(
  {
    systemPrompt: config.systemPrompt,
    systemPromptTemplateId: config.systemPromptTemplateId,
    systemPromptTemplateVariables: config.systemPromptTemplateVariables,
  },
  globalContext.container.get(Identifiers.PromptTemplateRegistry),
);
```

Then use `resolvedSystemPrompt` instead of `config.systemPrompt` when building the system message.

**Dependency**: This requires `PromptTemplateRegistry` to be registered in the DI container. If not available, fall back to `config.systemPrompt`.

### 3.6 Define merge/priority strategy for injection sources [🟠 Medium]

**Goal**: Establish a clear ordering and merging strategy for all injection sources.

**Changes**:

1. **Document the injection order** (from highest priority to lowest):
   ```
   1. Base system prompt (from config / template)
   2. Skill metadata ({SKILLS_METADATA})
   3. Static dynamic context (time, environment, MCP)
   4. Dynamic user context (TODO, pinned files, workspace)
   5. Tool schemas (LLM API function-calling format)
   ```

2. **In `injectDynamicPrompts`**, ensure the `staticSystem` suffix is appended in a consistent order. If multiple callers append to the same message, consider using a delimiter like `\n---\n` between sections.

3. **For the `transformContext` composition**, provide a utility function that merges multiple `DynamicPromptInjection` results:

   ```typescript
   function mergeDynamicInjections(
     injections: DynamicPromptInjection[],
   ): DynamicPromptInjection {
     return {
       staticSystem: injections.map(i => i.staticSystem).filter(Boolean).join("\n\n---\n\n"),
       dynamicUserContext: injections.map(i => i.dynamicUserContext).filter(Boolean).join("\n\n"),
     };
   }
   ```

### 3.7 Add text-form tool description to system prompt (optional) [🟢 Low]

**File**: `packages/sdk/agent/execution/coordinators/agent-iteration-coordinator.ts`

**Change**: In the iteration execution, optionally include a text-form tool description in the system prompt or as a user message, complementing the API-level tool schemas. This can be controlled by a config flag (`includeToolDescriptionsInPrompt?: boolean`).

### 3.8 Add caching for stable system context [🟢 Low]

**File**: `packages/sdk/resources/dynamic/system-context/builder.ts`

**Change**: Cache the result of `buildSystemContextPrompt` with a TTL (e.g., 60 seconds) since current time and environment info rarely change. Invalidate the cache when the timezone or OS changes (essentially never).

---

## 4. Implementation Order

| Priority | Change | Effort | Risk | Dependencies |
|----------|--------|--------|------|-------------|
| 1 | **3.1** Unify MCP transform context type | 2 days | Low | None |
| 2 | **3.2** Move Skill injection to SDK core | 1 day | Low | 3.5 |
| 3 | **3.5** Resolve `systemPromptTemplateId` in factory | 1 day | Low | None |
| 4 | **3.3** Make `buildSystemContextPrompt` respect config | 0.5 day | Low | None |
| 5 | **3.4** Connect user context data source | 1 day | Medium | None |
| 6 | **3.6** Define merge/priority strategy | 0.5 day | Low | 3.1, 3.3 |
| 7 | **3.7** Optional text-form tool descriptions | 0.5 day | Low | None |
| 8 | **3.8** Add caching for system context | 0.5 day | Low | 3.3 |

---

## 5. Architecture Diagram (After Changes)

```
CLI / SDK User
  │
  ├─ config.systemPrompt / systemPromptTemplateId
  │   └─ AgentLoopFactory.create()
  │       ├─ resolveSystemPrompt() → template → variables
  │       ├─ SkillRegistry.injectSkillsMetadata() → {SKILLS_METADATA}
  │       └─ system 消息
  │
  ├─ config.transformContext (composed)
  │   ├─ McpToolsDynamicContextProvider.createDynamicPromptInjectionFn()
  │   │   └─ MCP 工具列表 → staticSystem
  │   ├─ buildSystemContextPrompt(config) ⊆ DynamicContextConfig
  │   │   └─ 时间 / 环境 / custom → staticSystem
  │   └─ buildUserContextContent(metadata) ⊆ DynamicRuntimeContext
  │       └─ TODO / pinned files / workspace → dynamicUserContext
  │
  └─ mergeDynamicInjections() → 合并所有注入源
      └─ injectDynamicPrompts() → 注入到 system/user 消息
```

---

## 6. Backward Compatibility

| Change | Compatibility | Notes |
|--------|--------------|-------|
| 3.1 | ✅ New method added; existing method kept | `createTransformContextFn` remains but deprecated |
| 3.2 | ✅ Skill registry fallback | If registry unavailable, placeholder is removed (current behavior) |
| 3.3 | ✅ Default behavior unchanged | `includeCurrentTime` and `includeEnvironmentInfo` default to `true` |
| 3.4 | ⚠️ Metadata shape change | Callers must provide structured `DynamicRuntimeContext` instead of arbitrary metadata |
| 3.5 | ✅ Template registry fallback | If registry unavailable, falls back to `config.systemPrompt` |
| 3.6 | ✅ New utility only | No breaking changes to existing code |
| 3.7 | ✅ Opt-in via config flag | Default disabled |
| 3.8 | ✅ Transparent caching | No external API change |