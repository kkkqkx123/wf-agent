# Shared Event Builders

## 1. Overview

Event builder functions create structured event objects for the shared event system. Each builder function creates a specific event type with the correct payload structure.

## 2. Builder Pattern

All event builders follow a consistent pattern:

```typescript
function buildEventName(payload: EventPayload): Event {
  return {
    type: EventType.EVENT_NAME,
    timestamp: Date.now(),
    source: "component-name",
    payload,
  };
}
```

## 3. Event Builder Modules

### workflow-execution-events.ts

Builds workflow lifecycle and execution events:

| Builder | Event Type |
|---------|-----------|
| `buildWorkflowStartedEvent()` | `WORKFLOW_STARTED` |
| `buildWorkflowCompletedEvent()` | `WORKFLOW_COMPLETED` |
| `buildWorkflowFailedEvent()` | `WORKFLOW_FAILED` |
| `buildWorkflowPausedEvent()` | `WORKFLOW_PAUSED` |
| `buildWorkflowResumedEvent()` | `WORKFLOW_RESUMED` |
| `buildWorkflowCancelledEvent()` | `WORKFLOW_CANCELLED` |

### agent-events.ts

Builds agent lifecycle events:

| Builder | Event Type |
|---------|-----------|
| `buildAgentStartedEvent()` | `AGENT_STARTED` |
| `buildAgentCompletedEvent()` | `AGENT_COMPLETED` |
| `buildAgentFailedEvent()` | `AGENT_FAILED` |
| `buildAgentPausedEvent()` | `AGENT_PAUSED` |
| `buildAgentResumedEvent()` | `AGENT_RESUMED` |
| `buildAgentCancelledEvent()` | `AGENT_CANCELLED` |
| `buildAgentIterationCompletedEvent()` | `AGENT_ITERATION_COMPLETED` |
| `buildAgentToolExecutionStartedEvent()` | `AGENT_TOOL_EXECUTION_STARTED` |
| `buildAgentToolExecutionCompletedEvent()` | `AGENT_TOOL_EXECUTION_COMPLETED` |

### node-events.ts

Builds node execution events:

| Builder | Event Type |
|---------|-----------|
| `buildNodeExecutionStartedEvent()` | `NODE_EXECUTION_STARTED` |
| `buildNodeExecutionCompletedEvent()` | `NODE_EXECUTION_COMPLETED` |
| `buildNodeExecutionFailedEvent()` | `NODE_EXECUTION_FAILED` |
| `buildNodeExecutionSkippedEvent()` | `NODE_EXECUTION_SKIPPED` |

### llm-events.ts

Builds LLM-related events:

| Builder | Event Type |
|---------|-----------|
| `buildMessageAddedEvent()` | `MESSAGE_ADDED` |
| `buildTokenUsageWarningEvent()` | `TOKEN_USAGE_WARNING` |
| `buildConversationStateChangedEvent()` | `CONVERSATION_STATE_CHANGED` |

### tool-events.ts

Builds tool-related events:

| Builder | Event Type |
|---------|-----------|
| `buildToolCallStartedEvent()` | `TOOL_CALL_STARTED` |
| `buildToolCallCompletedEvent()` | `TOOL_CALL_COMPLETED` |
| `buildToolApprovalRequestedEvent()` | `TOOL_APPROVAL_REQUESTED` |
| `buildToolExecutionFailedEvent()` | `TOOL_EXECUTION_FAILED` |

### checkpoint-events.ts

Builds checkpoint lifecycle events:

| Builder | Event Type |
|---------|-----------|
| `buildCheckpointCreatedEvent()` | `CHECKPOINT_CREATED` |
| `buildCheckpointRestoredEvent()` | `CHECKPOINT_RESTORED` |
| `buildCheckpointFailedEvent()` | `CHECKPOINT_FAILED` |
| `buildCheckpointDeletedEvent()` | `CHECKPOINT_DELETED` |

### hook-events.ts

Builds hook execution events:

| Builder | Event Type |
|---------|-----------|
| `buildHookExecutedEvent()` | `HOOK_EXECUTED` |
| `buildNodeCustomEvent()` | `NODE_CUSTOM_EVENT` |

### conversation-events.ts

Builds conversation-related events:

| Builder | Event Type |
|---------|-----------|
| `buildConversationStartedEvent()` | `CONVERSATION_STARTED` |
| `buildConversationMessageAddedEvent()` | `CONVERSATION_MESSAGE_ADDED` |
| `buildConversationEndedEvent()` | `CONVERSATION_ENDED` |

### error-events.ts

Builds error events:

| Builder | Event Type |
|---------|-----------|
| `buildErrorEvent()` | `SYSTEM_ERROR` |
| `buildWarningEvent()` | `SYSTEM_WARNING` |

### system-events.ts

Builds system-level events:

| Builder | Event Type |
|---------|-----------|
| `buildSystemEvent()` | `SYSTEM_EVENT` |
| `buildConfigurationChangedEvent()` | `CONFIGURATION_CHANGED` |

### subgraph-events.ts

Builds subgraph execution events:

| Builder | Event Type |
|---------|-----------|
| `buildSubgraphStartedEvent()` | `SUBGRAPH_STARTED` |
| `buildSubgraphCompletedEvent()` | `SUBGRAPH_COMPLETED` |
| `buildSubgraphFailedEvent()` | `SUBGRAPH_FAILED` |

### custom-events.ts

Builds custom/user-defined events:

| Builder | Event Type |
|---------|-----------|
| `buildCustomEvent()` | `CUSTOM_EVENT` |
| `buildUserDefinedEvent()` | `USER_DEFINED` |

### interaction-events.ts

Builds user interaction events:

| Builder | Event Type |
|---------|-----------|
| `buildUserInputRequestedEvent()` | `USER_INPUT_REQUESTED` |
| `buildUserInputReceivedEvent()` | `USER_INPUT_RECEIVED` |
| `buildToolApprovalRequestedEvent()` | `TOOL_APPROVAL_REQUESTED` (user interaction) |

### skill-events.ts

Builds skill execution events:

| Builder | Event Type |
|---------|-----------|
| `buildSkillStartedEvent()` | `SKILL_STARTED` |
| `buildSkillCompletedEvent()` | `SKILL_COMPLETED` |
| `buildSkillFailedEvent()` | `SKILL_FAILED` |

### async-completion-events.ts

Builds async completion events:

| Builder | Event Type |
|---------|-----------|
| `buildAsyncTaskCompletedEvent()` | `ASYNC_TASK_COMPLETED` |
| `buildAsyncTaskFailedEvent()` | `ASYNC_TASK_FAILED` |

### attempt-completion-events.ts

Builds attempt completion events:

| Builder | Event Type |
|---------|-----------|
| `buildAttemptCompletionEvent()` | `ATTEMPT_COMPLETION` |

## 4. Common Event Builder

The `common.ts` module provides shared utilities:

```typescript
function createEvent(
  type: EventType,
  source: string,
  payload: Record<string, unknown>,
  metadata?: Record<string, unknown>
): Event
```

## 5. Event Payload Types

Each builder defines typed payload interfaces:

```typescript
interface AgentStartedPayload {
  agentLoopId: string;
  maxIterations: number;
  initialMessageCount: number;
  executionId: string;
}

interface AgentCompletedPayload {
  agentLoopId: string;
  iterationCount: number;
  toolCallCount: number;
  tokenUsage: TokenUsageStats;
  executionId: string;
}

interface WorkflowStartedPayload {
  workflowId: string;
  executionId: string;
  nodeCount: number;
  startNodeId: string;
}

// ... more payload types
```

## 6. Usage

```typescript
import { buildAgentStartedEvent, buildAgentCompletedEvent } from "../../shared/events/builders/agent-events.js";

// Build and emit
const startedEvent = buildAgentStartedEvent({
  agentLoopId: entity.id,
  maxIterations: entity.config.maxIterations ?? -1,
  initialMessageCount: messages.length,
  executionId: entity.id,
});
await emit(eventManager, startedEvent);
```