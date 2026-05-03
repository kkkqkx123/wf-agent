# Workflow Type Definitions Improvement Analysis

## Overview

After refactoring Agent Loop configuration types, this document analyzes similar improvements that could be applied to Workflow type definitions for better architectural consistency.

---

## Current Architecture Comparison

### Agent Loop (After Refactoring) ✅

```
packages/types/src/agent/
├── config.ts              → AgentLoopConfig (runtime with functions)
└── hooks.ts               → AgentHook (pure data)

sdk/api/shared/config/
├── types.ts               → AgentLoopConfigFile (file-based, no functions)
│                            AgentHookConfigFile (type alias = AgentHook)
│                            AgentTriggerConfigFile (SDK-specific)
└── validators/
    └── agent-loop-schema.ts → Zod schemas for file validation
```

**Key Principles:**
1. **Types package**: Core runtime types only
2. **SDK package**: File format concerns, parsing, transformation
3. **Clear separation**: Functions can't be serialized → separate types

---

### Workflow (Current State) ⚠️

```
packages/types/src/workflow/
├── definition.ts          → WorkflowDefinition/WorkflowTemplate
├── config.ts              → WorkflowConfig, CheckpointConfig, etc.
├── variables.ts           → WorkflowVariable
├── metadata.ts            → WorkflowMetadata
└── workflow-schema.ts     → Zod schemas

sdk/api/shared/config/
├── types.ts               → WorkflowConfigFile = WorkflowDefinition (alias)
└── config-transformer.ts  → Transformation logic
```

**Issues Identified:**

---

## 🔴 Issue 1: WorkflowDefinition Serves Dual Purpose

### Problem

`WorkflowDefinition` is used for BOTH:
- File-based configuration (TOML/JSON)
- Runtime execution structure

```typescript
// packages/types/src/workflow/definition.ts
export interface WorkflowTemplate {
  id: ID;
  name: string;
  nodes: Node[];        // ✅ Serializable
  edges: Edge[];        // ✅ Serializable
  variables?: WorkflowVariable[];  // ✅ Serializable
  triggers?: (WorkflowTrigger | TriggerReference)[];  // ⚠️ Mixed!
  availableTools?: {
    initial: Set<string>;  // ❌ Set is not JSON-serializable!
  };
}
```

**Problems:**
1. `Set<string>` cannot be directly serialized to JSON/TOML
2. `TriggerReference` may contain runtime-specific data
3. No clear boundary between "what's in the file" vs "what's added at runtime"

---

## 🔴 Issue 2: Node and Edge Types Have Same Problem

### Current State

```typescript
// packages/types/src/node/base.ts
export interface Node {
  id: ID;
  type: NodeType;
  config: NodeConfig;
  name?: string;
  incomingEdgeIds?: string[];   // ⚠️ Added during preprocessing
  outgoingEdgeIds?: string[];   // ⚠️ Added during preprocessing
  position?: { x: number; y: number };  // ⚠️ UI-only field
}

// packages/types/src/edge.ts
export interface Edge {
  id: ID;
  sourceNodeId: ID;
  targetNodeId: ID;
  type: EdgeType;
  condition?: EdgeCondition;  // ✅ Serializable
  metadata?: EdgeMetadata;    // ✅ Serializable
}
```

**Problem:** 
- `incomingEdgeIds` and `outgoingEdgeIds` are computed during graph preprocessing
- They shouldn't be in the file config but are part of the same type
- `position` is UI-specific, not relevant for execution

---

## 🔴 Issue 3: Triggers Mix File and Runtime Concepts

```typescript
// packages/types/src/trigger/index.ts
export interface WorkflowTrigger {
  id: string;
  type: TriggerType;
  config: TriggerConfig;
  enabled?: boolean;
}

export interface TriggerReference {
  templateName: string;  // References a template
  configOverride?: Partial<TriggerConfig>;
}
```

**Problem:**
- `TriggerReference` is a file-time concept (template reference)
- At runtime, it should be resolved to actual `WorkflowTrigger`
- But both exist in the same `triggers` array in `WorkflowDefinition`

---

## ✅ Recommended Improvements

### Solution A: Minimal Changes (Recommended for Now)

Keep current structure but improve documentation and add type guards:

#### 1. Clarify Serialization Boundaries

```typescript
// packages/types/src/workflow/definition.ts

/**
 * Workflow Template (File-Based Configuration)
 * 
 * This interface represents the workflow structure as defined in configuration files.
 * All fields must be JSON/TOML serializable.
 * 
 * Note: Some fields are transformed during preprocessing:
 * - triggers: TriggerReference[] → resolved to WorkflowTrigger[]
 * - availableTools.initial: Set<string> → converted from array in files
 */
export interface WorkflowTemplate {
  id: ID;
  name: string;
  type: WorkflowTemplateType;
  description?: string;
  nodes: Node[];
  edges: Edge[];
  variables?: WorkflowVariable[];
  
  /**
   * Triggers can be either:
   * - WorkflowTrigger: Fully defined trigger
   * - TriggerReference: Reference to a template (resolved at load time)
   */
  triggers?: (WorkflowTrigger | TriggerReference)[];
  
  triggeredSubworkflowConfig?: TriggeredSubworkflowConfig;
  config?: WorkflowConfig;
  metadata?: WorkflowMetadata;
  version: Version;
  createdAt: Timestamp;
  updatedAt: Timestamp;
  
  /**
   * Available tools
   * Note: In config files, this should be an array. It's converted to Set at runtime.
   */
  availableTools?: {
    initial: Set<string>;  // TODO: Consider using string[] for better serialization
  };
}
```

#### 2. Add Preprocessing Annotations to Node

```typescript
// packages/types/src/node/base.ts

export interface Node {
  id: ID;
  type: NodeType;
  config: NodeConfig;
  name?: string;
  
  /**
   * @internal Computed during graph preprocessing
   * Not present in configuration files
   */
  incomingEdgeIds?: string[];
  
  /**
   * @internal Computed during graph preprocessing
   * Not present in configuration files
   */
  outgoingEdgeIds?: string[];
  
  /**
   * @ui-only Position information for visual editors
   * Ignored during execution
   */
  position?: { x: number; y: number };
}
```

#### 3. Document SDK Config File Pattern

```typescript
// sdk/api/shared/config/types.ts

/**
 * Workflow Configuration File Format
 * 
 * This is currently just a type alias to WorkflowDefinition.
 * In the future, consider separating file-specific fields from runtime fields.
 * 
 * Key differences from runtime usage:
 * - availableTools.initial should be string[] in files (converted to Set at runtime)
 * - triggers with TriggerReference are resolved to WorkflowTrigger at load time
 * - Node.incomingEdgeIds/outgoingEdgeIds are computed, not in files
 */
export type WorkflowConfigFile = WorkflowDefinition;
```

---

### Solution B: Full Separation (Future Enhancement)

If Workflow complexity grows, consider full separation like Agent Loop:

```typescript
// packages/types/src/workflow/definition.ts
export interface WorkflowDefinition {
  id: ID;
  name: string;
  nodes: WorkflowNode[];      // Simplified node for runtime
  edges: WorkflowEdge[];      // Simplified edge for runtime
  variables?: WorkflowVariable[];
  triggers?: WorkflowTrigger[];  // Only resolved triggers
  config?: WorkflowConfig;
  // NO UI fields, NO preprocessing fields
}

// sdk/api/shared/config/types.ts
export interface WorkflowConfigFile {
  id: ID;
  name: string;
  nodes: NodeConfigFile[];    // File format nodes
  edges: EdgeConfigFile[];    // File format edges
  variables?: WorkflowVariable[];
  triggers?: (WorkflowTrigger | TriggerReference)[];  // Can have references
  availableTools?: {
    initial: string[];  // Array in files, not Set
  };
  // CAN have UI fields like position
}
```

**Pros:**
- Clear separation of concerns
- Better type safety
- Easier to validate file formats

**Cons:**
- Breaking change
- More complex transformation logic
- May be overkill if Workflow doesn't need runtime functions

---

## 🎯 Immediate Action Items

### Priority 1: Documentation ✅ LOW EFFORT

1. Add JSDoc comments to clarify which fields are:
   - File-only
   - Runtime-only
   - Computed during preprocessing
   - UI-specific

2. Update `WorkflowConfigFile` type alias with explanatory comment

3. Document serialization requirements (no Sets, Maps, etc. in files)

### Priority 2: Type Safety Improvements ⚡ MEDIUM EFFORT

1. Change `availableTools.initial` from `Set<string>` to `string[]`:
   ```typescript
   export interface WorkflowTemplate {
     availableTools?: {
       initial: string[];  // Array for serialization
     };
   }
   
   // Runtime conversion in SDK
   const runtimeTools = new Set(template.availableTools?.initial || []);
   ```

2. Add `@internal` tags to preprocessing-only fields

3. Create type guards for file vs runtime validation

### Priority 3: Schema Validation 🔧 FUTURE

1. Move Zod schemas from `packages/types` to `sdk/api/shared/config/validators`
2. Create separate schemas for:
   - `WorkflowConfigFileSchema` (file validation)
   - `WorkflowDefinitionSchema` (runtime validation)
3. Add transformation logic similar to Agent Loop

---

## Comparison Summary

| Aspect | Agent Loop (After) | Workflow (Current) | Recommendation |
|--------|-------------------|-------------------|----------------|
| Type Separation | ✅ Two types | ⚠️ One dual-purpose type | Add docs first |
| Function Support | ✅ Separate runtime type | N/A (no functions) | Keep as-is |
| Serialization | ✅ Clear boundaries | ⚠️ Mixed (Set, refs) | Fix Set issue |
| Schema Location | ✅ SDK-only | ⚠️ In types package | Move to SDK |
| Transformation | ✅ Explicit function | ⚠️ Implicit in builder | Document clearly |

---

## Conclusion

**For Workflow, the recommended approach is:**

1. **Short-term**: Improve documentation and fix serialization issues (Set → Array)
2. **Medium-term**: Add type annotations (@internal, @ui-only) for clarity
3. **Long-term**: If Workflow gains runtime-specific features (like functions), consider full separation

**Why not full separation now?**
- Workflow doesn't have runtime functions (unlike Agent Loop's `transformContext`)
- Current pattern works adequately
- Full separation would be a breaking change with limited immediate benefit
- Better to wait and see if complexity grows

**Key Lesson from Agent Loop Refactoring:**
The separation is driven by **technical necessity** (functions can't serialize), not architectural purity. Apply the same principle to Workflow: separate only when there's a concrete technical reason.
