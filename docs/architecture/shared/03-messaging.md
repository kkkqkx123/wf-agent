# Shared Messaging System

## 1. Overview

The shared messaging system provides conversation and message management for both workflow and agent execution. It handles message storage, state coordination, and cross-boundary message passing.

## 2. ConversationSession

The core message management class:

```
ConversationSession
├── Message Management
│   ├── getMessages() → LLMMessage[]
│   ├── addMessage(message) → void
│   ├── addMessages(messages) → void
│   ├── getMessageCount() → number
│   ├── getMessageAt(index) → LLMMessage?
│   └── clear() → void
│
├── Token Management
│   ├── getTokenUsage() → TokenUsageStats
│   ├── updateTokenUsage(usage) → void
│   └── getCurrentRequestUsage() → TokenUsageStats?
│
├── Message History Management
│   ├── getMessageHistory() → MessageHistory
│   ├── getVisibleRange() → MessageRange
│   └── setVisibleRange(range) → void
│
├── Checkpoint Support
│   ├── createSnapshot() → ConversationSnapshot
│   └── restoreFromSnapshot(snapshot) → void
│
└── Batch Operations
    ├── addMessagesWithDedup(messages) → void
    └── getMessagesInRange(start, end) → LLMMessage[]
```

## 3. BaseStateCoordinator

Provides unified message/state management base class for both `AgentStateCoordinator` and `WorkflowStateCoordinator`:

```
BaseStateCoordinator<TSnapshot>
├── Message Management
│   ├── getConversationManager() → ConversationSession
│   ├── getMessages() → LLMMessage[]
│   ├── addMessage(message) → void
│   ├── addMessages(messages) → void
│   └── getMessageCount() → number
│
├── Token Management
│   ├── getTokenUsage() → TokenUsageStats
│   └── updateTokenUsage(usage) → void
│
├── Checkpoint Support
│   ├── createSnapshot() → TSnapshot
│   └── restoreFromSnapshot(snapshot) → void
│
├── Parent-Child Message Passing
│   ├── exportMessagesForChild() → LLMMessage[]
│   ├── importMessagesFromChild(messages) → void
│   └── exportAllMessagesForCheckpoint() → LLMMessage[]
│
└── Batch Management
    ├── getBatchBatchManager() → BatchManager
    └── executeBatchOperation(operation) → void
```

## 4. Message Structure

### LLMMessage

```typescript
interface LLMMessage {
  id: string;
  role: "system" | "user" | "assistant" | "tool";
  content: string | ContentBlock[];
  tool_calls?: ToolCall[];
  tool_call_id?: string;
  name?: string;
  timestamp: number;
  metadata?: Record<string, unknown>;
}
```

### Message Content

```typescript
type ContentBlock = TextBlock | ImageBlock | ToolResultBlock;

interface TextBlock {
  type: "text";
  text: string;
}

interface ImageBlock {
  type: "image";
  source: { type: "base64" | "url"; data: string };
}

interface ToolResultBlock {
  type: "tool_result";
  tool_call_id: string;
  content: string;
}
```

## 5. Message Array Management

Utilities for efficient message array operations:

```
MessageArrayManager
├── addMessage(messages, message) → LLMMessage[]
├── addMessages(messages, newMessages) → LLMMessage[]
├── removeMessage(messages, id) → LLMMessage[]
├── updateMessage(messages, id, update) → LLMMessage[]
├── findMessage(messages, id) → LLMMessage?
├── filterMessages(messages, predicate) → LLMMessage[]
└── sliceMessages(messages, start, end) → LLMMessage[]
```

## 6. Message Indexing

Utilities for message indexing and lookup:

```
MessageIndexUtils
├── buildIndex(messages) → Map<string, number>
├── getMessageById(messages, id) → LLMMessage?
├── getMessagesByRole(messages, role) → LLMMessage[]
└── getMessageRange(messages, startId, endId) → LLMMessage[]
```

## 7. Cross-Boundary Message Passing

Handles message passing between parent and child executions:

```
CrossBoundaryConverter
├── convertForChild(messages, parentContext) → LLMMessage[]
│   └── Filter and format messages for child execution
│
├── convertFromChild(messages, childContext) → LLMMessage[]
│   └── Reformat child messages for parent context
│
└── createBoundaryMessage(content, boundary) → LLMMessage
    └── Create boundary marker messages
```

## 8. History Management

### MessageHistory

```
MessageHistory
├── getHistory() → LLMMessage[]
├── appendToHistory(message) → void
├── getHistoryInRange(start, end) → LLMMessage[]
├── getHistoryLength() → number
├── clearHistory() → void
└── createSnapshot() → MessageHistorySnapshot
```

### HistoryConverter

Converts between different message formats:

```
HistoryConverter
├── toLLMMessages(raw) → LLMMessage[]
├── fromLLMMessages(messages) → RawMessage[]
└── mergeHistories(histories) → LLMMessage[]
```

## 9. Prompt System

### SystemPromptResolver

Resolves system prompts from templates:

```
SystemPromptResolver
├── resolve(config) → Promise<string>
│   ├── If template: resolve template with variables
│   ├── If static: return as-is
│   └── If both: merge static + resolved template
│
├── resolveTemplate(templateId, variables) → Promise<string>
│   └── Load template from registry and interpolate variables
│
└── resolveSystemPrompt(prompt, templateId, variables) → Promise<string>
    └── Combined resolution logic
```

### InitialMessageBuilder

Builds initial messages for execution:

```
InitialMessageBuilder
├── buildInitialMessages(config) → LLMMessage[]
│   ├── Create system message from prompt
│   ├── Create user message from input
│   └── Return combined messages
│
└── buildSteeringMessage(content) → LLMMessage
    └── Create steering message
```

## 10. Visible Range Calculator

Determines which messages are visible to the LLM:

```
VisibleRangeCalculator
├── calculateVisibleRange(messages, config) → MessageRange
│   ├── Apply token budget
│   ├── Apply message count limit
│   ├── Preserve system messages
│   └── Return visible range
│
├── calculateTokenBudget(messages, maxTokens) → MessageRange
│   └── Calculate range within token budget
│
└── isMessageVisible(messageId, range) → boolean
    └── Check if message is in visible range
```