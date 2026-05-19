# Hook System Analysis and Enhancement Plan

## Overview

This document provides a comprehensive analysis of the hook system in the wf-agent project, including its architecture, trigger mechanisms, condition expression capabilities, and enhancement recommendations.

**Last Updated**: 2026-05-19  
**Status**: Phase 1 Completed (Agent Message Access), Phase 2 Completed (Workflow Enhancement with Node Output)

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Hook Trigger Mechanisms](#hook-trigger-mechanisms)
3. [Condition Expression System](#condition-expression-system)
4. [Available Data Sources](#available-data-sources)
5. [Current Limitations](#current-limitations)
6. [Enhancement Proposals](#enhancement-proposals)
7. [Implementation Roadmap](#implementation-roadmap)

---

## Architecture Overview

### Layered Design

The hook system follows a layered architecture with clear separation of concerns:

```
┌─────────────────────────────────────────┐
│   Business Layer (Workflow / Agent)     │
│   - Workflow Node Hooks                 │
│   - Agent Loop Hooks                    │
└─────────────────────────────────────────┘
              ↓ Uses
┌─────────────────────────────────────────┐
│   Core Hook Framework (sdk/core/hooks)  │
│   - Filter & Sort                       │
│   - Condition Evaluation                │
│   - Handler Chain Execution             │
│   - Event Emission                      │
└─────────────────────────────────────────┘
              ↓ Depends on
┌─────────────────────────────────────────┐
│   Expression Engine                     │
│   (packages/common-utils/evaluator)     │
│   - AST Parser                          │
│   - Expression Evaluator                │
│   - Security Validator                  │
└─────────────────────────────────────────┘
```

### Key Components

#### 1. Type Definitions (`packages/types/src/node/hooks.ts`)

```typescript
export type HookType = "BEFORE_EXECUTE" | "AFTER_EXECUTE";

export interface NodeHook {
  hookType: HookType;
  condition?: Condition;           // Optional trigger condition
  eventName: string;               // Event name to emit
  eventPayload?: Record<string, unknown>;
  enabled?: boolean;               // Default: true
  weight?: number;                 // Priority (higher = first)
  createCheckpoint?: boolean;      // Create checkpoint when triggered
  checkpointDescription?: string;
}
```

#### 2. Agent Hook Types (`packages/types/src/agent-execution/hooks.ts`)

```typescript
export type AgentHookType =
  | "BEFORE_ITERATION"
  | "AFTER_ITERATION"
  | "BEFORE_TOOL_CALL"
  | "AFTER_TOOL_CALL"
  | "BEFORE_LLM_CALL"
  | "AFTER_LLM_CALL";
```

#### 3. Core Executor (`sdk/core/hooks/executor.ts`)

Provides reusable hook execution logic:
- `filterAndSortHooks()` - Filter by type and sort by weight
- `evaluateHookCondition()` - Evaluate condition expressions
- `executeHooks()` - Execute hooks with parallel/serial support
- `resolvePayloadTemplate()` - Resolve template variables in payloads

---

## Hook Trigger Mechanisms

### Workflow Module

#### Trigger Points

Located in `sdk/workflow/execution/coordinators/node-execution-coordinator.ts`:

```typescript
// Step 3: BEFORE_EXECUTE hooks (before node execution)
await executeHook(context, "BEFORE_EXECUTE", emitEvent);

// Step 4: Execute node logic
const nodeResult = await this.executeNodeLogic(...);

// Step 6: AFTER_EXECUTE hooks (after node execution)
await executeHook({ ...context, result: nodeResult }, "AFTER_EXECUTE", emitEvent);
```

#### Execution Flow

```
Node Execution Start
    ↓
[BEFORE_EXECUTE Hooks]
    ├─ Filter hooks by type
    ├─ Evaluate conditions
    ├─ Execute handler chain:
    │   ├─ Checkpoint Handler (if configured)
    │   ├─ Custom Handler (if provided)
    │   └─ Event Emitter Handler
    └─ Emit hook_triggered event
    ↓
[Node Logic Execution]
    ↓
[AFTER_EXECUTE Hooks]
    ├─ Same flow as BEFORE_EXECUTE
    └─ Includes node execution result
    ↓
Node Execution Complete
```

### Agent Module

#### Trigger Points

Located in `sdk/agent/execution/coordinators/agent-execution-coordinator.ts` and `tool-execution-coordinator.ts`:

**Agent Loop Coordinator:**
```typescript
// Before iteration
await executeAgentHook(entity, "BEFORE_ITERATION", emitEvent);

// Before LLM call
await executeAgentHook(entity, "BEFORE_LLM_CALL", emitEvent);
const llmResult = await this.llmExecutor.executeLLMCall(...);

// After LLM call
await executeAgentHook(entity, "AFTER_LLM_CALL", emitEvent, undefined, {
  content: response.content,
  toolCalls: response.toolCalls,
});

// After iteration (no tool calls)
await executeAgentHook(entity, "AFTER_ITERATION", emitEvent);
```

**Tool Execution Coordinator:**
```typescript
// Before tool call
await executeAgentHook(entity, "BEFORE_TOOL_CALL", emitEvent, toolCallInfo);

// After tool call (success)
await executeAgentHook(entity, "AFTER_TOOL_CALL", emitEvent, {
  ...toolCallInfo,
  result: result.result,
});

// After tool call (failure)
await executeAgentHook(entity, "AFTER_TOOL_CALL", emitEvent, {
  ...toolCallInfo,
  error: result.error,
});
```

---

## Condition Expression System

### ⚠️ Important Finding: NOT A SHELL!

The condition system is **fully implemented** with a complete expression engine, not just an empty shell as might be assumed.

### Implementation Stack

```
Hook Configuration (condition?: Condition)
    ↓
ConditionEvaluator.evaluate()
    ↓ expression: string
ExpressionEvaluator.evaluate()
    ↓ Parse to AST
parseAST() → AST Node Tree
    ↓
evaluateAST() → Recursive Evaluation
    ↓
Returns boolean result
```

### Supported Expression Types

#### 1. Comparison Operators (8 types)

```typescript
user.age == 25              // Equal
user.age != 30              // Not equal
user.age > 20               // Greater than
user.age < 30               // Less than
user.age >= 18              // Greater or equal
user.age <= 65              // Less or equal
user.name contains 'oh'     // String contains
user.role in ['admin', 'user']  // Array membership
```

#### 2. Logical Operators (3 types)

```typescript
user.age >= 18 && user.age <= 65    // AND
status == 'active' || status == 'pending'  // OR
!user.isActive                      // NOT
!(age < 18)                         // NOT with grouping
```

#### 3. Arithmetic Operators (5 types)

```typescript
user.age + 1        // Addition
price * 0.9         // Multiplication
count % 2           // Modulo
score - 10          // Subtraction
total / 2           // Division
```

#### 4. String Methods (6 types)

```typescript
user.name.startsWith('J')      // Prefix check
user.email.endsWith('@xxx')    // Suffix check
user.name.length               // String length
user.name.toLowerCase()        // To lowercase
user.name.toUpperCase()        // To uppercase
user.name.trim()               // Trim whitespace
```

#### 5. Ternary Operator

```typescript
age >= 18 ? 'adult' : 'minor'
status == 'active' ? 'enabled' : 'disabled'
```

#### 6. Literals

```typescript
true                    // Boolean
false                   // Boolean
null                    // Null
42                      // Integer
3.14                    // Float
-100                    // Negative
'hello'                 // String (single quotes)
"world"                 // String (double quotes)
['admin', 'user']       // Array
```

### Data Source Access Rules

#### Explicit Prefix Access

```typescript
input.status == 'active'              // From input namespace
output.result.success == true         // From output namespace
variables.user.age == 25              // From variables namespace
```

#### Implicit Access (Syntactic Sugar)

```typescript
// Simple variable names default to variables namespace
maxAge == 65            // Equivalent to: variables.maxAge == 65
user.age == 25          // Equivalent to: variables.user.age == 25

// Nested paths also from variables unless prefixed
user.name               // From variables
output.result.message   // Explicitly from output
```

#### Array Index Access

```typescript
input.tags[0] == 'admin'
data.items[0].name == 'test'
```

### Security Features

1. **Expression Length Limit**: Maximum 1000 characters
2. **Path Depth Limit**: Maximum 10 levels of nesting
3. **Forbidden Properties**: Blocks `__proto__`, `constructor`, `prototype` access
4. **Path Format Validation**: Only allows alphanumeric, underscore, dot, array brackets
5. **Type Safety**: Prevents injection of special objects (Date, RegExp, etc.)

### Error Handling

- **Syntax Errors**: Throw `RuntimeValidationError`
- **Runtime Errors**: Log warning and return `false`
- **Undefined Variables**: Return `undefined`, comparisons return `false`
- **Type Mismatches**: Log warning and return `false`

---

## Available Data Sources

### Workflow Module

#### Current Implementation

Located in `sdk/workflow/execution/handlers/hook-handlers/context-builder.ts`:

```typescript
export function convertToEvaluationContext(
  hookContext: HookEvaluationContext
): EvaluationContext {
  return {
    input: {},  // ❌ EMPTY - No input data exposed
    output: {
      result: hookContext.output,        // workflowExecution.output (entire object)
      status: hookContext.status,        // Node execution status
      executionTime: hookContext.executionTime,
      error: hookContext.error,
    },
    variables: hookContext.variables,    // ✅ All variables from VariableManager
  };
}
```

#### Available Data

**input**: ❌ Currently empty
- Should contain: Workflow initial input data
- Actual source: `workflowExecution.input` (not currently exposed)

**output**: ⚠️ Limited
- `output.result` - The entire `workflowExecution.output` object (final workflow output, not current node output)
- `output.status` - Node execution status (COMPLETED/FAILED/etc.)
- `output.executionTime` - Execution time in milliseconds
- `output.error` - Error information if failed

**variables**: ✅ Fully available
- Source: `variableStateManager.getAllVariables()`
- Contains: Global variables, workflow-level variables, node-local variables (if supported)
- Example: `variables.user.age`, `variables.retryCount`, `variables.tempData`

**Missing Data**:
- ❌ Current node's input/output
- ❌ Message history
- ❌ Previous node results
- ❌ Workflow execution context metadata

### Agent Module

#### Current Implementation

Located in `sdk/agent/execution/handlers/hook-handlers/context-builder.ts`:

```typescript
export function convertToEvaluationContext(
  hookContext: AgentHookEvaluationContext
): EvaluationContext {
  return {
    input: {
      iteration: hookContext.iteration,        // Current iteration count
      maxIterations: hookContext.maxIterations, // Maximum iterations
      toolCallCount: hookContext.toolCallCount, // Total tool calls
    },
    output: {
      status: hookContext.status,    // Agent status
      error: hookContext.error,      // Error if any
    },
    variables: {},  // ❌ EMPTY - Agent Loop does not use scoped variables
  };
}
```

#### Available Data

**input**: ⚠️ Limited to iteration metadata
- `input.iteration` - Current iteration number
- `input.maxIterations` - Configured maximum iterations
- `input.toolCallCount` - Total number of tool calls made

**output**: ⚠️ Limited to status
- `output.status` - Agent loop status (RUNNING/COMPLETED/FAILED)
- `output.error` - Last error message if failed

**variables**: ❌ Empty
- Comment states: "Agent Loop does not use scoped variables"

**Additional Context Available but NOT Exposed**:
The `AgentHookExecutionContext` has access to:
- `entity.config.profileId` - LLM profile ID
- `entity.config.systemPrompt` - System prompt
- `entity.config.tools` - Available tools list
- `entity.conversationManager` - **Conversation manager with ALL messages** ⭐
- `toolCall.id/name/arguments/result/error` - Tool call details (in specific hooks)
- `llmResponse.content/toolCalls` - LLM response (in AFTER_LLM_CALL hook)

---

## Current Limitations

### Critical Issues

#### 1. No Access to Message History

**Problem**: Cannot inspect conversation history in conditions

**Impact**: 
- Cannot check if a specific pattern exists in messages
- Cannot verify if a tool was called previously
- Cannot make decisions based on dialogue context

**Example Use Cases Blocked**:
```typescript
// ❌ Cannot do this currently:
// Check if last message is from user
input.messages[input.messages.length - 1].role == 'user'

// Check if search tool was ever called
input.messages.some(m => m.toolCalls?.some(tc => tc.function.name == 'search'))

// Check for error patterns
input.messages.some(m => m.content contains 'error')
```

#### 2. Limited Input/Output Data

**Workflow Issues**:
- `input` namespace is empty despite `workflowExecution.input` existing
- `output.result` is the final workflow output, not current node output
- No access to node-specific inputs/outputs

**Agent Issues**:
- `variables` namespace is empty
- Rich context (messages, config, tool calls) not exposed to conditions

#### 3. Basic `in` Operator Limitations

**Current Support**:
```typescript
// ✅ Works: Simple value in array
user.role in ['admin', 'user']
```

**Not Supported**:
```typescript
// ❌ Cannot check object properties in arrays
messages.some(m => m.role == 'user')

// ❌ Cannot query message arrays
messages.filter(m => m.role == 'assistant').length > 0
```

### Root Causes

1. **Context Builder Design**: The `convertToEvaluationContext()` functions selectively expose only a subset of available data
2. **Historical Focus**: Initial implementation focused on basic variable access, not complex data structures
3. **Separation Concern**: Agent loops intentionally don't use scoped variables (by design)

---

## Enhancement Proposals

### Priority P0 - Immediate Implementation (High Value, Low Cost)

#### Proposal 1: Add Messages Support to Agent Hooks

**Rationale**: Agent scenarios critically need access to conversation history for conditional logic.

**Implementation**:

Modify `sdk/agent/execution/handlers/hook-handlers/context-builder.ts`:

```typescript
export function convertToEvaluationContext(
  hookContext: AgentHookEvaluationContext,
): EvaluationContext {
  const messages = hookContext.entity.conversationManager.getAllMessages();
  const lastMessage = messages.length > 0 ? messages[messages.length - 1] : null;

  return {
    input: {
      iteration: hookContext.iteration,
      maxIterations: hookContext.maxIterations,
      toolCallCount: hookContext.toolCallCount,
      
      // ✅ NEW: Full message history
      messages: messages,
      
      // ✅ NEW: Convenience accessor for last message
      lastMessage: lastMessage,
    },
    output: {
      status: hookContext.status,
      error: hookContext.error,
    },
    variables: {},
  };
}
```

**Use Cases Enabled**:

```typescript
// Check if last message is from user
{
  hookType: "BEFORE_LLM_CALL",
  condition: {
    expression: "input.lastMessage.role == 'user'"
  }
}

// Check if user mentioned help
{
  hookType: "BEFORE_LLM_CALL",
  condition: {
    expression: "input.lastMessage.content contains 'help'"
  }
}

// Check if conversation has started
{
  hookType: "BEFORE_ITERATION",
  condition: {
    expression: "input.messages.length > 0"
  }
}

// Verify assistant responded
{
  hookType: "AFTER_ITERATION",
  condition: {
    expression: "input.lastMessage.role == 'assistant'"
  }
}
```

**Estimated Effort**: 2-4 hours (code + tests)

---

### Priority P1 - Short-term Implementation (Medium Value, Medium Cost)

#### Proposal 2: Add Node Output and Messages to Workflow Hooks

**Rationale**: Workflow scenarios need access to node-specific outputs and message history.

**Implementation**:

Modify `sdk/workflow/execution/handlers/hook-handlers/context-builder.ts`:

```typescript
export function buildHookEvaluationContext(
  context: HookExecutionContext
): HookEvaluationContext {
  const { workflowExecutionEntity, node, result } = context;
  const workflowExecution = workflowExecutionEntity.getExecution();

  return {
    // Keep existing
    output: workflowExecution.output,
    
    // ✅ NEW: Current node's output (from execution result)
    nodeOutput: result?.output,
    
    // ✅ NEW: Message history if available
    messages: workflowExecutionEntity.messageHistoryManager?.getAllMessages() || [],
    
    status: result?.status || "PENDING",
    executionTime: result?.executionTime || 0,
    error: result?.error,
    variables: workflowExecutionEntity.variableStateManager.getAllVariables(),
    config: node.config,
    metadata: node.metadata,
  };
}

export function convertToEvaluationContext(
  hookContext: HookEvaluationContext
): EvaluationContext {
  return {
    // ✅ NEW: Expose workflow input
    input: hookContext.workflowExecution?.input || {},
    
    output: {
      result: hookContext.output,
      nodeOutput: hookContext.nodeOutput,  // ✅ NEW: Node-specific output
      status: hookContext.status,
      executionTime: hookContext.executionTime,
      error: hookContext.error,
    },
    
    // ✅ NEW: Messages in input namespace for easy access
    messages: hookContext.messages,
    
    variables: hookContext.variables,
  };
}
```

**Use Cases Enabled**:

```typescript
// Check workflow input parameter
{
  hookType: "BEFORE_EXECUTE",
  condition: {
    expression: "input.userName == 'admin'"
  }
}

// Check current node output
{
  hookType: "AFTER_EXECUTE",
  condition: {
    expression: "output.nodeOutput.success == true"
  }
}

// Check message history
{
  hookType: "BEFORE_EXECUTE",
  condition: {
    expression: "input.messages.length > 5"
  }
}
```

**Estimated Effort**: 4-6 hours (code + tests + documentation)

---

#### Proposal 3: Enhanced `in` Operator for Object Arrays

**Rationale**: Enable more flexible array queries without full array method support.

**Implementation Option A: Smart Property Extraction**

Modify `packages/common-utils/src/evalutor/expression-evaluator.ts` in `evaluateComparison`:

```typescript
case "in":
  if (!Array.isArray(compareValue)) {
    logger.warn(`Right operand of 'in' operator must be an array`, {
      variablePath: node.variablePath,
      compareValue,
    });
    return false;
  }
  
  // ✅ ENHANCED: Support checking if object's common properties are in array
  if (typeof variableValue === 'object' && variableValue !== null && !Array.isArray(variableValue)) {
    // Try common property names for message-like objects
    const commonProps = ['role', 'name', 'type', 'id', 'status'];
    for (const prop of commonProps) {
      if (prop in variableValue && compareValue.includes(variableValue[prop])) {
        return true;
      }
    }
    return false;
  }
  
  // Original behavior for primitive values
  return compareValue.includes(variableValue);
```

**Implementation Option B: Array Projection Syntax**

Add support for projection syntax in parser:

```typescript
// New syntax: array[*].property
messages[*].role in ['user', 'assistant']

// Would check if ANY message has role in the array
```

**Recommended**: Start with Option A (simpler), evaluate need for Option B later.

**Use Cases**:

```typescript
// Check if message role is valid (with Option A)
{
  condition: {
    expression: "lastMessage in ['user', 'assistant', 'system']"
  }
}
```

**Estimated Effort**: 2-3 hours

---

### Priority P2 - Long-term Planning (High Value, High Cost)

#### Proposal 4: Full Array Method Support (some/every/filter/map)

**Rationale**: Enable complex array queries and transformations.

**Design**:

Add new AST node type in `packages/common-utils/src/evalutor/ast-types.ts`:

```typescript
export interface ArrayMethodNode {
  type: "arrayMethod";
  method: "some" | "every" | "filter" | "map" | "find";
  arrayPath: string;           // Path to the array
  predicate: ASTNode;          // Predicate expression
  variableName?: string;       // Iterator variable name (e.g., 'm' in m => ...)
}
```

**Target Syntax**:

```typescript
// Check if any message matches condition
input.messages.some(m => m.role == 'user')

// Check if all messages have content
input.messages.every(m => m.content != null)

// Filter and check length
input.messages.filter(m => m.role == 'assistant').length > 0

// Find specific message
input.messages.find(m => m.toolCalls != null) != null
```

**Implementation Challenges**:

1. **Parser Complexity**: Need to parse lambda expressions (`m => ...`)
2. **Scope Management**: Handle iterator variable scoping
3. **Performance**: Array operations on large message histories
4. **Security**: Prevent arbitrary code execution in predicates

**Alternative Approach**: Pre-defined helper functions instead of full lambda support

```typescript
// Simpler syntax without lambdas
messages.someEqual('role', 'user')
messages.everyHas('content')
messages.countWhere('role', 'assistant') > 0
```

**Estimated Effort**: 2-3 weeks (design + implementation + testing + security review)

---

## Implementation Roadmap

### ✅ Phase 1: Foundation - COMPLETED

**Goals**: Enable basic message access in Agent hooks

**Completed Tasks**:
- [x] Implement Proposal 1: Add messages to Agent hook context
- [x] Add unit tests for message access
- [x] Update documentation with examples
- [x] Test with real-world scenarios

**Deliverables**:
- Modified `sdk/agent/execution/handlers/hook-handlers/context-builder.ts`
- Test suite: `sdk/agent/execution/handlers/hook-handlers/__tests__/context-builder.test.ts`
- This documentation update

**Implementation Details**:

The `convertToEvaluationContext()` function now exposes:

```typescript
export function convertToEvaluationContext(
  hookContext: AgentHookEvaluationContext,
): EvaluationContext {
  // Extract messages from conversation manager
  const messages = hookContext.conversationManager.getAllMessages();
  const lastMessage = messages.length > 0 ? messages[messages.length - 1] : null;

  return {
    input: {
      iteration: hookContext.iteration,
      maxIterations: hookContext.maxIterations,
      toolCallCount: hookContext.toolCallCount,
      
      // NEW: Full message history for conditional logic
      messages: messages,
      
      // NEW: Convenience accessor for last message
      lastMessage: lastMessage,
    },
    output: {
      status: hookContext.status,
      error: hookContext.error,
    },
    variables: {}, // Agent Loop does not use scoped variables
  };
}
```

**Success Criteria**:
- ✅ Can access `input.messages` in Agent hook conditions
- ✅ Can check `input.lastMessage` properties
- ✅ All existing tests pass
- ✅ New unit tests cover message access patterns

---

### ✅ Phase 2: Workflow Enhancement - COMPLETED

**Goals**: Improve Workflow hook data exposure with node output support

**Completed Tasks**:
- [x] Extend NodeExecutionResult type with optional `output` field
- [x] Update node-execution-coordinator to preserve handler output
- [x] Implement sanitizeNodeOutput() method for different node types
- [x] Expose workflow input to hook conditions
- [x] Expose message history to hook conditions
- [x] Expose node-specific output to hook conditions
- [x] Add comprehensive unit tests
- [x] Verify SCRIPT, SUBGRAPH, LLM, AGENT_LOOP, FORK nodes

**Deliverables**:
- Modified `packages/types/src/workflow-execution/history.ts` (added output field)
- Modified `sdk/workflow/execution/coordinators/node-execution-coordinator.ts` (preserve output)
- Modified `sdk/workflow/execution/handlers/hook-handlers/context-builder.ts` (expose data)
- Test suite: `sdk/workflow/execution/handlers/hook-handlers/__tests__/context-builder.test.ts`
- Analysis document: `docs/architecture/node-output-analysis.md`

**Implementation Details**:

1. **Type Extension**:
```typescript
export interface NodeExecutionResult {
  // ... existing fields ...
  output?: unknown;  // NEW: Optional node execution output
}
```

2. **Coordinator Update**:
```typescript
// In executeNodeLogic()
return {
  nodeId: node.id,
  nodeType: node.type,
  status,
  step: workflowExecutionEntity.getNodeResults().length + 1,
  startTime,
  endTime,
  executionTime: diffTimestamp(startTime, endTime),
  output: this.sanitizeNodeOutput(output, node.type),  // NEW
};
```

3. **Context Builder Update**:
```typescript
export function convertToEvaluationContext(hookContext: HookEvaluationContext): EvaluationContext {
  return {
    input: {
      ...hookContext.workflowInput,  // NEW: Workflow input
      messages: hookContext.messages,  // NEW: Message history
    },
    output: {
      result: hookContext.output,
      nodeOutput: hookContext.nodeOutput,  // NEW: Node-specific output
      status: hookContext.status,
      executionTime: hookContext.executionTime,
      error: hookContext.error,
    },
    variables: hookContext.variables,
  };
}
```

**Supported Node Outputs**:
- ✅ **SCRIPT**: Raw script execution result
- ✅ **LLM**: `{ content, toolCalls }`
- ✅ **AGENT_LOOP**: `{ finalResponse, toolCallCount, iterationCount }`
- ✅ **SUBGRAPH**: `{ executionResult: { output, status }, duration }`
- ✅ **FORK**: Array of `{ forkPathId, output, status }`
- ✅ **JOIN**: Aggregated branch results (future enhancement)
- ✅ **END**: `{ output: workflowOutput }`

**Use Cases Enabled**:

```typescript
// Check workflow input parameter
{
  hookType: "BEFORE_EXECUTE",
  condition: {
    expression: "input.userName == 'admin'"
  }
}

// Check script output
{
  hookType: "AFTER_EXECUTE",
  condition: {
    expression: "output.nodeOutput.transformedCount > 0"
  }
}

// Check LLM response content
{
  hookType: "AFTER_EXECUTE",
  condition: {
    expression: "output.nodeOutput.content contains 'error'"
  }
}

// Check subgraph execution status
{
  hookType: "AFTER_EXECUTE",
  condition: {
    expression: "output.nodeOutput.executionResult.status == 'COMPLETED'"
  }
}

// Check message history
{
  hookType: "BEFORE_EXECUTE",
  condition: {
    expression: "input.messages.length > 5"
  }
}
```

**Success Criteria**:
- ✅ Can access `input.userName` (workflow input) in conditions
- ✅ Can access `output.nodeOutput` (node-specific output)
- ✅ Message history available in both modules
- ✅ All 12 unit tests passing
- ✅ Backward compatible (optional output field)

**Test Results**:
```
✓ buildHookEvaluationContext > should build context with workflow input
✓ buildHookEvaluationContext > should include node output from execution result
✓ buildHookEvaluationContext > should handle missing result gracefully
✓ buildHookEvaluationContext > should expose message history
✓ buildHookEvaluationContext > should include all required fields
✓ convertToEvaluationContext > should expose workflow input in input namespace
✓ convertToEvaluationContext > should expose node output in output namespace
✓ convertToEvaluationContext > should handle undefined node output
✓ convertToEvaluationContext > should preserve all evaluation context fields
✓ Integration > should support complete workflow hook scenario
✓ Integration > should support SUBGRAPH node output checking
✓ Integration > should support LLM node content checking

Test Files  1 passed (1)
Tests  12 passed (12)
```

---

## Phase 1 Usage Examples

### Example 1: Check Last Message Role

Trigger hook only when the last message is from the user:

```typescript
{
  hookType: "BEFORE_LLM_CALL",
  condition: {
    expression: "input.lastMessage.role == 'user'"
  },
  eventName: "user.message.received"
}
```

### Example 2: Detect Help Requests

Check if user mentioned "help" in their last message:

```typescript
{
  hookType: "BEFORE_LLM_CALL",
  condition: {
    expression: "input.lastMessage.content contains 'help'"
  },
  eventName: "help.request.detected",
  eventPayload: {
    action: "show_help_menu"
  }
}
```

### Example 3: Monitor Conversation Length

Create checkpoint after conversation reaches certain length:

```typescript
{
  hookType: "AFTER_ITERATION",
  condition: {
    expression: "input.messages.length > 10"
  },
  eventName: "conversation.checkpoint",
  createCheckpoint: true,
  checkpointDescription: "Long conversation checkpoint"
}
```

### Example 4: Verify Assistant Response

Ensure assistant has responded before proceeding:

```typescript
{
  hookType: "AFTER_ITERATION",
  condition: {
    expression: "input.lastMessage.role == 'assistant'"
  },
  eventName: "assistant.response.complete"
}
```

### Example 5: Check Message Count Threshold

Trigger warning when conversation becomes too long:

```typescript
{
  hookType: "BEFORE_ITERATION",
  condition: {
    expression: "input.messages.length >= 50"
  },
  eventName: "conversation.warning",
  eventPayload: {
    message: "Conversation approaching maximum length"
  }
}
```

### Example 6: Complex Condition with Multiple Checks

Combine multiple conditions:

```typescript
{
  hookType: "BEFORE_LLM_CALL",
  condition: {
    expression: "input.lastMessage.role == 'user' && input.messages.length < 100"
  },
  eventName: "safe.to.continue"
}
```

---

### Phase 2: Workflow Enhancement (Week 2)

**Goals**: Improve Workflow hook data exposure

**Tasks**:
- [ ] Implement Proposal 2: Add node output and messages to Workflow hooks
- [ ] Expose `workflowExecution.input` to conditions
- [ ] Add integration tests
- [ ] Document breaking changes (if any)

**Deliverables**:
- Modified `context-builder.ts` for Workflow
- Integration tests with sample workflows
- Migration guide if needed

**Success Criteria**:
- Can access `input.userName` (workflow input) in conditions
- Can access `output.nodeOutput` (node-specific output)
- Message history available in both modules

---

### Phase 3: Expression Enhancement (Week 3)

**Goals**: Make array queries more practical

**Tasks**:
- [ ] Implement Proposal 3: Enhanced `in` operator
- [ ] Add tests for object-in-array scenarios
- [ ] Benchmark performance with large arrays
- [ ] Document limitations and best practices

**Deliverables**:
- Enhanced `expression-evaluator.ts`
- Performance test results
- Usage guidelines

**Success Criteria**:
- Object property checking works reliably
- No performance degradation for existing use cases
- Clear documentation of supported patterns

---

### Phase 4: Advanced Features (Month 2+)

**Goals**: Evaluate and implement array methods if needed

**Tasks**:
- [ ] Gather user feedback on Phases 1-3
- [ ] Assess demand for full array methods
- [ ] Design lambda expression syntax (if proceeding)
- [ ] Security audit of expression evaluation
- [ ] Implement Proposal 4 (if approved)

**Decision Point**: After Phase 3, evaluate if simpler helper functions suffice or if full array methods are necessary.

---

## Workarounds (Until Implementation)

### Workaround 1: Use Variables to Pass Messages

```typescript
// Before executing hooks, manually store messages in variables
workflowExecutionEntity.variableStateManager.setVariable(
  'messages',
  conversationManager.getAllMessages()
);

// Then use in conditions
{
  condition: {
    expression: "variables.messages.length > 0"
  }
}
```

**Pros**: No code changes needed  
**Cons**: Manual setup, not automatic, clutters variables namespace

---

### Workaround 2: Custom Handler Functions

```typescript
{
  hookType: "BEFORE_LLM_CALL",
  eventName: "check.message.pattern",
  eventPayload: {
    handler: (context, hook, eventData) => {
      const messages = context.entity.conversationManager.getAllMessages();
      const hasPattern = messages.some(m => 
        typeof m.content === 'string' && m.content.includes('specific pattern')
      );
      
      // Take action based on result
      if (hasPattern) {
        // Custom logic here
      }
    }
  }
}
```

**Pros**: Full flexibility, can implement any logic  
**Cons**: Requires JavaScript code, not declarative, harder to serialize/configure

---

### Workaround 3: Pre-filtering in Application Code

```typescript
// In application code before starting execution
const shouldTriggerHook = messages.some(m => m.role === 'user');

if (shouldTriggerHook) {
  // Add hook dynamically or set a flag variable
  agentConfig.hooks.push({
    hookType: "BEFORE_LLM_CALL",
    eventName: "user.message.detected",
    enabled: true
  });
}
```

**Pros**: Clean separation of concerns  
**Cons**: Logic outside of hook system, harder to maintain

---

## Testing Strategy

### Unit Tests

**Location**: `packages/common-utils/src/evalutor/__tests__/`

**Test Cases**:
1. Message array access patterns
2. Object property extraction
3. Edge cases (empty arrays, null messages, etc.)
4. Performance with large message histories (1000+ messages)

### Integration Tests

**Location**: `sdk/agent/__tests__/` and `sdk/workflow/__tests__/`

**Test Scenarios**:
1. Agent loop with message-based conditions
2. Workflow with node output conditions
3. Multi-turn conversations with conditional hooks
4. Error handling when messages are unavailable

### Real-world Validation

**Test Cases**:
1. Customer service bot: Detect escalation keywords in messages
2. Code assistant: Check if user provided code snippets
3. Data processing: Validate input parameters before execution
4. Multi-agent coordination: Check message history for coordination signals

---

## Migration Guide

### For Existing Users

**Breaking Changes**: None (additive only)

**New Capabilities**:
```typescript
// Old: Limited to variables
{
  condition: {
    expression: "variables.retryCount < 3"
  }
}

// New: Can now use messages and enhanced data sources
{
  condition: {
    expression: "input.lastMessage.role == 'user' && variables.retryCount < 3"
  }
}
```

### Recommended Updates

1. **Review existing hooks**: Identify opportunities to use message-based conditions
2. **Update documentation**: Reference new data sources in hook configuration docs
3. **Add examples**: Provide real-world examples in documentation
4. **Monitor performance**: Watch for any performance impact with large message histories

---

## Conclusion

### Key Findings

1. ✅ **Condition system is fully functional** - Not a shell, has complete expression engine
2. ⚠️ **Data exposure is limited** - Rich context exists but not exposed to conditions
3. 🔴 **Message access is critical gap** - Especially for Agent scenarios
4. 🟡 **Array operations need enhancement** - Current `in` operator is too basic

### Recommendations

1. **Immediate**: Implement Phase 1 (Agent message access) - highest ROI
2. **Short-term**: Complete Phases 2-3 for comprehensive improvement
3. **Long-term**: Evaluate Phase 4 based on user feedback and实际需求

### Expected Impact

- **Developer Experience**: Significantly improved with declarative message-based conditions
- **Flexibility**: Enable complex conditional logic without custom handlers
- **Maintainability**: Declarative conditions easier to understand and debug than imperative code
- **Performance**: Minimal impact (message access is read-only, no additional computation)

---

## References

### Related Documentation

- [Hook System Architecture](./hook-architecture.md) - This document
- [Expression Engine Details](../../packages/common-utils/src/evalutor/README.md)
- [Agent Loop Architecture](../agent-loop-architecture.md)
- [Workflow Execution Model](../workflow-type-improvements.md)

### Code Locations

- **Type Definitions**: `packages/types/src/node/hooks.ts`, `packages/types/src/agent-execution/hooks.ts`
- **Core Executor**: `sdk/core/hooks/executor.ts`
- **Workflow Handler**: `sdk/workflow/execution/handlers/hook-handlers/hook-handler.ts`
- **Agent Handler**: `sdk/agent/execution/handlers/hook-handlers/hook-handler.ts`
- **Context Builders**: 
  - `sdk/workflow/execution/handlers/hook-handlers/context-builder.ts`
  - `sdk/agent/execution/handlers/hook-handlers/context-builder.ts`
- **Expression Engine**: `packages/common-utils/src/evalutor/`

### Test Files

- **Condition Evaluator Tests**: `packages/common-utils/src/evalutor/__tests__/condition-evaluator.test.ts`
- **Expression Parser Tests**: `packages/common-utils/src/evalutor/__tests__/expression-parser.test.ts`

---

**Document Version**: 1.0  
**Author**: AI Analysis  
**Review Status**: Pending Technical Review  
**Next Review Date**: After Phase 1 Implementation
