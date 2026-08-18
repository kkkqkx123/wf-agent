# Agent Iteration and Tool Execution

## 1. Iteration Lifecycle

Each agent loop iteration follows a well-defined lifecycle with hooks at each phase:

```
┌─────────────────────────────────────────────────────┐
│                  BEFORE_ITERATION hooks               │
├─────────────────────────────────────────────────────┤
│                  BEFORE_LLM_CALL hooks                │
├─────────────────────────────────────────────────────┤
│                  LLM Execution                         │
│  (LLMExecutionCoordinator.execute())                  │
├─────────────────────────────────────────────────────┤
│                  AFTER_LLM_CALL hooks                 │
├─────────────────────────────────────────────────────┤
│           Response Processing                         │
│  ┌───────────────┐     ┌───────────────────┐         │
│  │ Final Answer   │     │   Tool Calls      │         │
│  │ → Complete     │     │ → Execute Tools   │         │
│  └───────────────┘     └───────────────────┘         │
│                              │                        │
│                    ┌────────▼──────────┐              │
│                    │ BEFORE_TOOL_CALL  │              │
│                    │ hooks             │              │
│                    ├───────────────────┤              │
│                    │ Tool Execution    │              │
│                    │ (with approval)   │              │
│                    ├───────────────────┤              │
│                    │ AFTER_TOOL_CALL   │              │
│                    │ hooks             │              │
│                    └───────────────────┘              │
├─────────────────────────────────────────────────────┤
│                  AFTER_ITERATION hooks                │
├─────────────────────────────────────────────────────┤
│                  Trigger Execution                    │
│  (executeAgentTriggers)                               │
└─────────────────────────────────────────────────────┘
```

## 2. LLM Execution

The LLM execution is handled by the shared `LLMExecutionCoordinator`:

### LLM Call Flow

```
LLMExecutionCoordinator.execute(context):
  1. Prepare messages (system prompt + conversation history)
  2. Resolve LLM profile (model, provider, parameters)
  3. Select tool format (compatible with provider)
  4. Call LLM provider via LLMWrapper
  5. Process response:
     - Extract text content
     - Extract tool calls
     - Track token usage
  6. Return LLMResult
```

### Tool Format Compatibility

The `prepareToolSchemas()` function in `tool-schema-helper.ts`:
- Converts tool definitions to provider-specific schemas
- Validates tool format compatibility with selected LLM profile
- Uses `validateToolFormatCompatibility()` for pre-flight checks

### Message Preparation

Before each LLM call, messages are prepared from:
- System prompt (resolved from config or template)
- Conversation history (from ConversationSession)
- Steering messages (from external input)
- Context injection (error context, retry instructions)

## 3. Tool Execution

### ToolExecutionCoordinator

Coordinates tool call execution within an iteration:

```
ToolExecutionCoordinator.executeTools(toolCalls, entity, options):
  1. For each tool call (batch):
     a. Check tool approval:
        - Auto-approved: execute immediately
        - Confirmation required: wait for approval handler
        - Denied: skip with rejection message
     b. Execute BEFORE_TOOL_CALL hooks
     c. Execute tool via ToolCallExecutor:
        - Resolve tool from ToolRegistry
        - Execute with timeout
        - Handle errors (recoverable vs fatal)
     d. Execute AFTER_TOOL_CALL hooks
     e. Add tool result to conversation
     f. Emit tool execution events:
        - TOOL_CALL_STARTED
        - TOOL_CALL_COMPLETED / TOOL_CALL_FAILED
  2. Return batch execution results
```

### Tool Approval Flow

```
ToolApprovalCoordinator (shared):
├── autoApproved: execute immediately
├── confirmationRequired: delegate to ToolApprovalHandler
│   ├── approve → execute
│   ├── reject → skip with rejection message
│   └── timeout → handle as rejection
└── denied: skip with rejection message
```

### Tool Types

- **Builtin Tools**: Pre-registered tools (file operations, calculations, etc.)
- **Native Tools**: Custom tools registered via plugin/tool system
- **REST Tools**: External API tools
- **MCP Tools**: Model Context Protocol tools

## 4. Response Processing

After LLM execution, the response is processed:

### Final Answer Path

```
LLM response has text content + no tool calls:
  1. Record final answer in iteration history
  2. Add assistant message to conversation
  3. Check termination condition:
     - No more iterations needed → complete loop
     - Continue if follow-up mode allows
```

### Tool Call Path

```
LLM response has tool calls (with or without text):
  1. Record tool calls in iteration state
  2. Add assistant message with tool calls to conversation
  3. Execute tools via ToolExecutionCoordinator
  4. Add tool results to conversation
  5. Continue to next iteration for LLM to process results
```

### Mixed Response

```
LLM response has both text and tool calls:
  1. Extract text as reasoning
  2. Execute tool calls
  3. Continue to next iteration
  - If text is a final answer AND no tool calls are needed:
    Treat as final answer
```

## 5. Conversation Coordination

### ConversationCoordinator

Provides stateless conversation management:

```
ConversationCoordinator
├── getConversationManager(agentLoopId) → ConversationSession
├── getNormalizedHistory(agentLoopId) → LLMMessage[]
└── getConversationStats(agentLoopId) → Stats
```

### ConversationSession (shared)

The `ConversationSession` manages message state:

```
ConversationSession
├── getMessages() → LLMMessage[]
├── addMessage(msg) → void
├── getTokenUsage() → TokenUsageStats
├── createSnapshot() → ConversationSnapshot
├── restoreFromSnapshot(snapshot) → void
└── clear() → void
```

## 6. Iteration Termination

The iteration loop checks termination after each iteration:

| Condition | Action |
|-----------|--------|
| Final answer received (no tool calls) | Complete loop |
| maxIterations reached | Complete loop with warning |
| Pause interruption | Pause loop, create checkpoint |
| Stop interruption | Cancel loop |
| Error (non-recoverable) | Fail loop |
| Error (recoverable, retries left) | Retry with backoff |
| Error (recoverable, no retries) | Fail loop |

## 7. Streaming Support

The agent loop supports two streaming modes:

### Message Streaming

- Individual token chunks from LLM
- Real-time message content
- Tool call progress updates

### Agent Streaming Events

```
AgentStreamEvent:
├── ITERATION_START
├── ITERATION_END
├── LLM_CALL_START
├── LLM_CALL_END
├── TOOL_CALL_START
├── TOOL_CALL_END
└── AGENT_COMPLETED
```

Streaming is handled via `RunAgentLoopStreamCommand` which returns an async iterable of stream events.