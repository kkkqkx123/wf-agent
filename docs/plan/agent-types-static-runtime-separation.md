# Agent Type Static and Runtime Separation Plan

## 1. Current Status Analysis

### 1.1 Existing Agent Type Definition Structure

| Type | Location | Purpose | Serializable |
|------|----------|---------|--------------|
| `AgentLoopRuntimeConfig` | packages/types | Runtime config (with functions) | No |
| `AgentLoopDefinition` | packages/types | Static definition | Yes |
| `AgentHook` | packages/types | Hook definition (runtime) | Partial |
| `AgentHookStatic` | packages/types | Hook definition (static) | Yes |
| `AgentLoopState` | SDK internal | Runtime state | Yes |
| `AgentLoopEntity` | SDK internal | Execution instance | No |

### 1.2 Completed Refactoring

✅ **Phase 1-7 Completed**: Static and runtime separation has been implemented.

## 2. Directory Structure Analysis

### 2.1 Workflow Pattern (Reference)

```
packages/types/src/
├── workflow/                    # Static definition (template)
│   ├── definition.ts           # WorkflowTemplate
│   ├── config.ts               # WorkflowConfig (behavior)
│   ├── type.ts                 # WorkflowTemplateType
│   ├── variables.ts            # WorkflowVariable
│   ├── metadata.ts             # WorkflowMetadata
│   └── ...
│
└── workflow-execution/          # Runtime execution
    ├── definition.ts           # WorkflowExecution (data object)
    ├── execution.ts            # WorkflowExecutionOptions, Result
    ├── status.ts               # WorkflowExecutionStatus
    ├── context.ts              # Execution context
    ├── variables.ts            # VariableScopes
    ├── history.ts              # NodeExecutionResult
    ├── scopes.ts               # Scope definitions
    └── signal/                 # Interruption signals
```

**Key Design Principles:**
1. **workflow/**: Static template definition, fully serializable
2. **workflow-execution/**: Runtime execution data, contains execution state
3. **WorkflowExecution**: Pure data object (no methods), serializable
4. **WorkflowExecutionEntity**: SDK internal, wraps data + methods

### 2.2 Current Agent Structure

```
packages/types/src/agent/
├── definition.ts               # AgentLoopDefinition (static)
├── static-config.ts            # AgentHookStatic, AgentTriggerStatic
├── runtime-config.ts           # AgentLoopRuntimeConfig (runtime)
├── hooks.ts                    # AgentHook (runtime)
├── status.ts                   # AgentLoopStatus
├── event.ts                    # AgentStreamEvent types
├── result.ts                   # AgentLoopResult
└── records.ts                  # ToolCallRecord, IterationRecord
```

### 2.3 Structure Comparison

| Aspect | Workflow | Agent (Current) | Agent (Proposed) |
|--------|----------|-----------------|------------------|
| Static Definition | `workflow/` | `agent/` | `agent/` |
| Runtime Execution | `workflow-execution/` | Mixed in `agent/` | `agent-execution/` |
| Data Object | `WorkflowExecution` | None (only State in SDK) | `AgentLoopExecution` |
| Execution Options | `WorkflowExecutionOptions` | Mixed in RuntimeConfig | `AgentLoopExecutionOptions` |
| Execution Result | `WorkflowExecutionResult` | `AgentLoopResult` | `AgentLoopResult` |

## 3. Proposed Directory Structure Improvement

### 3.1 Option A: Create agent-execution Directory (Recommended)

```
packages/types/src/
├── agent/                       # Static definition
│   ├── definition.ts           # AgentLoopDefinition
│   ├── static-config.ts        # AgentHookStatic, AgentTriggerStatic
│   ├── metadata.ts             # AgentLoopMetadata (NEW)
│   └── index.ts
│
└── agent-execution/             # Runtime execution (NEW)
    ├── definition.ts           # AgentLoopExecution (data object)
    ├── execution.ts            # AgentLoopExecutionOptions, AgentLoopResult
    ├── runtime-config.ts       # AgentLoopRuntimeConfig
    ├── status.ts               # AgentLoopStatus
    ├── context.ts              # AgentLoopContext (if needed)
    ├── hooks.ts                # AgentHook (runtime)
    ├── event.ts                # AgentStreamEvent types
    ├── records.ts              # ToolCallRecord, IterationRecord
    └── index.ts
```

**Benefits:**
1. Consistent with workflow/workflow-execution pattern
2. Clear separation of static and runtime concerns
3. Easier to locate types by purpose
4. Better scalability for future additions

### 3.2 Option B: Keep Single Directory with Clear Separation

```
packages/types/src/agent/
├── definition.ts               # AgentLoopDefinition (static)
├── static-config.ts            # Static components
├── metadata.ts                 # AgentLoopMetadata
├── runtime-config.ts           # AgentLoopRuntimeConfig
├── execution.ts                # AgentLoopExecutionOptions, AgentLoopResult
├── hooks.ts                    # AgentHook (runtime)
├── status.ts                   # AgentLoopStatus
├── event.ts                    # Event types
├── records.ts                  # Execution records
└── index.ts
```

**Benefits:**
1. Simpler structure, fewer directories
2. All agent types in one place
3. Easier to import

**Drawbacks:**
1. Less consistent with workflow pattern
2. Mixed concerns in single directory

## 4. Recommended Implementation

### 4.1 Phase 8: Create agent-execution Directory (Optional)

If following the workflow pattern strictly:

1. Create `packages/types/src/agent-execution/` directory
2. Move runtime-related types:
   - `runtime-config.ts` → `agent-execution/`
   - `status.ts` → `agent-execution/`
   - `event.ts` → `agent-execution/`
   - `result.ts` → `agent-execution/`
   - `records.ts` → `agent-execution/`
   - `hooks.ts` → `agent-execution/`
3. Keep static types in `agent/`:
   - `definition.ts`
   - `static-config.ts`
   - `metadata.ts` (new)

### 4.2 Phase 9: Add Missing Types

1. **AgentLoopExecution** (data object, similar to WorkflowExecution):
   ```typescript
   interface AgentLoopExecution {
     id: ID;
     definitionId: ID;
     status: AgentLoopStatus;
     currentIteration: number;
     toolCallCount: number;
     iterationHistory: IterationRecord[];
     startTime: Timestamp;
     endTime?: Timestamp;
     error?: unknown;
   }
   ```

2. **AgentLoopExecutionOptions** (separate from RuntimeConfig):
   ```typescript
   interface AgentLoopExecutionOptions {
     initialMessages?: Message[];
     initialVariables?: Record<string, unknown>;
     parentExecutionId?: ID;
     nodeId?: ID;
     stream?: boolean;
   }
   ```

## 5. Implementation Status

| Phase | Task | Status |
|-------|------|--------|
| 1 | Create `AgentLoopDefinition` and `AgentHookStatic` | ✅ Completed |
| 2 | Create `static-config.ts` for static components | ✅ Completed |
| 3 | Rename `AgentLoopConfig` to `AgentLoopRuntimeConfig` | ✅ Completed |
| 4 | Update `agent/index.ts` exports | ✅ Completed |
| 5 | Update SDK references to use new types | ✅ Completed |
| 6 | Update SDK `AgentLoopConfigFile` to use `AgentLoopDefinition` | ✅ Completed |
| 7 | Run build to verify changes | ✅ Completed |
| 8 | Create `agent-execution` directory (optional) | ⏳ Pending |
| 9 | Add `AgentLoopExecution` data object (optional) | ⏳ Pending |

## 6. Decision Required

**Question**: Should we create `agent-execution` directory to match workflow pattern?

**Arguments for:**
- Consistency with workflow/workflow-execution pattern
- Clear separation of static and runtime concerns
- Better organization for future growth

**Arguments against:**
- Agent Loop is simpler than Workflow (linear vs graph)
- Current structure is already functional
- Additional migration effort required

**Recommendation**: Keep current single-directory structure for now, but ensure clear file naming and documentation. Consider `agent-execution` directory if/when Agent Loop complexity increases.
