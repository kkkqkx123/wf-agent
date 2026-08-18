# Shared Hooks System

## 1. Overview

The shared hooks system provides hook execution infrastructure used by both workflow and agent modules. Hooks allow custom logic to be executed at specific lifecycle points.

## 2. Hook Executor

The core hook execution engine:

```
HookExecutor
├── executeHooks(hooks, hookType, context) → Promise<HookResult[]>
│   ├── Filter hooks by type
│   ├── Sort hooks by priority
│   ├── For each hook:
│   │   a. Evaluate hook condition (if any)
│   │   b. If condition matches:
│   │      - Execute hook action
│   │      - Emit HOOK_EXECUTED event
│   │      - Collect result
│   │   c. If condition doesn't match:
│   │      - Skip hook
│   └── Return aggregated results
│
├── executeHook(hook, context) → Promise<HookResult>
│   └── Execute single hook with context
│
├── evaluateCondition(condition, context) → boolean
│   └── Evaluate hook condition expression
│
└── createHookContext(baseContext, extra) → HookContext
    └── Build complete hook execution context
```

## 3. Hook Types

### Workflow Hook Types

| Hook Type | Timing |
|-----------|--------|
| `BEFORE_EXECUTE` | Before node execution |
| `AFTER_EXECUTE` | After node execution |
| `ON_ERROR` | On node execution error |
| `ON_COMPLETE` | On workflow completion |

### Agent Hook Types

| Hook Type | Timing |
|-----------|--------|
| `BEFORE_ITERATION` | Before iteration starts |
| `AFTER_ITERATION` | After iteration ends |
| `BEFORE_LLM_CALL` | Before LLM call |
| `AFTER_LLM_CALL` | After LLM call |
| `BEFORE_TOOL_CALL` | Before tool call starts |
| `AFTER_TOOL_CALL` | After tool call ends |

## 4. Hook Context

```typescript
interface HookContext {
  executionId: string;
  hookType: HookType;
  timestamp: number;
  metadata?: Record<string, unknown>;
  [key: string]: unknown;  // Type-specific context
}
```

### Workflow Hook Context

```typescript
interface WorkflowHookContext extends HookContext {
  workflowExecutionEntity: WorkflowExecutionEntity;
  node: StaticNode;
  result?: NodeExecutionResult;
  error?: Error;
}
```

### Agent Hook Context

```typescript
interface AgentHookContext extends HookContext {
  entity: AgentLoopEntity;
  messages?: LLMMessage[];
  llmResult?: LLMResult;
  toolCalls?: ToolCallRecord[];
  toolResults?: ToolExecutionResult[];
  iteration?: number;
}
```

## 5. Hook Results

```typescript
interface HookResult {
  hookType: HookType;
  hookId: string;
  success: boolean;
  data?: unknown;
  error?: Error;
  executionTime: number;
  metadata?: Record<string, unknown>;
}
```

## 6. Hook Configuration

```typescript
interface Hook {
  type: HookType;
  condition?: HookCondition;  // Optional condition expression
  action: HookAction;         // Callback function
  priority?: number;          // Execution order (lower = earlier)
  metadata?: Record<string, unknown>;
  enabled?: boolean;
}

interface HookCondition {
  type: "expression" | "function";
  value: string | ((context: HookContext) => boolean);
}

type HookAction = (context: HookContext) => Promise<HookResult>;
```

## 7. Hook Event Emission

When a hook is executed, a `HOOK_EXECUTED` event is emitted:

```typescript
interface HookExecutedEvent {
  type: "HOOK_EXECUTED";
  hookType: HookType;
  hookId: string;
  executionId: string;
  success: boolean;
  executionTime: number;
  metadata?: Record<string, unknown>;
}
```

## 8. Hook Integration

### Workflow Integration

```
NodeExecutionCoordinator.executeNode():
  1. Execute BEFORE_EXECUTE hooks
  2. Execute node handler
  3. Execute AFTER_EXECUTE hooks
  4. On error: execute ON_ERROR hooks
```

### Agent Integration

```
AgentIterationCoordinator.executeIteration():
  1. Execute BEFORE_ITERATION hooks
  2. Execute BEFORE_LLM_CALL hooks
  3. LLM execution
  4. Execute AFTER_LLM_CALL hooks
  5. For each tool call:
     a. Execute BEFORE_TOOL_CALL hooks
     b. Execute tool
     c. Execute AFTER_TOOL_CALL hooks
  6. Execute AFTER_ITERATION hooks
```