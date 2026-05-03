# Schema Consolidation Analysis

## Overview

This document analyzes the current distribution of Zod validation schemas across the monorepo and provides recommendations for consolidation to maintain the "Single Source of Truth" principle.

**Date:** 2026-05-02  
**Status:** Analysis Complete  
**Priority:** High (CLI app refactoring needed)

---

## Current State Analysis

### 1. Schema Distribution by Package

#### ✅ `packages/types` - Primary Schema Location

The `types` package currently serves as the central location for core schemas:

**LLM Schemas:**
- `LLMProviderSchema` - LLM provider enum validation
- `LLMProfileSchema` - Complete LLM profile configuration
- `ToolCallFormatSchema`, `ToolCallFormatConfigSchema` - Tool calling format validation

**Workflow Schemas:**
- `WorkflowDefinitionSchema` - Complete workflow structure validation
- `WorkflowVariableSchema` - Variable definitions
- `CheckpointConfigSchema`, `TriggeredSubworkflowConfigSchema` - Workflow features

**Node Configuration Schemas:**
- Control nodes: `StartNodeConfigSchema`, `EndNodeConfigSchema`, `RouteNodeConfigSchema`
- Execution nodes: `ScriptNodeConfigSchema`, `LLMNodeConfigSchema`
- Context nodes: `ContextProcessorNodeConfigSchema`
- Interaction nodes: `InteractionNodeConfigSchema`, `UserInputNodeConfigSchema`
- Loop nodes: `LoopNodeConfigSchema`, `LoopBreakNodeConfigSchema`
- Fork/Join nodes: `ForkNodeConfigSchema`, `JoinNodeConfigSchema`
- Subgraph nodes: `SubgraphNodeConfigSchema`, `StartFromTriggerNodeConfigSchema`
- Variable nodes: `VariableNodeConfigSchema`

**Tool Schemas:**
- `ToolMetadataSchema` - Tool metadata validation
- `StatelessToolConfigSchema`, `StatefulToolConfigSchema` - Tool configurations
- `RestToolConfigSchema` - REST tool configuration
- `ToolPropertySchema` - JSON Schema parameter validation

**Script Schemas:**
- `ScriptTypeSchema` - Script type validation
- `SandboxConfigSchema` - Sandbox configuration
- `ScriptConfigSchema` - Complete script configuration

**Message Operation Schemas:**
- `AppendMessageOperationSchema`, `TruncateMessageOperationSchema`
- `FilterMessageOperationSchema`, `ClearMessageOperationSchema`
- `ReplaceMessageOperationSchema`, `BatchMessageOperationSchema`

**Configuration Schemas:**
- Storage: `CompressionConfigSchema`, `JsonStorageConfigSchema`, `SqliteStorageConfigSchema`, `StorageConfigSchema`
- Output: `OutputConfigSchema`
- Presets: `ContextCompressionPresetConfigSchema`, `PredefinedToolsPresetConfigSchema`, `PredefinedPromptsPresetConfigSchema`, `PresetsConfigSchema`
- Logging: `LogLevelSchema`, `SDKLogLevelSchema`

**Architecture Rationale:**
- Types and schemas co-exist in the same package
- Ensures synchronization between TypeScript types and runtime validation
- Follows modern TypeScript patterns (similar to Prisma, tRPC)
- Single source of truth for framework-wide contracts

---

#### ⚠️ `packages/prompt-templates` - Separate Schema Location

Currently contains its own schemas:

```typescript
// packages/prompt-templates/src/schema.ts
export const VariableDefinitionSchema: z.ZodType<VariableDefinition>
export const PromptTemplateSchema: z.ZodType<PromptTemplate>
```

**Considerations:**
- The `prompt-templates` package exports both types and schemas
- This maintains package independence (other packages don't need prompt-templates)
- However, it creates inconsistency with the established pattern in `types`

**Options:**
1. **Keep separate** - Maintains modularity, but inconsistent with other domains
2. **Move to types** - Consistent pattern, but creates dependency on prompt-templates

**Recommendation:** Keep separate for now, as prompt templates may be optional in some deployments. Document this as an intentional architectural decision.

---

#### ❌ `apps/cli-app` - Schema Duplication Problem

The CLI app has significant schema duplication with `@wf-agent/types`:

**Duplicated Schemas (9 total):**

| CLI App Schema | Types Package Schema | Status |
|---------------|---------------------|--------|
| `CompressionConfigSchema` | `CompressionConfigSchema` | ❌ Duplicate |
| `JsonStorageConfigSchema` | `JsonStorageConfigSchema` | ❌ Duplicate |
| `SqliteStorageConfigSchema` | `SqliteStorageConfigSchema` | ❌ Duplicate |
| `StorageConfigSchema` | `StorageConfigSchema` | ❌ Duplicate |
| `OutputConfigSchema` | `OutputConfigSchema` | ❌ Duplicate |
| `ContextCompressionPresetConfigSchema` | `ContextCompressionPresetConfigSchema` | ❌ Duplicate |
| `PredefinedToolsPresetConfigSchema` | `PredefinedToolsPresetConfigSchema` | ❌ Duplicate |
| `PredefinedPromptsPresetConfigSchema` | `PredefinedPromptsPresetConfigSchema` | ❌ Duplicate |
| `PresetsConfigSchema` | `PresetsConfigSchema` | ❌ Duplicate |

**CLI-Specific Schemas (OK to keep):**
- `CLIConfigSchema` - Combines shared schemas with CLI-specific fields (apiUrl, apiKey, verbose, debug, etc.)

**File:** `apps/cli-app/src/config/cli/schema.ts` (163 lines)

---

### 2. Other Packages (No Schemas Found)

The following packages correctly do NOT define their own schemas:
- `packages/storage` - Implementation only
- `packages/script-executors` - Implementation only
- `packages/tool-executors` - Implementation only
- `packages/common-utils` - Utilities only

This is the correct pattern - implementation packages should not define configuration schemas.

---

## Problems Identified

### 🔴 Critical Issues

1. **Code Duplication in CLI App**
   - 9 schemas duplicated between CLI app and types package
   - Risk of divergence over time
   - Violates DRY principle
   - Maintenance burden (changes must be made in 2 places)

2. **Inconsistent Validation Behavior**
   - If types package updates a schema, CLI app won't automatically benefit
   - Potential for different validation rules in different parts of the system

3. **Type Safety Gaps**
   - CLI app uses `satisfies z.ZodType<Type>` pattern
   - But the Type comes from `@wf-agent/types`
   - If schema changes but type doesn't (or vice versa), compilation may not catch it

---

### 🟡 Medium Priority Issues

4. **Prompt Templates Package Isolation**
   - Inconsistent with the established pattern
   - May cause confusion for developers
   - Not a critical issue, but worth documenting

---

## Recommendations

### Priority 1: Remove CLI App Schema Duplicates (HIGH)

**Action:** Refactor `apps/cli-app/src/config/cli/schema.ts` to import shared schemas from `@wf-agent/types`.

**Implementation Plan:**

```typescript
// apps/cli-app/src/config/cli/schema.ts (refactored)

import { z } from "zod";
import type { CLIConfig } from "./types.js";

// Import shared schemas from types package
import {
  CompressionConfigSchema,
  JsonStorageConfigSchema,
  SqliteStorageConfigSchema,
  StorageConfigSchema,
  OutputConfigSchema,
  ContextCompressionPresetConfigSchema,
  PredefinedToolsPresetConfigSchema,
  PredefinedPromptsPresetConfigSchema,
  PresetsConfigSchema,
} from "@wf-agent/types";

/**
 * Complete CLI Configuration Schema
 * Combines shared schemas with CLI-specific configuration
 */
export const CLIConfigSchema = z.object({
  // CLI-specific fields
  apiUrl: z.string().url().optional(),
  apiKey: z.string().optional(),
  defaultTimeout: z.number().positive().default(30000),
  verbose: z.boolean().default(false),
  debug: z.boolean().default(false),
  logLevel: z.enum(["error", "warn", "info", "debug"]).default("warn"),
  outputFormat: z.enum(["json", "table", "plain"]).default("table"),
  maxConcurrentExecutions: z.number().positive().default(5),
  
  // Use imported shared schemas
  storage: StorageConfigSchema.optional(),
  output: OutputConfigSchema.optional(),
  presets: PresetsConfigSchema.optional(),
}) satisfies z.ZodType<CLIConfig>;

export type CLIConfigValidated = z.infer<typeof CLIConfigSchema>;
```

**Benefits:**
- ✅ Eliminates ~140 lines of duplicate code
- ✅ Single source of truth for shared configuration
- ✅ Automatic consistency across all apps
- ✅ Easier maintenance and updates
- ✅ Reduced risk of validation divergence

**Estimated Effort:** 1-2 hours
**Risk:** Low (pure refactoring, no behavior change)

---

### Priority 2: Document Prompt Templates Decision (MEDIUM)

**Action:** Add documentation explaining why `PromptTemplateSchema` remains in `prompt-templates` package.

**Documentation Addition:**

Add to `packages/prompt-templates/README.md`:

```markdown
## Architecture Note: Schema Location

The `PromptTemplateSchema` is defined in this package rather than in `@wf-agent/types` 
to maintain package independence. This allows applications to use prompt templates 
without requiring the full types package if they choose a different validation strategy.

This is an intentional architectural decision that differs from other domains (LLM, 
Workflow, Tools) where schemas are centralized in the types package.
```

**Alternative:** If prompt templates become core to all deployments, consider moving both types and schemas to `packages/types` for consistency.

**Estimated Effort:** 30 minutes
**Risk:** None (documentation only)

---

### Priority 3: Establish Schema Guidelines (LOW)

**Action:** Create a guideline document for future schema development.

**Guidelines:**

1. **Core Domain Schemas → `packages/types`**
   - LLM profiles, workflows, tools, scripts, nodes
   - Anything used across multiple apps/packages

2. **Optional Feature Schemas → Feature Package**
   - Prompt templates (if optional)
   - Other pluggable features

3. **App-Specific Schemas → App Directory**
   - CLI configuration (fields unique to CLI)
   - Web app configuration (fields unique to web)
   - Should compose shared schemas from types

4. **Never Duplicate Schemas**
   - Always import from appropriate location
   - Use composition, not duplication

**Estimated Effort:** 1 hour
**Risk:** None (guidelines only)

---

## Implementation Roadmap

### Phase 1: Fix CLI App Duplication (Week 1)

1. Update `apps/cli-app/src/config/cli/schema.ts`
2. Run tests to ensure no regression
3. Verify build succeeds
4. Update any imports in CLI app that reference the removed schemas

**Success Criteria:**
- All CLI app tests pass
- No duplicate schema definitions
- Build succeeds without errors

---

### Phase 2: Documentation (Week 1-2)

1. Add architecture note to prompt-templates README
2. Create schema development guidelines
3. Update AGENTS.md if needed

**Success Criteria:**
- Clear documentation exists
- Future developers understand the pattern

---

### Phase 3: Review Other Apps (Week 2-3)

Check if other apps have similar duplication:
- `apps/web-app-backend`
- `apps/web-app-frontend`
- `apps/vscode-app`

**Success Criteria:**
- No other apps have schema duplication
- Consistent pattern across all apps

---

## Technical Considerations

### Dependency Graph

Current dependencies:
```
apps/cli-app
  └─ @wf-agent/types (already depends on this)
  └─ zod (already depends on this)

packages/types
  └─ zod (runtime dependency)
```

After refactoring, no new dependencies are introduced - we're just using existing ones more effectively.

### Version Compatibility

Both CLI app and types package already use:
- `zod: 4.3.6`

No version conflicts expected.

### Testing Strategy

1. **Unit Tests:** Existing CLI config tests should continue to pass
2. **Integration Tests:** Verify CLI can still load and validate configs
3. **Manual Testing:** Test CLI with various config files

---

## Migration Checklist

For Phase 1 (CLI App Refactoring):

- [ ] Backup current `apps/cli-app/src/config/cli/schema.ts`
- [ ] Replace duplicate schemas with imports from `@wf-agent/types`
- [ ] Update any internal references to removed schemas
- [ ] Run `pnpm build --filter=cli-app`
- [ ] Run `pnpm test --filter=cli-app`
- [ ] Manual test: Load a CLI config file
- [ ] Manual test: Validate config with invalid values
- [ ] Commit changes with clear message
- [ ] Update CHANGELOG if applicable

---

## Benefits Summary

### Immediate Benefits (Phase 1)

1. **Code Reduction:** ~140 fewer lines in CLI app
2. **Consistency:** Guaranteed same validation everywhere
3. **Maintainability:** Single place to update schemas
4. **Type Safety:** Better alignment between types and schemas

### Long-term Benefits

1. **Scalability:** Pattern scales to new apps
2. **Clarity:** Clear ownership of schemas
3. **Flexibility:** Apps can compose shared + specific schemas
4. **Quality:** Reduced risk of bugs from divergent validation

---

## Risks and Mitigation

### Risk 1: Breaking Changes

**Risk:** Types package schema changes might break CLI app unexpectedly.

**Mitigation:**
- Use semantic versioning for types package
- Apps pin to specific versions
- Comprehensive test suite catches issues early

---

### Risk 2: Circular Dependencies

**Risk:** Moving schemas could create circular dependencies.

**Mitigation:**
- Current architecture already avoids this
- Types package has no dependencies on apps
- Verified dependency graph shows no cycles

---

### Risk 3: Performance Impact

**Risk:** Importing from types package might be slower.

**Mitigation:**
- Modern bundlers tree-shake unused exports
- No measurable performance difference expected
- Can verify with bundle analyzer if concerned

---

## Conclusion

The current schema distribution has one critical issue: **significant duplication in the CLI app**. This violates fundamental software engineering principles and creates maintenance burdens.

**Recommended immediate action:** Refactor CLI app to import shared schemas from `@wf-agent/types`, eliminating ~140 lines of duplicate code and ensuring consistency across the codebase.

The prompt-templates package separation is acceptable as an intentional architectural decision for modularity, but should be documented clearly.

Following these recommendations will result in:
- Cleaner, more maintainable code
- Stronger type safety guarantees
- Better scalability for future apps
- Clearer architectural patterns

---

## References

- [Zod Documentation](https://zod.dev/)
- [Monorepo Best Practices](https://monorepo.tools/)
- [Single Source of Truth Principle](https://en.wikipedia.org/wiki/Single_source_of_truth)
- Related: `sdk-api-validator-refactor.md` - Previous validator refactoring work
