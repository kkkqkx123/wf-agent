# Node Handlers

## 1. Handler Resolution

Node handlers are resolved via `getNodeHandler(nodeType)` with a **three-tier priority**:

1. **Built-in handlers** (hardcoded) — highest priority
2. **Plugin-contributed handlers** (via `ContributionManager`) — resolved at call site
3. **Template-based handlers** (via `NodeTemplateRegistry`) — resolved at call site

## 2. Handler Catalog

### Boundary Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `startHandler` | `START` | Initializes workflow execution, records start time |
| `endHandler` | `END` | Finalizes workflow, builds output, triggers completion |
| `startFromTriggerHandler` | `START_FROM_TRIGGER` | Starts execution from a trigger event context |
| `continueFromTriggerHandler` | `CONTINUE_FROM_TRIGGER` | Continues execution after trigger handling |
| `embedStartHandler` | `EMBED_START` | Internal node for EMBED_GRAPH expansion |
| `embedEndHandler` | `EMBED_END` | Internal node for EMBED_GRAPH expansion |

### Control Flow Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `routeHandler` | `ROUTE` | Evaluates conditions and routes to matching branch |
| `forkHandler` | `FORK` | Creates parallel branch executions |
| `joinHandler` | `JOIN` | Waits for all fork branches, aggregates results |
| `syncHandler` | `SYNC` | Explicit synchronization between fork branches |
| `loopStartHandler` | `LOOP_START` | Marks the beginning of a loop |
| `loopEndHandler` | `LOOP_END` | Evaluates loop condition, routes back or forward |

### Execution Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `llmHandler` | `LLM` | Coordinates LLM call with tool execution |
| `scriptHandler` | `SCRIPT` | Executes a predefined script |
| `interactiveScriptHandler` | `INTERACTIVE_SCRIPT` | Interactive script with PTY terminal session |
| `agentLoopHandler` | `AGENT_LOOP` | Delegates to AgentLoopExecutor for agent-style execution |

### Data Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `variableHandler` | `VARIABLE` | Manages variable operations (set, get, transform) |
| `contextProcessorHandler` | `CONTEXT_PROCESSOR` | Processes context data for downstream nodes |

### Composition Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `subgraphHandler` | `SUBGRAPH` | Creates child execution entity for sub-workflow |
| `triggeredSubworkflowHandler` | (triggered) | Manages triggered sub-workflow execution |

### Interaction Nodes

| Handler | Node Type | Behavior |
|---------|-----------|----------|
| `userInteractionHandler` | `USER_INTERACTION` | Handles user input/output during execution |
| `toolVisibilityHandler` | `TOOL_VISIBILITY` | Manages tool visibility for LLM context |

## 3. Fork/Join Handler Detail

### ForkHandler

```
forkHandler(globalContext, executionEntity, node, context):
  for each branch in forkConfig.branches:
    1. Create child WorkflowExecutionEntity via WorkflowExecutionBuilder
    2. Copy variables and conversation state
    3. Register child in execution hierarchy
    4. Execute child asynchronously via WorkflowExecutor
    5. Collect branch results
  return aggregated ForkBranchResult[]
```

- Each branch executes as an independent execution entity
- Variables are copied per-branch (isolated execution contexts)
- Results are collected and passed to the JOIN handler

### JoinHandler

```
joinHandler(globalContext, executionEntity, node):
  1. Collect all branch results
  2. Evaluate join strategy (all/any/n)
  3. Aggregate outputs:
     - Variable outputs: merge per scope
     - Message outputs: merge conversation history
     - Data outputs: merge result data
  4. Return aggregated JoinNodeOutput
```

**Join Strategies**:
- `all` — Wait for all branches to complete
- `any` — Complete when first branch completes
- `n` — Complete when N branches complete

## 4. LLM Handler Detail

The `llmHandler` delegates to `LLMExecutionCoordinator` (workflow-specific), which composes the shared `LLMExecutionCoordinator`:

```
LLMExecutionCoordinator.execute(params):
  1. Build LLM context via LLMContextFactory
  2. Execute LLM call (via shared LLMExecutionCoordinator)
  3. Handle tool calls with approval
  4. Track token usage and checkpoint
  5. Return LLMNodeOutput
```

## 5. Hook Handlers

Hooks are executed **before** and **after** node execution:

```
Hook Execution Flow:
  BEFORE_EXECUTE hooks:
    1. Filter and sort hooks by criteria
    2. Build evaluation context
    3. Execute matching hooks
    4. Handle checkpoint creation if configured

  AFTER_EXECUTE hooks:
    1. Filter and sort hooks by criteria
    2. Build evaluation context (with node result)
    3. Execute matching hooks
    4. Handle checkpoint creation if configured
```

- Hook types: `BEFORE_EXECUTE`, `AFTER_EXECUTE`, `ON_ERROR`, `ON_COMPLETE`
- Hooks support custom event emission via `emitHookEvent`