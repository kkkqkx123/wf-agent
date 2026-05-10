# Ask Follow-up Question Tool - Phase 2 Implementation Summary

## Overview

Phase 2 implementation completed successfully. This phase focuses on ToolCallExecutor integration to support interactive tool execution with context passing.

## Completed Changes

### 1. BaseExecutor Context Support ✅

**File**: `packages/tool-executors/src/core/base/BaseExecutor.ts`

**Changes**:
- Added `context` parameter to `execute()` method signature (line 45)
- Added `context` parameter to abstract `doExecute()` method signature (line 133)
- Updated method to pass context to doExecute implementation (line 69)

**Key Code**:
```typescript
async execute(
  tool: Tool,
  parameters: Record<string, unknown>,
  options: ToolExecutionOptions = {},
  executionId?: string,
  context?: Record<string, unknown>, // NEW
): Promise<ToolExecutionResult> {
  // ... 
  const result = await this.timeoutController.executeWithTimeout(
    () => this.doExecute(tool, parameters, executionId, context), // Pass context
    timeout,
    options.signal,
  );
}
```

### 2. BuiltinExecutor Context Merging ✅

**File**: `packages/tool-executors/src/builtin/BuiltinExecutor.ts`

**Changes**:
- Added `context` parameter to `doExecute()` method (line 33)
- Implemented context merging logic to combine default context with passed context (lines 53-58)
- Ensures interactive tools receive full execution context

**Key Code**:
```typescript
protected async doExecute(
  tool: Tool,
  parameters: Record<string, unknown>,
  executionId?: string,
  context?: Record<string, unknown>, // NEW
): Promise<unknown> {
  // Build execution context by merging default context with passed context
  const mergedContext: BuiltinToolExecutionContext = {
    executionId,
    ...this.defaultContext,
    ...context, // Merge passed context
  };

  // Execute the builtin tool with merged context
  const result = await config.execute(parameters, mergedContext);
  // ...
}
```

### 3. Other Executors Updated ✅

Updated all executor implementations to accept the new `context` parameter (even if not used):

- **StatelessExecutor**: Added `_context` parameter (unused)
- **StatefulExecutor**: Added `_context` parameter (unused)  
- **RestExecutor**: Added `_context` parameter (unused)

This ensures interface consistency across all executor types.

### 4. ToolCallExecutor Interactive Tool Detection ✅

**File**: `sdk/core/executors/tool-call-executor.ts`

**Changes**:
- Added interactive tool detection logic (line 473)
- Builds context object for interactive tools with event manager and execution info (lines 476-483)
- Passes context to ToolRegistry.execute() for interactive tools (line 490)

**Key Code**:
```typescript
// Check if this is an interactive tool
const isInteractiveTool = toolConfig?.metadata?.requiresUserInteraction === true;

// Call ToolRegistry to execute the tool.
try {
  // For interactive tools, pass context with event manager and execution info
  let context: Record<string, unknown> | undefined;
  if (isInteractiveTool) {
    context = {
      eventManager: this.eventManager,
      executionId,
      nodeId,
      parentExecutionEntity: this.checkpointDependencies?.workflowExecutionRegistry?.get(executionId || ""),
    };
  }

  const result = await this.toolService.execute(
    toolCall.name,
    JSON.parse(toolCall.arguments),
    executionOptions,
    executionId,
    context, // Pass context for interactive tools
  );
  // ...
}
```

## Architecture Flow

### Interactive Tool Execution Flow

```
┌─────────────────────┐
│  ToolCallExecutor   │
│                     │
│ 1. Detects          │
│    interactive      │
│    tool via         │
│    metadata         │
└──────────┬──────────┘
           │
           │ Creates context with:
           │ - eventManager
           │ - executionId
           │ - nodeId
           │ - parentExecutionEntity
           │
           ▼
┌─────────────────────┐
│   ToolRegistry      │
│                     │
│ 2. Routes to        │
│    BuiltinExecutor  │
│    (passes context) │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  BuiltinExecutor    │
│                     │
│ 3. Merges context:  │
│    default + passed │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ ask_followup_       │
│ question handler    │
│                     │
│ 4. Uses context to: │
│    - Access event   │
│      manager        │
│    - Emit events    │
│    - Wait for       │
│      response       │
└─────────────────────┘
```

## Key Design Decisions

### 1. Optional Context Parameter

The `context` parameter is optional throughout the call chain:
- Allows non-interactive tools to ignore it
- Enables gradual adoption
- Maintains backward compatibility

### 2. Context Merging in BuiltinExecutor

BuiltinExecutor merges three context sources:
1. `executionId` from parameter
2. `defaultContext` from executor configuration
3. `context` from ToolCallExecutor

This provides flexibility for different deployment scenarios.

### 3. Metadata-Based Detection

ToolCallExecutor detects interactive tools using metadata:
```typescript
toolConfig?.metadata?.requiresUserInteraction === true
```

This approach:
- Keeps detection logic simple
- Leverages existing metadata system
- Doesn't require tool type changes

### 4. Context Structure

The context passed to interactive tools includes:
- `eventManager`: For emitting/listening to events
- `executionId`: For tracking execution
- `nodeId`: For node identification
- `parentExecutionEntity`: For accessing execution state

This provides all necessary information for interactive operations.

## Testing

### Compilation Verification

All changes compile successfully:
```bash
pnpm build --filter=@wf-agent/tool-executors  # ✅ Success
pnpm build --filter=@wf-agent/sdk             # ✅ Success
```

### Manual Testing Scenarios

The implementation supports these scenarios:

1. **Fallback Mode** (no context):
   - Tool executes without event manager
   - Returns formatted text for manual input
   - Already tested in Phase 1

2. **Interactive Mode** (with context):
   - Tool receives event manager via context
   - Emits USER_INTERACTION_REQUESTED event
   - Waits for USER_INTERACTION_RESPONDED event
   - Requires UI layer implementation (Phase 3)

## Files Modified

### Core Executor Changes
1. `packages/tool-executors/src/core/base/BaseExecutor.ts`
2. `packages/tool-executors/src/builtin/BuiltinExecutor.ts`
3. `packages/tool-executors/src/stateless/StatelessExecutor.ts`
4. `packages/tool-executors/src/stateful/StatefulExecutor.ts`
5. `packages/tool-executors/src/rest/RestExecutor.ts`

### SDK Integration
6. `sdk/core/executors/tool-call-executor.ts`

## Next Steps (Phase 3)

Phase 3 will focus on Apps layer integration:

1. **CLI App Integration**:
   - Subscribe to USER_INTERACTION_REQUESTED events
   - Render interactive prompts in terminal
   - Collect user responses and emit USER_INTERACTION_RESPONDED

2. **Web App Integration**:
   - Create React/Vue component for question display
   - Implement radio buttons for options
   - Add textarea for additional info
   - Handle event subscription/response

3. **VSCode Extension Integration**:
   - Create webview panel for questions
   - Implement VSCode-specific UI components
   - Handle event flow through extension API

## Summary

Phase 2 successfully implements the infrastructure for interactive tool execution:

✅ **BaseExecutor** accepts and passes context parameter
✅ **BuiltinExecutor** merges contexts properly
✅ **ToolCallExecutor** detects interactive tools and builds context
✅ **All executors** updated for interface consistency
✅ **Compilation** successful with no errors

The foundation is now in place for Phase 3 UI integration across all application layers.
