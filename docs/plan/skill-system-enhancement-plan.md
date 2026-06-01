# Skill System Enhancement Plan

## Background Analysis

The current skill system has complete core infrastructure (SkillRegistry, SkillLoader, SkillRegistryAPI, DI bindings, types, prompt templates), but has the following gaps:

1. **`skill` tool handler is a stub** - Returns placeholder text instead of actually loading skill content via SkillLoader
2. **Agent loop skills integration missing** - `injectSkillsMetadata` exists but is not called in agent flow
3. **Runtime skill enable/disable management missing** - No mechanism for dynamic state control
4. **Cache inconsistency** - SkillRegistry and SkillLoader have separate caches without cross-invalidation
5. **Variable substitution in SkillLoader** - `buildContext` creates context but never performs variable substitution
6. **`buildContext` semantic issue** - Always sets tools from `skill.metadata.allowedTools` which is semantically incorrect for a general context

## Implementation Plan (6 Phases)

### Phase 1: Implement `skill` Tool Handler (Priority: High)

**Problem**: The `skill` tool handler at `sdk/resources/predefined/tools/stateless/interaction/skill/handler.ts` is a stub that returns a placeholder message. It doesn't actually load skill content from SkillLoader.

**Changes**:
- Inject SkillLoader dependency into the handler via a factory pattern
- Update handler to call `SkillLoader.loadContent()` and return actual skill content
- Pass skill args as variables for template substitution

**Files to modify**:
- `sdk/resources/predefined/tools/stateless/interaction/skill/handler.ts`
- `sdk/resources/predefined/tools/stateless/interaction/skill/schema.ts` (add `args` as object for variable passing)
- `sdk/resources/predefined/tools/stateless/interaction/skill/description.ts` (update description)
- `sdk/resources/predefined/tools/registry.ts` (pass SkillLoader to handler factory)

---

### Phase 2: Integrate Skills into Agent Loop (Priority: High)

**Problem**: Skill metadata is not injected into the agent loop system prompt, and `get_skill` tool is not automatically available when skills exist.

**Changes**:
- Auto-inject skill metadata into system prompt during AgentLoopEntity creation
- Automatically add `skill` tool to available tools when skills are configured
- Integrate `injectSkillsMetadata` into the system prompt builder flow

**Files to modify**:
- `sdk/agent/execution/executors/agent-loop-executor.ts` (or equivalent entry point)
- `apps/cli-app/src/adapters/skill-adapter.ts` (ensure `injectSkillsMetadata` is called)
- `apps/cli-app/src/commands/agent/index.ts` (add skill metadata injection before execution)

---

### Phase 3: Runtime Skill Enable/Disable Management (Priority: Medium)

**Problem**: No API to dynamically enable/disable skills during agent execution. Users cannot control which skills are active.

**Changes**:
- Add `enabled` field to SkillMetadata type
- Add enable/disable methods to SkillRegistry
- Add filter in SkillLoader to skip disabled skills
- Expose enable/disable via SkillRegistryAPI
- Add CLI commands for skill enable/disable

**Files to modify**:
- `packages/types/src/skill.ts` (add `enabled` field)
- `sdk/core/registry/skill-registry.ts` (add enable/disable methods)
- `sdk/core/utils/skill-loader.ts` (filter disabled skills)
- `sdk/api/shared/resources/skills/skill-registry-api.ts` (expose enable/disable)
- `apps/cli-app/src/adapters/skill-adapter.ts` (add CLI methods)
- `apps/cli-app/src/commands/skill/index.ts` (add CLI commands)

---

### Phase 4: Fix Cache Inconsistency (Priority: Medium)

**Problem**: SkillRegistry maintains its own contentCache/resourceCache, while SkillLoader maintains its own contentCache (CheckpointStore). When SkillRegistry's cache is cleared, SkillLoader's cache becomes stale.

**Changes**:
- Add cache invalidation event so SkillLoader listens to SkillRegistry cache changes
- Or: Unify cache management so SkillLoader becomes the single source of truth for content caching
- Provide `clearCache` propagation between the two

**Files to modify**:
- `sdk/core/utils/skill-loader.ts` (add cache invalidation listener or delegate caching)
- `sdk/core/registry/skill-registry.ts` (emit events on cache clear)

---

### Phase 5: Variable Substitution in SkillLoader (Priority: Medium)

**Problem**: `SkillLoader.buildContext()` creates a context with empty `variables: {}` but never performs actual variable substitution in skill content.

**Changes**:
- Implement variable substitution in `loadContent()` (e.g., replace `{{variableName}}` with values from context)
- Update `buildContext()` to properly populate variables
- Add variable extraction from skill content

**Files to modify**:
- `sdk/core/utils/skill-loader.ts` (add substitution logic)
- `packages/types/src/skill.ts` (update context type if needed)

---

### Phase 6: Fix `buildContext` Semantic & Populate Resource Fields (Priority: Medium)

**Problem**: `buildContext()` has two issues:
1. Sets `tools` to `skill.metadata.allowedTools` instead of available runtime tools
2. Skill resource fields (`references`, `examples`, `scripts`, `assets`) are not populated during scanning

**Changes**:
- Fix `buildContext` to accept runtime tools as parameter
- Auto-discover resource directories during skill scanning and populate resource fields in Skill type

**Files to modify**:
- `sdk/core/utils/skill-loader.ts` (fix `buildContext` signature and logic)
- `sdk/core/registry/skill-registry.ts` (auto-discover resources during `loadSkill`)
- `packages/types/src/skill.ts` (update resource field types if needed)

---

## Execution Order

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4 ──→ Phase 5 ──→ Phase 6
(high)      (high)      (medium)    (medium)    (medium)    (medium)
```

Phases 1-2 are the critical path for basic functionality. Phases 3-6 are enhancements that build on the core.