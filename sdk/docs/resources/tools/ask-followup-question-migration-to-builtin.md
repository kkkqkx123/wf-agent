# Ask Follow-up Question Tool - Migration to Builtin

## Migration Summary

Successfully migrated `ask_followup_question` tool from `stateless/interaction` to `builtin/interaction` directory.

## Rationale

**Core Decision Criterion**: Whether the tool requires execution context.

- **Stateless tools**: Do NOT need SDK internal context (EventRegistry, executionId, etc.)
- **Builtin tools**: DO need SDK internal context for proper operation

Since `ask_followup_question` requires:
- `EventRegistry` for emitting interaction events
- `executionId` for tracking
- `nodeId` for context
- Access to execution entity

It is **completely reasonable** to classify it as a builtin tool.

## Changes Made

### 1. Directory Structure

**Before**:
```
sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/
├── schema.ts
├── description.ts
├── handler.ts
└── index.ts
```

**After**:
```
sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/
├── schema.ts
├── description.ts
├── handler.ts
└── index.ts
```

### 2. Handler Updates

**File**: `sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/handler.ts`

**Changes**:
- Changed parameter type from custom `InteractiveToolContext` to `BuiltinToolExecutionContext`
- Removed custom interface definition
- Updated context access pattern:
  ```typescript
  // Before
  context?: InteractiveToolContext
  
  // After
  context: BuiltinToolExecutionContext
  ```
- Extracted eventManager with type casting:
  ```typescript
  const eventManager = context.eventManager as EventRegistry | undefined;
  ```
- Extracted nodeId from parentExecutionEntity:
  ```typescript
  let nodeId: string | undefined;
  if (context.parentExecutionEntity) {
    const entity = context.parentExecutionEntity as { getCurrentNodeId?: () => string };
    nodeId = entity.getCurrentNodeId?.();
  }
  ```

### 3. Type Definition Updates

**File**: `sdk/resources/predefined/tools/builtin/types.ts`

Added new category:
```typescript
export type BuiltinToolCategory =
  | "workflow"      // Workflow execution tools
  | "agent"         // Agent interaction tools
  | "interaction";  // User interaction tools (NEW)
```

### 4. Registry Updates

**File**: `sdk/resources/predefined/tools/builtin/registry.ts`

Added registration:
```typescript
// ask_followup_question
if (!isDisabled("ask_followup_question", options)) {
  tools.push({
    id: "ask_followup_question",
    type: "BUILTIN",
    description: renderToolDescription(ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION),
    parameters: askFollowupQuestionSchema,
    config: {
      execute: createAskFollowupQuestionHandler(),
    },
    metadata: {
      category: "interaction",
      requiresUserInteraction: true,
      interactionType: "ASK_FOLLOWUP_QUESTION",
    },
  });
}
```

### 5. Export Chain Updates

**Files Modified**:
- `sdk/resources/predefined/tools/builtin/index.ts` - Added interaction exports
- `sdk/resources/predefined/tools/builtin/interaction/index.ts` - Created new file
- `sdk/resources/predefined/tools/stateless/interaction/index.ts` - Removed ask-followup-question export
- `sdk/resources/predefined/tools/tool-descriptions.ts` - Removed stateless references

### 6. Cleanup

**Deleted**:
- `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/` (entire directory)

## Benefits of Migration

### 1. **Architectural Clarity**
- Clear separation: tools needing context → builtin, tools not needing context → stateless
- Follows existing pattern (execute_workflow, call_agent, etc.)

### 2. **Simplified Implementation**
- No need to modify IToolExecutor interface
- No need to update all executor implementations
- BuiltinExecutor already supports context passing

### 3. **Type Safety**
- Uses standard `BuiltinToolExecutionContext` type
- Consistent with other builtin tools
- Better IDE support and type checking

### 4. **Future Extensibility**
- Easy to add more interactive tools to `builtin/interaction/`
- Clear categorization within builtin tools
- Consistent registration pattern

## Comparison: Before vs After

| Aspect | Before (Stateless) | After (Builtin) |
|--------|-------------------|-----------------|
| **Location** | `stateless/interaction/` | `builtin/interaction/` |
| **Tool Type** | `STATELESS` | `BUILTIN` |
| **Context Access** | Custom interface | Standard `BuiltinToolExecutionContext` |
| **Executor** | StatelessExecutor | BuiltinExecutor |
| **Config Type** | `StatelessToolConfig` | `BuiltinToolConfig` |
| **Handler Signature** | `(params, context?)` | `(params, context)` |
| **Context Required** | Optional | Required |
| **Architecture Fit** | ❌ Misaligned | ✅ Correct |

## Testing

✅ Compilation successful
✅ No TypeScript errors
✅ All imports resolved correctly
✅ Tool registered in builtin registry

## Migration Checklist

- [x] Create `builtin/interaction/` directory structure
- [x] Copy files to new location
- [x] Update handler to use `BuiltinToolExecutionContext`
- [x] Add "interaction" to `BuiltinToolCategory`
- [x] Register tool in `builtin/registry.ts`
- [x] Update export chains
- [x] Remove from stateless registry
- [x] Delete old directory
- [x] Fix compilation errors
- [x] Verify build success

## Notes

### Context Extraction Pattern

The migration demonstrates how to extract information from `BuiltinToolExecutionContext`:

```typescript
// EventManager (typed as unknown in base interface)
const eventManager = context.eventManager as EventRegistry | undefined;

// Execution ID (directly available)
const executionId = context.executionId;

// Node ID (extracted from parent entity)
let nodeId: string | undefined;
if (context.parentExecutionEntity) {
  const entity = context.parentExecutionEntity as { getCurrentNodeId?: () => string };
  nodeId = entity.getCurrentNodeId?.();
}
```

This pattern can be reused for other interactive tools.

### Timeout Configuration

Currently hardcoded to 5 minutes (300000ms). Future enhancement could add timeout configuration via:
- Tool metadata
- Execution options
- Global configuration

## Conclusion

The migration successfully aligns `ask_followup_question` with the architectural principle:

> **"Tools that require SDK internal execution context should be builtin tools."**

This provides:
- ✅ Clearer architecture
- ✅ Simpler implementation
- ✅ Better type safety
- ✅ Easier maintenance
- ✅ Consistent patterns

The tool is now properly categorized and ready for Phase 2 integration (ToolCallExecutor updates).
