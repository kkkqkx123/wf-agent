# Skill Integration Analysis and Modification Plan

## 1. Current Architecture: Four-Layer Analysis

### 1.1 Type Definition Layer (`packages/types/src/skill.ts`)

Core types for the skill system:
- `Skill` - Full skill object (metadata + raw content + resource paths)
- `SkillMetadata` - Parsed SKILL.md frontmatter (name, description, version, tags, allowedTools)
- `SkillLoadContext` / `SkillLoadResult` - Loading context and result types
- `SkillResourceType` - Resource categories (references, examples, scripts, assets)
- `SkillConfig` - Registry configuration (paths, cache settings)

**Status**: Complete. All type definitions exist and are well-structured.

### 1.2 Core SDK Layer (`sdk/core/`)

Two key components:

**SkillRegistry** (`sdk/core/registry/skill-registry.ts`):
- Scans directories for skill folders with SKILL.md
- Parses SKILL.md frontmatter and registers skills
- Manages in-memory cache (contentCache, resourceCache)
- Supports enable/disable per skill
- Provides cache clear handler registration

**SkillLoader** (`sdk/core/utils/skill-loader.ts`):
- Implements three-level progressive disclosure:
  - Level 1: `generateMetadataPrompt()` - Brief metadata list for system prompt
  - Level 2: `loadContent()` - Full SKILL.md content on demand
  - Level 3: `loadResources()` - Nested resource loading
- Uses CheckpointStore for content caching with TTL
- Registers cache invalidation handlers with SkillRegistry

Also includes:
- Event builders for skill lifecycle (`sdk/core/utils/event/builders/skill-events.ts`)
- DI container bindings (`sdk/core/di/container-config.ts`)

**Status**: Core infrastructure is complete. Missing: reusable utility functions that bridge skills with LLM execution contexts.

### 1.3 API Layer (`sdk/api/`)

**SkillRegistryAPI** (`sdk/api/shared/resources/skills/skill-registry-api.ts`):
- Wraps SkillRegistry + SkillLoader with ExecutionResult pattern
- Exposes: scanSkills, getAll, get, loadContent, loadResources, generateMetadataPrompt, matchSkills, enable, disable, getEnabledSkills, getDisabledSkills, clearCache, reload

**Status**: Complete. Full API surface available.

### 1.4 Application Layer (`apps/cli-app/`)

**SkillAdapter** (`apps/cli-app/src/adapters/skill-adapter.ts`):
- Encapsulates SkillRegistryAPI calls
- Methods: initialize, listSkills, getSkill, loadContent, loadResources, generateMetadataPrompt, injectSkillsMetadata, matchSkills, enable, disable

**AgentLoopAdapter** (`apps/cli-app/src/adapters/agent-loop-adapter.ts`):
- `registerSkillTool()`: Registers the `skill` tool dynamically into the agent loop's ToolRegistry
- The tool handler calls `SkillLoader.loadContent()` to load full skill content

**Agent Command** (`apps/cli-app/src/commands/agent/index.ts`):
- Before execution: injects skill metadata into system prompt
- Registers `skill` tool in available tools list
- This is the ONLY integration point for skills today

**Status**: Agent loop integration exists in the CLI layer, but is tightly coupled to the CLI application. Not reusable by other consumers (e.g., workflow).

---

## 2. Problem Analysis

### 2.1 Current Integration is Application-Layer Only

The skill system's integration into execution flows currently exists **only** in `apps/cli-app/src/commands/agent/index.ts`. This means:

| Consumer | Skill Integration Status |
|----------|------------------------|
| CLI Agent Loop command | ✅ Manual injection in command handler |
| CLI Agent Loop adapter | ✅ `registerSkillTool()` available |
| Workflow LLM node | ❌ No integration |
| Workflow Agent Loop node | ❌ No integration |
| SDK AgentLoopExecutor | ❌ No integration |
| SDK LLMExecutionCoordinator | ❌ No integration |

### 2.2 Duplication Risk

If we independently integrate skills into:
- Agent loop's `AgentLoopExecutor` (sdk/agent)
- Workflow's `AgentLoopHandler` (sdk/workflow)
- Workflow's `LLMHandler` (sdk/workflow)

...we will end up with duplicated logic for:
- Skill metadata injection into system prompt
- Dynamic skill tool creation
- Skill enable/disable filtering

### 2.3 Progressive Disclosure Requires Context

The three-level progressive disclosure design requires:
- Level 1: Skill metadata prompt → Must be injected during **system prompt construction**
- Level 2: Content loading → Must be available as a **runtime tool** (`skill`)
- Level 3: Resource loading → Must be accessible via the same tool

Both agent loop and workflow share these same requirements.

---

## 3. Architecture Decision: Pure Utility Functions, Not a Service

### 3.1 Comparison: Skill vs Tool Complexity

| Aspect | Tool Module | Skill Module |
|--------|------------|-------------|
| Executor types | STATELESS, stateful, MCP, REST, approval | None (content loading only) |
| State management | ToolRegistry + ToolPermissionManager | SkillRegistry |
| Runtime lifecycle | execute, approve, reject, fail-protect | load, enable/disable |
| Cross-cutting concerns | Visibility, failure protection, approval | Metadata injection, tool registration |

**Tool module** is far more complex than skills, yet it doesn't have a "ToolIntegrationService" - tools are used directly via `ToolRegistry` + utility functions (e.g., `prepareToolSchemasFromTools`, `tool-schema-helper.ts`).

**Conclusion**: Skills should follow the same pattern. What we need are **pure utility functions** that operate on `SkillRegistry` and `SkillLoader`, not a new service class.

### 3.2 Current Redundancy in SkillLoader

`SkillLoader.generateMetadataPrompt()` already generates the metadata prompt string. The CLI's `SkillAdapter.injectSkillsMetadata()` already does the system prompt injection. The gap is simply that:
1. These are not accessible from `sdk/agent/` or `sdk/workflow/`
2. The `skill` tool creation logic is embedded in `AgentLoopAdapter.registerSkillTool()` rather than being reusable

### 3.3 Design: Minimal Utility Functions in `sdk/core/skill/`

```typescript
// sdk/core/skill/skill-utils.ts
//
// Pure utility functions for integrating skills into LLM execution contexts.
// These operate on SkillRegistry and SkillLoader (state holders) and return
// derived data (prompts, tool definitions).
//
// No state, no class - just functions.

import type { SkillLoader } from "../utils/skill-loader.js";
import type { SkillRegistry } from "../registry/skill-registry.js";
import type { Tool } from "@wf-agent/types";

/**
 * Inject skill metadata into system prompt.
 *
 * - If prompt contains {SKILLS_METADATA}, replace it with metadata
 * - If skills exist and no placeholder, append metadata at the end
 * - If no skills configured, remove placeholder if present
 *
 * Pure function: no side effects, no state.
 */
export function injectSkillsMetadata(
  systemPrompt: string,
  registry: SkillRegistry,
  loader: SkillLoader,
): string {
  const metadataPrompt = loader.generateMetadataPrompt();
  if (!metadataPrompt) {
    return systemPrompt.replace("{SKILLS_METADATA}", "");
  }
  if (systemPrompt.includes("{SKILLS_METADATA}")) {
    return systemPrompt.replace("{SKILLS_METADATA}", metadataPrompt);
  }
  if (registry.getEnabledSkills().length === 0) {
    return systemPrompt;
  }
  return `${systemPrompt}\n\n${metadataPrompt}`;
}

/**
 * Check whether any skills are configured and enabled.
 */
export function isSkillsConfigured(registry: SkillRegistry): boolean {
  return registry.getEnabledSkills().length > 0;
}

/**
 * Create the dynamic `skill` tool definition.
 *
 * This is a factory function: takes SkillLoader as the content source,
 * returns a Tool that can be registered in any ToolRegistry.
 *
 * The tool execution calls SkillLoader.loadContent() to implement
 * progressive disclosure Level 2 (on-demand content loading).
 */
export function createSkillTool(loader: SkillLoader): Tool {
  return {
    id: "skill",
    type: "STATELESS",
    description: buildSkillToolDescription(),
    parameters: skillSchema,
    config: {
      execute: async (params: Record<string, unknown>) => {
        const { skill, args } = params as {
          skill: string;
          args?: Record<string, unknown> | null;
        };
        if (!skill || typeof skill !== "string") {
          return { success: false, error: "Missing 'skill' parameter" };
        }
        const result = await loader.loadContent(skill, {
          context: { variables: args || undefined },
        });
        return {
          success: result.success,
          result: result.content,
          error: result.error?.message,
        };
      },
    },
  };
}
```

**Key design characteristics**:
- No class, no DI needed for the utility functions themselves
- `SkillRegistry` and `SkillLoader` remain the only stateful objects (already in DI container)
- Functions are pure transformations: (state + input) → output
- Callers (agent loop executor, workflow handler) get `SkillRegistry`/`SkillLoader` from DI and call these functions directly

### 3.4 Architecture Diagram (Revised)

```
                    sdk/core/

  ┌──────────────────────────────────────────────────────┐
  │ SkillRegistry (registry/skill-registry.ts)           │
  │   State: skill map, enable/disable status            │
  └──────────────────────┬───────────────────────────────┘
                         │
  ┌──────────────────────▼───────────────────────────────┐
  │ SkillLoader (utils/skill-loader.ts)                   │
  │   State: content cache                                │
  └──────────────────────┬───────────────────────────────┘
                         │
  ┌──────────────────────▼───────────────────────────────┐
  │ skill-utils.ts (NEW - pure functions)                │
  │   injectSkillsMetadata(systemPrompt, registry, loader)│
  │   createSkillTool(loader) → Tool                     │
  │   isSkillsConfigured(registry) → boolean             │
  └──────┬─────────────────────────────────┬─────────────┘
         │                                 │
         ▼                                 ▼
  sdk/agent/                        sdk/workflow/
  AgentLoopExecutor                 AgentLoopHandler
    → calls injectSkillsMetadata()    → calls injectSkillsMetadata()
    → calls createSkillTool()         → calls createSkillTool()
    → calls isSkillsConfigured()      → calls isSkillsConfigured()
```

---

## 4. Agent Adaptation Layer

### 4.1 What Agent Loop Needs

The agent loop (`sdk/agent/`) needs to:
1. **Before execution**: Check if skills are configured; if so, inject metadata into system prompt and register the `skill` tool
2. **Execution**: Nothing special - skill tool works like any other tool

### 4.2 Current CLI Implementation (Reference)

Currently in `apps/cli-app/src/commands/agent/index.ts`:
```typescript
const metadataPrompt = skillAdapter.generateMetadataPrompt();
if (metadataPrompt) {
  config.systemPrompt = skillAdapter.injectSkillsMetadata(config.systemPrompt);
  adapter.registerSkillTool({ loader: { loadContent: ... } });
  config.availableTools.tools.push("skill");
}
```

### 4.3 Target: AgentLoopExecutor Integration

The integration moves from CLI to `AgentLoopExecutor`, using SkillRegistry/SkillLoader from DI plus pure utility functions:

```typescript
// sdk/agent/execution/executors/agent-loop-executor.ts

import { injectSkillsMetadata, createSkillTool, isSkillsConfigured } from "../../core/skill/skill-utils.js";

export class AgentLoopExecutor {
  private skillRegistry?: SkillRegistry;
  private skillLoader?: SkillLoader;

  /** Enable skills integration. Call during construction or before execute(). */
  enableSkills(registry: SkillRegistry, loader: SkillLoader): void {
    this.skillRegistry = registry;
    this.skillLoader = loader;
  }

  async execute(config: AgentLoopRuntimeConfig, options: AgentLoopEntityOptions): Promise<AgentLoopResult> {
    // Step 1: If skills are configured, inject metadata + register tool
    if (this.skillRegistry && this.skillLoader && isSkillsConfigured(this.skillRegistry)) {
      config.systemPrompt = injectSkillsMetadata(
        config.systemPrompt || "",
        this.skillRegistry,
        this.skillLoader,
      );
      const skillTool = createSkillTool(this.skillLoader);
      this.toolService.registerTool(skillTool, { skipIfExists: true });
      if (!config.availableTools) {
        config.availableTools = { tools: ["skill"] };
      } else if (!config.availableTools.tools?.includes("skill")) {
        config.availableTools.tools = [...(config.availableTools.tools || []), "skill"];
      }
    }
    // ... rest of execution
  }
}
```

### 4.4 CLI Adapter Simplification

The `AgentLoopAdapter.registerSkillTool()` method can be deprecated or simplified. The CLI command no longer needs to manually do skill injection - it just passes SkillRegistry/SkillLoader:

```typescript
// apps/cli-app/src/adapters/agent-loop-adapter.ts
const skillRegistry = globalContext.container.get(Identifiers.SkillRegistry);
const skillLoader = globalContext.container.get(Identifiers.SkillLoader);
executor.enableSkills(skillRegistry, skillLoader);
```

The existing `registerSkillTool()` method on the adapter is kept for backward compatibility but internally delegates to the utility functions.

---

## 5. Workflow Adaptation Layer

### 5.1 What Workflow Needs

Workflow has two node types:

**LLM Node**: The `skill` tool needs to be available in the tool registry so LLM can call it. This is best handled at system initialization (register skill tool globally once), not per-node.

**Agent Loop Node**: Same as standalone agent loop - system prompt injection + skill tool registration. Must happen within `AgentLoopHandler` before creating the AgentLoopCoordinator.

### 5.2 Agent Loop Node Integration

```typescript
// sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts

import { injectSkillsMetadata, createSkillTool, isSkillsConfigured } from "../../../core/skill/skill-utils.js";

export async function agentLoopHandler(globalContext, execution, node, context) {
  const config = node.config as AgentLoopNodeConfig;

  // Inject skill metadata into system prompt + register skill tool
  const skillRegistry = globalContext.container.get(Identifiers.SkillRegistry);
  const skillLoader = globalContext.container.get(Identifiers.SkillLoader);
  if (skillRegistry && skillLoader && isSkillsConfigured(skillRegistry)) {
    const systemPrompt = resolveSystemPrompt(config.inlineConfig || {});
    config.inlineConfig.systemPrompt = injectSkillsMetadata(
      systemPrompt, skillRegistry, skillLoader,
    );
    const skillTool = createSkillTool(skillLoader);
    context.toolService.registerTool(skillTool, { skipIfExists: true });
    if (!config.inlineConfig.availableTools) {
      config.inlineConfig.availableTools = { tools: ["skill"] };
    } else if (!config.inlineConfig.availableTools.tools?.includes("skill")) {
      config.inlineConfig.availableTools.tools.push("skill");
    }
  }

  // ... existing logic (create coordinator, execute) ...
}
```

### 5.3 System-Level Skill Tool Registration

To make the `skill` tool available to all LLM nodes (not just Agent Loop nodes), register it once at system startup:

```typescript
// During system initialization (e.g., in GlobalContext setup or DI bootstrap)
import { skillRegistry, skillLoader } from "...";
import { createSkillTool } from "./skill/skill-utils.js";

const skillTool = createSkillTool(skillLoader);
toolRegistry.registerTool(skillTool, { skipIfExists: true });
```

This ensures that any LLM node in any workflow can call the `skill` tool if skills are configured.

### 5.4 Workflow-Level Skill Configuration (Optional)

Workflow templates can optionally declare skill requirements for per-workflow control:

```typescript
// packages/types/src/workflow/config.ts
export interface WorkflowConfig {
  // ...
  skills?: {
    enabled?: boolean;
    include?: string[];
    exclude?: string[];
  };
}
```

---

## 6. Implementation Plan (5 Phases)

### Phase 1: Create Utility Functions (Priority: High)

**Goal**: Add pure utility functions in `sdk/core/skill/skill-utils.ts` that operate on SkillRegistry/SkillLoader.

**Changes**:
- Create `sdk/core/skill/skill-utils.ts` with:
  - `injectSkillsMetadata(systemPrompt, registry, loader): string`
  - `createSkillTool(loader): Tool`
  - `isSkillsConfigured(registry): boolean`
- Export from `sdk/core/index.ts`

**Files**:
- `sdk/core/skill/skill-utils.ts` (NEW)
- `sdk/core/index.ts` (MODIFY)

**Why this location**:
- `sdk/core/skill/` mirrors the existing `sdk/core/coordinators/` pattern
- Close to `SkillRegistry` and `SkillLoader` without adding a new service layer
- No DI bindings needed (pure functions)

### Phase 2: Migrate Agent Loop Integration (Priority: High)

**Goal**: Move skill integration from CLI command into AgentLoopExecutor using the new utility functions.

**Changes**:
- Add `enableSkills(registry, loader)` to `AgentLoopExecutor`
- Call utility functions during `execute()`
- Update CLI adapter to pass SkillRegistry/SkillLoader
- Simplify CLI command to remove manual injection

**Files**:
- `sdk/agent/execution/executors/agent-loop-executor.ts` (MODIFY)
- `apps/cli-app/src/commands/agent/index.ts` (MODIFY)
- `apps/cli-app/src/adapters/agent-loop-adapter.ts` (MODIFY)

### Phase 3: Integrate into Workflow AgentLoopHandler (Priority: High)

**Goal**: Add skill integration to workflow's AgentLoopHandler.

**Changes**:
- In `AgentLoopHandler`, get SkillRegistry/SkillLoader from DI container
- Call utility functions before creating coordinator
- Register skill tool in context's ToolRegistry

**Files**:
- `sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts` (MODIFY)

### Phase 4: System-Level Skill Tool Registration (Priority: Medium)

**Goal**: Register the skill tool globally at startup so all LLM nodes can use it without per-node setup.

**Changes**:
- During system initialization, create and register skill tool in the global ToolRegistry
- Use `createSkillTool()` utility function

**Files**:
- Initialization/bootstrap code (varies by application entry point)

### Phase 5: Cleanup CLI Duplication (Priority: Low)

**Goal**: Remove now-redundant code from CLI layer.

**Changes**:
- Remove or deprecate `SkillAdapter.injectSkillsMetadata()` (moved to core utility)
- Simplify `AgentLoopAdapter.registerSkillTool()` to delegate internally
- Ensure backward compatibility

**Files**:
- `apps/cli-app/src/adapters/skill-adapter.ts` (MODIFY)
- `apps/cli-app/src/adapters/agent-loop-adapter.ts` (MODIFY)

---

## 7. Execution Order

```
Phase 1 (Utility Functions)
  │
  ├──→ Phase 2 (Agent Loop) - depends on Phase 1
  │
  ├──→ Phase 3 (Workflow Agent Loop) - depends on Phase 1
  │
  ├──→ Phase 4 (System Registration) - depends on Phase 1
  │
  └──→ Phase 5 (Cleanup) - depends on Phase 2, 3
```

**Recommended execution**: Phase 1 → Phase 2 → Phase 3. Phases 4 and 5 are lower priority.

---

## 8. Risk Assessment

| Risk | Impact | Probability | Mitigation |
|------|--------|------------|------------|
| Existing CLI integration breaks during migration | Medium | Medium | Keep backward compat in AgentLoopAdapter during transition |
| Workflow handler doesn't have access to SkillRegistry via DI | High | Low | SkillRegistry already has DI binding; just ensure it's resolvable in handler context |
| SkillLoader cache TTL causes stale skill content | Low | Low | Existing cache invalidation mechanism already handles this |
| LLM node calls skill tool but skills not configured | Low | Low | Tool execution returns clear error message |
| Skill metadata duplicates in system prompt | Low | Low | Only inject when placeholder exists or skills are new |

---

## 9. Summary

- **Current state**: Skills only work in CLI agent commands; workflow has no skill support
- **Core problem**: Integration logic is embedded in CLI layer, not reusable
- **Solution**: Create pure utility functions in `sdk/core/skill/skill-utils.ts` that operate on existing `SkillRegistry` and `SkillLoader` state holders
- **Agent integration**: `AgentLoopExecutor` calls utility functions during `execute()`
- **Workflow integration**: `AgentLoopHandler` calls utility functions before creating coordinator
- **No new service classes, no new DI bindings** - just functions + existing stateful objects