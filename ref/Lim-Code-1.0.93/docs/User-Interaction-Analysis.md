# Lim-Code Agent Runtime User Interaction Analysis

## 1. Overview

Lim-Code implements a sophisticated user interaction system for its LLM agent runtime, providing multiple interaction patterns including tool confirmation, message editing, retry mechanisms, and checkpoint-based recovery. The system follows a streaming-first architecture with real-time feedback to users.

## 2. Core Architecture

### 2.1 Interaction Flow Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (Vue.js)                         │
│  - ToolMessage.vue (Tool confirmation UI)                   │
│  - MessageActions.vue (Edit/Retry/Delete actions)           │
│  - DeleteDialog/EditDialog/RetryDialog                      │
│  - ChatStore (State management)                             │
└─────────────────────────────────────────────────────────────┘
                              │
                    VSCode Extension IPC
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              StreamRequestHandler (webview/)                 │
│  - handleChatStream                                         │
│  - handleToolConfirmationStream                             │
│  - handleEditAndRetryStream                                 │
│  - handleRetryStream                                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  ChatHandler (backend)                       │
│  - handleChatStream                                         │
│  - handleToolConfirmation                                   │
│  - handleEditAndRetryStream                                 │
│  - handleRetryStream                                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│               ChatFlowService                                │
│  - handleChatStream (New conversation flow)                 │
│  - handleToolConfirmation (Tool approval flow)              │
│  - handleEditAndRetryStream (Edit & resend flow)            │
│  - handleRetryStream (Retry last response)                  │
│  - handleDeleteToMessage (Delete & rollback)                │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│          ToolIterationLoopService                            │
│  - runToolLoop (Core agent loop with tool execution)        │
│  - runNonStreamLoop (Non-streaming variant)                 │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Key Components

| Component | Location | Responsibility |
|-----------|----------|----------------|
| `ChatFlowService` | `backend/modules/api/chat/services/ChatFlowService.ts` | Orchestrates all user interaction flows |
| `ToolIterationLoopService` | `backend/modules/api/chat/services/ToolIterationLoopService.ts` | Manages tool execution loop with user confirmation |
| `ToolExecutionService` | `backend/modules/api/chat/services/ToolExecutionService.ts` | Executes tools with progress tracking |
| `ConversationManager` | `backend/modules/conversation/ConversationManager.ts` | Manages message history and metadata |
| `CheckpointService` | `backend/modules/api/chat/services/CheckpointService.ts` | Workspace snapshots for rollback |
| `ChatStore` | `frontend/src/stores/chat/index.ts` | Frontend state management |
| `ToolMessage.vue` | `frontend/src/components/message/ToolMessage.vue` | Tool confirmation UI component |

## 3. Tool Confirmation Mechanism

### 3.1 Design Philosophy

The tool confirmation mechanism allows users to control which tools the AI can execute, providing a safety layer for potentially dangerous operations. Tools are categorized into:

- **Auto-execute tools**: Execute without user confirmation (safe operations)
- **Confirmation-required tools**: Require explicit user approval before execution

### 3.2 Configuration System

Tools can be configured through the AutoExecSettings interface:

```typescript
// Settings Manager determines if a tool needs confirmation
toolNeedsConfirmation(toolName: string): boolean {
    // Check if tool is disabled in current mode
    if (getToolRejectionReason(toolName) !== null) {
        return false;  // Disabled tools don't wait for confirmation
    }
    // Use unified auto-execution configuration
    return !settingsManager.isToolAutoExec(toolName);
}
```

**Configuration Rules:**
- Dangerous tools (file deletion, command execution) default to requiring confirmation
- MCP tools default to requiring confirmation
- Users can customize per-tool settings via UI

### 3.3 Execution Flow

#### Phase 1: Initial Tool Call Detection

```typescript
// In ToolIterationLoopService.runToolLoop()

// Extract function calls from AI response
const functionCalls = extractFunctionCalls(finalContent);

if (functionCalls.length === 0) {
    // No tools, complete normally
    return;
}

// Find first confirmation-required tool (in order)
const autoPrefix: FunctionCallInfo[] = [];
let firstConfirmTool: FunctionCallInfo | null = null;

for (const call of functionCalls) {
    if (toolNeedsConfirmation(call.name)) {
        firstConfirmTool = call;
        break;
    }
    autoPrefix.push(call);
}
```

**Key Rule:** Tools execute sequentially in AI output order. When encountering the first confirmation-required tool, execution pauses and waits for user input. Subsequent tools must wait for previous tools to complete.

#### Phase 2: Auto-Execute Prefix Tools

```typescript
if (autoPrefix.length > 0) {
    // Send initial toolsExecuting event with full queue
    yield {
        conversationId,
        content: finalContent,
        toolsExecuting: true,
        pendingToolCalls: autoPrefix.map(c => ({
            id: c.id,
            name: c.name,
            args: c.args
        }))
    };

    // Execute tools with progress tracking
    const gen = executeFunctionCallsWithProgress(autoPrefix, ...);
    
    while (true) {
        const { value, done } = await gen.next();
        if (done) {
            executionResult = value;
            break;
        }
        
        const event = value as ToolExecutionProgressEvent;
        
        if (event.type === 'start') {
            // Update remaining queue before each tool executes
            yield {
                conversationId,
                content: finalContent,
                toolsExecuting: true,
                pendingToolCalls: remaining.map(...)
            };
        }
        
        if (event.type === 'end') {
            // Send individual tool status update
            yield {
                conversationId,
                toolStatus: true,
                tool: {
                    id: event.call.id,
                    name: event.call.name,
                    status: 'success' | 'error' | 'warning',
                    result: event.toolResult.result
                }
            };
        }
    }
    
    // Add function responses to history
    await addContent(conversationId, {
        role: 'user',
        parts: functionResponseParts,
        isFunctionResponse: true
    });
}
```

#### Phase 3: Pause for User Confirmation

```typescript
if (firstConfirmTool) {
    yield {
        conversationId,
        pendingToolCalls: [{
            id: firstConfirmTool.id,
            name: firstConfirmTool.name,
            args: firstConfirmTool.args
        }],
        content: finalContent,
        awaitingConfirmation: true,
        toolResults: executionResult?.toolResults,
        checkpoints: executionResult?.checkpoints
    };
    
    return;  // Pause and wait for user decision
}
```

### 3.4 Frontend Handling

#### State Management

```typescript
// In ChatStore computed properties
const hasPendingToolConfirmation = computed(() => {
    // Must be waiting for response but not streaming
    if (!state.isWaitingForResponse.value || state.isStreaming.value) return false;
    
    // Check if last assistant message has tools with 'awaiting_approval' status
    const lastMessage = state.allMessages.value[state.allMessages.value.length - 1];
    if (!lastMessage?.tools) return false;
    
    return lastMessage.tools.some(t => t.status === 'awaiting_approval');
});
```

#### Tool Confirmation UI

```vue
<!-- ToolMessage.vue -->
<button
    v-if="tool.status === 'awaiting_approval'"
    class="confirm-btn"
    @click.stop="confirmToolExecution(tool.id, tool.name)"
>
    <span class="codicon codicon-check"></span>
    {{ t('components.message.tool.confirm') }}
</button>

<button
    v-if="tool.status === 'awaiting_approval'"
    class="reject-btn"
    @click.stop="rejectToolExecution(tool.id, tool.name)"
>
    <span class="codicon codicon-close"></span>
    {{ t('components.message.tool.reject') }}
</button>
```

#### Submit Decision

```typescript
async function submitToolDecision(toolId: string, toolName: string, confirmed: boolean) {
    processingToolIds.value.add(toolId);
    
    // Get optional annotation from input field
    const annotation = chatStore.inputValue.trim();
    
    const toolResponses = [{
        id: toolId,
        name: toolName,
        confirmed
    }];
    
    const streamId = generateId();
    state.activeStreamId.value = streamId;
    
    await sendToExtension('toolConfirmation', {
        conversationId: currentConversationId,
        configId: currentConfig.id,
        modelOverride: chatStore.pendingModelOverride || undefined,
        toolResponses,
        annotation,
        streamId,
        promptModeId: chatStore.currentPromptModeId
    });
}
```

### 3.5 Backend Processing of Confirmation

```typescript
// In ChatFlowService.handleToolConfirmation()

// 1. Find the last model message with tool calls
const history = await getHistoryRef(conversationId);
let modelMessageIndex = -1;
let lastMessage: Content | undefined;

for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].role === 'model') {
        const calls = extractFunctionCalls(history[i]);
        if (calls.length > 0) {
            modelMessageIndex = i;
            lastMessage = history[i];
            break;
        }
    }
}

// 2. Filter out already-responded tools
const respondedToolIds = new Set<string>();
for (let i = modelMessageIndex + 1; i < history.length; i++) {
    const msg = history[i];
    if (msg.parts) {
        for (const part of msg.parts) {
            if (part.functionResponse?.id) {
                respondedToolIds.add(part.functionResponse.id);
            }
        }
    }
}

const pendingCalls = allFunctionCalls.filter(call => !respondedToolIds.has(call.id));

// 3. Process the next pending tool (queue head)
const nextCall = allFunctionCalls.find(call => !respondedToolIds.has(call.id));
const nextDecision = toolResponses.find(r => r.id === nextCall.id);

if (nextDecision.confirmed) {
    // Execute the approved tool
    const gen = executeFunctionCallsWithProgress([nextCall], ...);
    // ... process execution events
} else {
    // Reject the tool
    await rejectToolCalls(conversationId, messageIndex, [nextCall.id]);
    
    yield {
        conversationId,
        toolStatus: true,
        tool: {
            id: nextCall.id,
            name: nextCall.name,
            status: 'error',
            result: { success: false, error: 'User rejected', rejected: true }
        }
    };
}

// 4. Auto-execute subsequent non-confirmation tools
const autoSuffix = [];
let nextConfirmTool = null;

for (let i = nextIndex + 1; i < allFunctionCalls.length; i++) {
    const c = allFunctionCalls[i];
    if (respondedToolIds.has(c.id) || resolvedIdsThisTurn.has(c.id)) continue;
    
    if (toolNeedsConfirmation(c.name)) {
        nextConfirmTool = c;
        break;
    }
    autoSuffix.push(c);
}

if (autoSuffix.length > 0) {
    // Execute suffix tools automatically
    // ... similar to prefix execution
}

// 5. Persist function responses
if (responseParts.length > 0) {
    await addContent(conversationId, {
        role: 'user',
        parts: confirmFunctionResponseParts,
        isFunctionResponse: true
    });
}

// 6. Add user annotation if provided
if (request.annotation && request.annotation.trim()) {
    await addContent(conversationId, {
        role: 'user',
        parts: [{ text: request.annotation.trim() }]
    });
}

// 7. If there's another confirmation-required tool, pause again
if (nextConfirmTool) {
    yield {
        conversationId,
        pendingToolCalls: [{
            id: nextConfirmTool.id,
            name: nextConfirmTool.name,
            args: nextConfirmTool.args
        }],
        content: lastMessage,
        awaitingConfirmation: true,
        toolResults: toolResultsThisTurn,
        checkpoints: checkpointsThisTurn
    };
    return;
}

// 8. All tools completed, continue AI dialogue
yield {
    conversationId,
    content: lastMessage,
    toolIteration: true,
    toolResults: toolResultsThisTurn,
    checkpoints: checkpointsThisTurn
};

// Continue the tool loop for AI to process results
for await (const output of runToolLoop({
    conversationId,
    configId,
    config,
    modelOverride,
    abortSignal: request.abortSignal,
    summarizeAbortSignal: request.summarizeAbortSignal,
    isFirstMessage: false,
    maxIterations: getMaxToolIterations(),
    createBeforeModelCheckpoint: false,
    isNewTurn: false
})) {
    yield output;
}
```

### 3.6 Streaming Data Types

```typescript
// Tool confirmation request (backend → frontend)
interface ChatStreamToolConfirmationData {
    conversationId: string;
    pendingToolCalls: PendingToolCall[];
    content: Content;
    awaitingConfirmation: true;
    toolResults?: Array<{
        id?: string;
        name: string;
        result: Record<string, unknown>;
    }>;
    checkpoints?: CheckpointRecord[];
}

// Tool execution start (backend → frontend)
interface ChatStreamToolsExecutingData {
    conversationId: string;
    content: Content;
    toolsExecuting: true;
    pendingToolCalls: PendingToolCall[];
}

// Individual tool status update (backend → frontend)
interface ChatStreamToolStatusData {
    conversationId: string;
    toolStatus: true;
    tool: {
        id: string;
        name: string;
        status: 'executing' | 'success' | 'error' | 'warning';
        result: Record<string, unknown>;
    };
}

// Tool confirmation response (frontend → backend)
interface ToolConfirmationResponseData {
    conversationId: string;
    configId: string;
    toolResponses: Array<{
        id: string;
        name: string;
        confirmed: boolean;
    }>;
    annotation?: string;  // Optional user comment
    modelOverride?: string;
    streamId: string;
    promptModeId?: string;
}
```

## 4. Message Editing and Retry

### 4.1 Edit Message Flow

Users can edit any message in the conversation history and resend it, triggering a new AI response.

#### Frontend Implementation

```typescript
// In messageActions.ts
async function editAndRetry(
    messageIndex: number,
    newMessage: string,
    attachments: Attachment[] | undefined
) {
    // Cancel any ongoing stream
    if (state.isStreaming.value || state.isWaitingForResponse.value) {
        await cancelStream();
    }
    
    // Calculate backend message index
    const backendMessageIndex = calculateBackendIndex(
        state.allMessages.value,
        messageIndex,
        state.windowStartIndex.value
    );
    
    // Update the edited message
    const targetMessage = state.allMessages.value[messageIndex];
    targetMessage.content = newMessage;
    targetMessage.parts = [{ text: newMessage }];
    targetMessage.attachments = attachments;
    
    // Delete all messages after the edited one
    state.allMessages.value = state.allMessages.value.slice(0, messageIndex + 1);
    clearCheckpointsFromIndex(state, backendMessageIndex);
    
    // Create placeholder assistant message
    const assistantMessageId = generateId();
    state.allMessages.value.push({
        id: assistantMessageId,
        role: 'assistant',
        content: '',
        timestamp: Date.now(),
        backendIndex: getNextBackendIndex(state),
        streaming: true,
        localOnly: true
    });
    state.streamingMessageId.value = assistantMessageId;
    
    // Send edit and retry request
    await sendToExtension('editAndRetryStream', {
        conversationId: state.currentConversationId.value,
        messageIndex: backendMessageIndex,
        newMessage,
        attachments: attachmentData,
        configId: state.configId.value,
        modelOverride,
        streamId,
        promptModeId: state.currentPromptModeId.value
    });
}
```

#### Backend Processing

```typescript
// In ChatFlowService.handleEditAndRetryStream()

async *handleEditAndRetryStream(request: EditAndRetryRequestData) {
    const { conversationId, messageIndex, newMessage, configId, modelOverride, attachments } = request;
    
    // 1. Ensure conversation exists
    await ensureConversation(conversationId);
    
    // 2. Validate configuration
    const config = await configManager.getConfig(configId);
    
    // 3. Interrupt pending diff operations
    diffInterruptService.markUserInterrupt();
    await diffInterruptService.cancelAllPending();
    
    // 4. Reject all unresponded tool calls
    await conversationManager.rejectAllPendingToolCalls(conversationId);
    
    // 5. Delete checkpoints from the edited message onwards
    await checkpointService.deleteCheckpointsFromIndex(conversationId, messageIndex);
    
    // 6. Delete messages from the target index
    const deletedCount = await conversationManager.deleteToMessage(conversationId, messageIndex);
    
    // 7. Rebuild TODO list metadata from remaining history
    await rebuildTodoListMetadataFromHistory(conversationId);
    
    // 8. Clear trim state (recalculate after edit)
    await toolIterationLoopService.clearTrimState(conversationId);
    
    // 9. Add the edited user message
    await conversationManager.addContent(conversationId, {
        role: 'user',
        parts: [{ text: newMessage }],
        isUserInput: true,
        ...(attachments ? { attachments } : {})
    });
    
    // 10. Create checkpoint after user message
    const afterEditCheckpoint = await checkpointService.createUserMessageCheckpoint(
        conversationId,
        'after',
        messageIndex
    );
    if (afterEditCheckpoint) {
        yield {
            conversationId,
            checkpoints: [afterEditCheckpoint],
            checkpointOnly: true
        };
    }
    
    // 11. Reset interrupt flag
    diffInterruptService.resetUserInterrupt();
    
    // 12. Determine if this is editing the first message (refresh system prompt)
    const isEditFirstMessage = messageIndex === 0;
    
    // 13. Run tool iteration loop
    const maxToolIterations = getMaxToolIterations();
    
    for await (const output of toolIterationLoopService.runToolLoop({
        conversationId,
        configId,
        config,
        modelOverride,
        abortSignal: request.abortSignal,
        summarizeAbortSignal: request.summarizeAbortSignal,
        isFirstMessage: isEditFirstMessage,
        maxIterations: maxToolIterations
    })) {
        yield output;
    }
}
```

### 4.2 Retry Mechanism

Users can retry the last AI response, deleting it and requesting a new response.

#### Frontend Implementation

```typescript
async function retryMessage(messageIndex: number) {
    // Cancel ongoing stream
    if (state.isStreaming.value || state.isWaitingForResponse.value) {
        await cancelStream();
    }
    
    // Calculate backend index
    const backendMessageIndex = calculateBackendIndex(...);
    
    // Delete messages from target index onwards
    state.allMessages.value = state.allMessages.value.slice(0, messageIndex);
    clearCheckpointsFromIndex(state, backendMessageIndex);
    
    // Create placeholder assistant message
    const assistantMessageId = generateId();
    state.allMessages.value.push({
        id: assistantMessageId,
        role: 'assistant',
        content: '',
        timestamp: Date.now(),
        backendIndex: getNextBackendIndex(state),
        streaming: true,
        localOnly: true
    });
    state.streamingMessageId.value = assistantMessageId;
    
    // Send retry request
    await sendToExtension('retryStream', {
        conversationId: state.currentConversationId.value,
        configId: state.configId.value,
        modelOverride,
        streamId,
        promptModeId: state.currentPromptModeId.value
    });
}
```

#### Backend Processing

```typescript
// In ChatFlowService.handleRetryStream()

async *handleRetryStream(request: RetryRequestData) {
    const { conversationId, configId, modelOverride } = request;
    
    // 1. Ensure conversation exists
    await ensureConversation(conversationId);
    
    // 2. Validate configuration
    const config = await configManager.getConfig(configId);
    
    // 3. Find the last user message
    const history = await conversationManager.getHistoryRef(conversationId);
    let lastUserMessageIndex = -1;
    
    for (let i = history.length - 1; i >= 0; i--) {
        if (history[i].role === 'user' && history[i].isUserInput) {
            lastUserMessageIndex = i;
            break;
        }
    }
    
    if (lastUserMessageIndex === -1) {
        yield { conversationId, error: { code: 'NO_USER_MESSAGE', ... } };
        return;
    }
    
    // 4. Interrupt pending diffs
    diffInterruptService.markUserInterrupt();
    await diffInterruptService.cancelAllPending();
    
    // 5. Reject all unresponded tool calls
    await conversationManager.rejectAllPendingToolCalls(conversationId);
    
    // 6. Delete checkpoints from last user message onwards
    await checkpointService.deleteCheckpointsFromIndex(conversationId, lastUserMessageIndex);
    
    // 7. Delete messages from last user message onwards
    const deletedCount = await conversationManager.deleteToMessage(
        conversationId,
        lastUserMessageIndex + 1
    );
    
    // 8. Rebuild TODO list metadata
    await rebuildTodoListMetadataFromHistory(conversationId);
    
    // 9. Clear trim state
    await toolIterationLoopService.clearTrimState(conversationId);
    
    // 10. Reset interrupt flag
    diffInterruptService.resetUserInterrupt();
    
    // 11. Run tool iteration loop (treat as first message if needed)
    const maxToolIterations = getMaxToolIterations();
    const isFirstMessage = lastUserMessageIndex === 0;
    
    for await (const output of toolIterationLoopService.runToolLoop({
        conversationId,
        configId,
        config,
        modelOverride,
        abortSignal: request.abortSignal,
        summarizeAbortSignal: request.summarizeAbortSignal,
        isFirstMessage,
        maxIterations: maxToolIterations
    })) {
        yield output;
    }
}
```

## 5. Message Deletion and Checkpoint Recovery

### 5.1 Delete Message Flow

When deleting a message, users can optionally restore to a checkpoint before deletion to recover file changes.

#### Frontend Dialog

```vue
<!-- DeleteDialog.vue -->
<template>
    <div class="dialog">
        <div class="dialog-header">
            <i class="codicon codicon-trash"></i>
            <span>{{ t('components.common.deleteDialog.title') }}</span>
        </div>
        <div class="dialog-body">
            <p>{{ deleteMessageText }}</p>
            
            <!-- Checkpoint hint -->
            <p v-if="hasCheckpoints" class="checkpoint-hint">
                <i class="codicon codicon-info"></i>
                {{ t('components.common.deleteDialog.checkpointHint') }}
            </p>
        </div>
        <div class="dialog-footer">
            <button class="cancel" @click="$emit('cancel')">
                {{ t('components.common.deleteDialog.cancel') }}
            </button>
            
            <!-- Restore options if checkpoints exist -->
            <button
                v-if="latestCheckpoint"
                class="restore"
                @click="$emit('restore-and-delete', latestCheckpoint.id)"
            >
                <i class="codicon codicon-discard"></i>
                {{ formatCheckpointDesc(latestCheckpoint) }}
            </button>
            
            <button class="delete" @click="$emit('delete')">
                {{ t('components.common.deleteDialog.delete') }}
            </button>
        </div>
    </div>
</template>
```

#### Backend Processing

```typescript
// In ChatFlowService.handleDeleteToMessage()

async handleDeleteToMessage(request: DeleteToMessageRequestData) {
    const { conversationId, targetIndex } = request;
    
    // 1. Ensure conversation exists
    await ensureConversation(conversationId);
    
    // 2. Interrupt pending diff operations
    diffInterruptService.markUserInterrupt();
    await diffInterruptService.cancelAllPending();
    
    // 3. Reject all unresponded tool calls
    await conversationManager.rejectAllPendingToolCalls(conversationId);
    
    try {
        // 4. Delete checkpoints from target index onwards
        await checkpointService.deleteCheckpointsFromIndex(conversationId, targetIndex);
        
        // 5. Delete messages
        const deletedCount = await conversationManager.deleteToMessage(
            conversationId,
            targetIndex
        );
        
        // 6. Rebuild TODO list metadata
        await rebuildTodoListMetadataFromHistory(conversationId);
        
        // 7. Clear trim state
        await toolIterationLoopService.clearTrimState(conversationId);
        
        return {
            success: true,
            deletedCount
        };
    } finally {
        // 8. Reset interrupt flag
        diffInterruptService.resetUserInterrupt();
    }
}
```

### 5.2 Checkpoint Restoration

Users can restore workspace files to a previous checkpoint state.

```typescript
// In ChatFlowService

async handleRestoreCheckpoint(request: RestoreCheckpointRequestData) {
    const { conversationId, checkpointId } = request;
    
    // Restore workspace files to checkpoint state
    const result = await checkpointService.restoreCheckpoint(
        conversationId,
        checkpointId
    );
    
    return {
        success: result.success,
        restored: result.restored,
        deleted: result.deleted,
        skipped: result.skipped
    };
}
```

## 6. Cancellation Support

### 6.1 Abort Signal Integration

All streaming operations support cancellation via `AbortSignal`:

```typescript
// Creating abort controllers
const controller = new AbortController();
const summarizeController = new AbortController();

// Passing to backend
await sendToExtension('chatStream', {
    conversationId,
    configId,
    abortSignal: controller.signal,
    summarizeAbortSignal: summarizeController.signal
});

// Backend checks for cancellation
if (abortSignal?.aborted) {
    yield {
        conversationId,
        cancelled: true
    };
    return;
}
```

### 6.2 Frontend Cancellation

```typescript
// In ChatStore
async function cancelStream() {
    const currentStreamingId = state.activeStreamId.value;
    
    if (currentStreamingId) {
        // Mark tools as error/cancelled
        markIncompleteToolsAsError(state, currentStreamingId);
        ensureFunctionResponseMessageForRejectedTools(state);
        
        // Send cancel request to backend
        await sendToExtension('cancelStream', {
            conversationId: state.currentConversationId.value,
            streamId: currentStreamingId
        });
    }
    
    // Reset state
    state.streamingMessageId.value = null;
    state.activeStreamId.value = null;
    state.isLoading.value = false;
    state.isStreaming.value = false;
    state.isWaitingForResponse.value = false;
}
```

### 6.3 Cancel During Tool Confirmation

Users can cancel even when waiting for tool confirmation:

```typescript
// Cancel button is always visible during isWaitingForResponse
<button
    v-if="chatStore.isWaitingForResponse"
    class="cancel-btn"
    @click="handleCancel"
>
    <i class="codicon codicon-close"></i>
    {{ t('app.chat.cancel') }}
</button>
```

## 7. Dynamic Context Management

### 7.1 Turn-Based Context Caching

To maintain consistency within a conversation turn, dynamic context is generated once at the start of a turn and cached:

```typescript
// In ToolIterationLoopService.runToolLoop()

// Find turn start message (last user message with isUserInput=true)
const turnStartIndex = findTurnStartMessageIndex(historyRef);

if (isNewTurn || turnStartIndex < 0 || !historyRef[turnStartIndex]?.turnDynamicContext) {
    // New turn: generate dynamic context
    const runtimeContext = await loadDynamicRuntimeContext(conversationId);
    dynamicContextMessages = promptManager.getDynamicContextMessages(runtimeContext);
    dynamicContextText = promptManager.getDynamicContextText(runtimeContext);
    
    // Cache on turn start user message
    if (turnStartIndex >= 0) {
        await conversationManager.updateMessage(conversationId, turnStartIndex, {
            turnDynamicContext: dynamicContextText
        });
    }
} else {
    // Turn continuation (e.g., after tool confirmation): reuse cached context
    dynamicContextText = historyRef[turnStartIndex].turnDynamicContext!;
    dynamicContextMessages = [{
        role: 'user',
        parts: [{ text: dynamicContextText }]
    }];
}
```

### 7.2 Dynamic Context Components

Dynamic context includes:
- Current time
- Workspace file tree
- Open tabs
- Active editor
- Diagnostics
- Pinned files
- TODO list
- Skills content

These are injected temporarily before sending to LLM but not persisted in history.

## 8. Auto-Summarization

### 8.1 Trigger Conditions

When context exceeds token threshold, automatic summarization is triggered:

```typescript
// In ToolIterationLoopService.runToolLoop()

const trimResult = await contextTrimService.getHistoryWithContextTrimInfo(...);

if (trimResult.needsAutoSummarize && summarizeService) {
    console.log(`[ToolLoop] Auto-summarize triggered`);
    
    // Notify frontend
    yield {
        conversationId,
        autoSummaryStatus: true,
        status: 'started'
    };
    
    // Merge abort signals (main request + summarize-only cancel)
    const autoSummarizeAbortSignal = mergeAbortSignals(abortSignal, summarizeAbortSignal);
    
    // Execute summarization
    const summarizeResult = await summarizeService.handleAutoSummarize(
        conversationId,
        configId,
        autoSummarizeAbortSignal
    );
    
    if (summarizeResult.success) {
        // Insert summary message
        yield {
            conversationId,
            autoSummary: true,
            summaryContent: summarizeResult.summaryContent,
            insertIndex: summarizeResult.insertIndex
        };
        
        // Hide "summarizing" indicator
        yield {
            conversationId,
            autoSummaryStatus: true,
            status: 'completed'
        };
        
        // Re-fetch history and continue loop
        continue;
    }
    
    if ('error' in summarizeResult) {
        // Main request cancelled: end entire conversation
        if (abortSignal?.aborted) {
            yield { conversationId, cancelled: true };
            return;
        }
        
        // Summarize-only cancelled: continue without summarization
        const isSummaryOnlyAborted = summarizeResult.error.code === 'ABORTED';
        
        yield {
            conversationId,
            autoSummaryStatus: true,
            status: 'failed',
            message: isSummaryOnlyAborted
                ? t('modules.api.chat.errors.summarizeAborted')
                : summarizeResult.error.message
        };
    }
}
```

### 8.2 Separate Cancel Control

Users can cancel summarization without cancelling the main conversation:

```typescript
// Two separate abort controllers
const mainController = new AbortController();
const summarizeController = new AbortController();

// Cancel only summarization
summarizeController.abort();

// Cancel entire conversation
mainController.abort();
```

## 9. Error Handling and Retry

### 9.1 Automatic Retry Mechanism

Network errors trigger automatic retry with configurable attempts:

```typescript
// Retry configuration
const retryEnabled = config.retryEnabled ?? true;
const maxRetries = config.retryCount ?? 3;
const retryInterval = config.retryInterval ?? 3000;

// Retry logic in ChannelManager
for (let attempt = 1; attempt <= totalAttempts; attempt++) {
    try {
        const response = await executeRequest(httpRequest, abortSignal);
        return formatter.parseResponse(response);
    } catch (error) {
        if (!isRetryableError(error) || attempt >= totalAttempts) {
            throw error;
        }
        
        // Notify frontend about retry
        retryStatusCallback?.({
            type: 'retrying',
            attempt,
            maxAttempts: totalAttempts,
            nextRetryIn: retryInterval
        });
        
        await delay(retryInterval, abortSignal);
    }
}
```

### 9.2 Frontend Retry Status Display

```vue
<!-- App.vue -->
<div v-if="chatStore.retryStatus.type === 'retrying'" class="retry-panel">
    <div class="retry-header">
        <i class="codicon codicon-sync spin"></i>
        <span>{{ t('app.retryPanel.retrying') }}</span>
        
        <div class="retry-progress-inline">
            <i class="codicon codicon-sync spin"></i>
            <span>{{ chatStore.retryStatus.attempt }}/{{ chatStore.retryStatus.maxAttempts }}</span>
            <span v-if="chatStore.retryStatus.nextRetryIn" class="retry-countdown">
                ({{ Math.ceil(chatStore.retryStatus.nextRetryIn / 1000) }}s)
            </span>
        </div>
        
        <button class="retry-cancel-btn" @click="handleCancel">
            <i class="codicon codicon-close"></i>
        </button>
    </div>
    <div class="retry-body">
        <pre class="retry-error-json">{{ chatStore.retryStatus.error }}</pre>
    </div>
</div>
```

## 10. State Synchronization

### 10.1 Frontend-Backend Message Index Mapping

Frontend maintains a windowed view of messages, requiring index translation:

```typescript
// Calculate backend index from frontend index
function calculateBackendIndex(
    allMessages: Message[],
    frontendIndex: number,
    windowStartIndex: number
): number {
    return windowStartIndex + frontendIndex;
}

// Calculate frontend index from backend index
function calculateFrontendIndex(
    backendIndex: number,
    windowStartIndex: number
): number {
    return backendIndex - windowStartIndex;
}
```

### 10.2 Tool Status Synchronization

Backend sends real-time tool status updates that frontend merges with local state:

```typescript
// In streamChunkHandlers.ts
function handleAwaitingConfirmation(chunk, state) {
    const messageIndex = state.allMessages.value.findIndex(
        m => m.id === state.streamingMessageId.value
    );
    
    if (messageIndex !== -1 && chunk.content) {
        const message = state.allMessages.value[messageIndex];
        const finalMessage = contentToMessage(chunk.content, message.id);
        
        // Merge tools: prefer existing tools' runtime state
        const mergedTools = mergeToolsPreferExisting(
            message.tools,
            finalMessage.tools
        ) || [];
        
        // Mark tools as awaiting approval
        const pendingIds = new Set(
            (chunk.pendingToolCalls || []).map((t: any) => t.id)
        );
        
        updatedMessage.tools = mergedTools.map(tool => {
            if (pendingIds.has(tool.id)) {
                return { ...tool, status: 'awaiting_approval' };
            }
            return tool;
        });
        
        state.allMessages.value[messageIndex] = updatedMessage;
    }
}
```

## 11. Design Patterns

### 11.1 Sequential Tool Execution with Confirmation

**Pattern:** Tools execute in AI output order, pausing at the first confirmation-required tool.

**Benefits:**
- Predictable execution order
- User control over dangerous operations
- Automatic execution of safe tools
- Clear feedback on tool queue progress

**Implementation:**
```typescript
// Split tools into auto-prefix and confirmation tool
const autoPrefix = [];
let firstConfirmTool = null;

for (const call of functionCalls) {
    if (toolNeedsConfirmation(call.name)) {
        firstConfirmTool = call;
        break;
    }
    autoPrefix.push(call);
}

// Execute prefix automatically
if (autoPrefix.length > 0) {
    await executeFunctionCallsWithProgress(autoPrefix);
}

// Pause for confirmation if needed
if (firstConfirmTool) {
    yield { awaitingConfirmation: true, pendingToolCalls: [firstConfirmTool] };
    return;
}
```

### 11.2 Progressive Status Updates

**Pattern:** Real-time tool status updates during execution.

**Events:**
1. `toolsExecuting`: Initial queue display
2. `toolStatus`: Individual tool start/end
3. `awaitingConfirmation`: Pause for user input
4. `toolIteration`: Complete iteration with results

**Benefits:**
- Immediate user feedback
- Transparent execution progress
- Ability to cancel mid-execution

### 11.3 Turn-Based Context Consistency

**Pattern:** Generate dynamic context once per turn, cache for subsequent iterations.

**Benefits:**
- Consistent context within a turn
- Reduced redundant computation
- Stable file tree/tabs during tool execution

**Implementation:**
```typescript
// Cache on turn start user message
await updateMessage(conversationId, turnStartIndex, {
    turnDynamicContext: dynamicContextText
});

// Reuse in subsequent iterations
dynamicContextText = historyRef[turnStartIndex].turnDynamicContext;
```

### 11.4 Dual Abort Signal Pattern

**Pattern:** Separate abort signals for main request and sub-operations (summarization).

**Benefits:**
- Fine-grained cancellation control
- Cancel summarization without stopping main conversation
- Clean separation of concerns

## 12. Comparison with Modular Agent Framework

| Feature | Lim-Code | Modular Agent Framework |
|---------|----------|------------------------|
| **Tool Confirmation** | Sequential with pause at first confirmation tool | USER_INTERACTION node with custom handlers |
| **Message Editing** | Delete from index + resend | Not explicitly mentioned |
| **Retry Mechanism** | Delete last assistant message + regenerate | Not explicitly mentioned |
| **Cancellation** | AbortSignal throughout stack | Not explicitly mentioned |
| **Checkpoint Recovery** | Workspace file snapshots | State snapshots |
| **Context Management** | Turn-based caching with dynamic injection | Variable-based context |
| **Auto-Summarization** | Token threshold trigger with separate cancel | Not explicitly mentioned |
| **Real-time Feedback** | Progressive tool status updates | Event-driven architecture |
| **State Synchronization** | Windowed message view with index mapping | Direct state access |

## 13. Key Insights for Modular Agent Framework

### 13.1 Strengths to Adopt

1. **Sequential Tool Confirmation**: The pattern of executing safe tools automatically and pausing at the first dangerous tool provides excellent UX.

2. **Progressive Status Updates**: Real-time tool status updates give users transparency into execution progress.

3. **Turn-Based Context Caching**: Caching dynamic context per turn ensures consistency while reducing redundant computation.

4. **Dual Abort Signals**: Separating main request cancellation from sub-operation cancellation (e.g., summarization) provides fine-grained control.

5. **Annotation Support**: Allowing users to add comments when confirming/rejecting tools enriches the conversation context.

6. **Checkpoint-Aware Deletion**: Offering checkpoint restoration before deletion prevents accidental data loss.

### 13.2 Areas for Improvement

1. **Complexity**: The sequential confirmation approach requires careful state management between frontend and backend.

2. **Index Mapping**: Windowed message views require complex index translation logic.

3. **Tight Coupling**: Frontend and backend are tightly coupled through specific streaming protocols.

4. **Limited Parallelism**: Sequential tool execution limits parallelism opportunities.

### 13.3 Recommendations

1. **Adopt Sequential Confirmation**: Implement similar sequential tool confirmation in USER_INTERACTION nodes.

2. **Implement Progressive Updates**: Add real-time status updates for long-running operations.

3. **Add Context Caching**: Cache expensive-to-compute context within conversation turns.

4. **Support Annotations**: Allow users to add comments during interactive operations.

5. **Integrate Checkpoints**: Combine checkpoint recovery with message editing/deletion workflows.

6. **Separate Cancellation Controls**: Provide granular cancellation for sub-operations.

## 14. Conclusion

Lim-Code implements a sophisticated user interaction system that balances automation with user control. The tool confirmation mechanism, combined with progressive status updates and checkpoint recovery, provides a robust foundation for safe AI-assisted development.

Key takeaways:
- **User Control First**: Dangerous operations require explicit approval
- **Transparency**: Real-time feedback on tool execution progress
- **Safety Nets**: Checkpoints enable recovery from mistakes
- **Flexibility**: Edit, retry, and delete operations provide conversation control
- **Performance**: Context caching and selective recomputation optimize efficiency

This analysis provides valuable insights for enhancing the Modular Agent Framework's user interaction capabilities, particularly around tool confirmation, real-time feedback, and conversation management.

---

*Analysis completed: 2026-05-06*
