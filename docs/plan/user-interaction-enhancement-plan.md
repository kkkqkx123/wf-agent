# User Interaction Enhancement Implementation Plan

## Overview

This document outlines the phased implementation plan for enhancing the user interaction system in the Modular Agent Framework, based on best practices from Lim-Code analysis. The focus is on improving tool approval workflows, adding progressive feedback, and optimizing context management.

**Scope:** SDK and packages only (apps layer excluded from this phase)

**Timeline:** 4 phases over 3-4 weeks

**Key Objectives:**
1. Implement sequential tool execution with auto-approval prefix
2. Add progressive status update events
3. Support user annotations in approval decisions
4. Implement turn-based context caching
5. Maintain full backward compatibility

---

## Phase 1: Type System & Event Infrastructure

**Duration:** 3-4 days  
**Priority:** P0 (Foundation)  
**Risk:** Low (additive changes only)

### 1.1 Enhanced Type Definitions

**File:** `packages/types/src/tool/approval.ts`

**Changes:**
```typescript
// ENHANCE: ToolApprovalRequest - add batch context
export interface ToolApprovalRequest {
  toolCall: LLMToolCall;
  toolDescription?: string;
  contextId: string;
  nodeId?: string;
  interactionId: string;
  
  // NEW: Batch execution metadata
  batchId?: string;                    // Unique ID for this batch
  toolIndex?: number;                  // Position in batch (0-based)
  totalTools?: number;                 // Total tools in batch
  pendingQueue?: LLMToolCall[];        // Tools remaining after this one
  autoExecutedResults?: ToolExecutionResult[];  // Results from auto-executed prefix
}

// ENHANCE: ToolApprovalResult - add annotation and continuation flag
export interface ToolApprovalResult {
  approved: boolean;
  toolCallId: string;
  editedParameters?: Record<string, unknown>;
  userInstruction?: string;
  annotation?: string;                 // NEW: User's comment explaining decision
  rejectionReason?: string;
  
  // NEW: Batch control
  continueBatch?: boolean;             // Should continue with remaining tools?
}

// NEW: ToolBatchResult - result of processing a batch of tools
export interface ToolBatchResult {
  batchId: string;
  autoExecuted: ToolExecutionResult[];           // Auto-executed tools
  confirmationRequired: LLMToolCall | null;      // First tool needing approval
  confirmationResult?: ToolApprovalResult;       // User's decision
  remainingQueue: LLMToolCall[];                 // Tools not yet processed
  allCompleted: boolean;                         // True if all tools done
}

// NEW: Pending tool call info for UI display
export interface PendingToolCall {
  id: string;
  name: string;
  arguments?: string;
  riskLevel?: ToolRiskLevel;
}
```

**Rationale:** 
- Extends existing interfaces without breaking changes
- Provides rich context for sequential execution
- Enables handlers to show queue progress

---

**File:** `packages/types/src/interaction.ts`

**Changes:**
```typescript
// After Refactoring: Separated workflow config from general protocol

// Workflow-specific node configuration (workflow state management)
export interface UserInteractionNodeConfig {
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  variables?: WorkflowVariableUpdateConfig[];
  message?: WorkflowMessageConfig;
  prompt: string;
  timeout?: number;
  metadata?: Record<string, unknown>;
}

// General-purpose interaction protocol (app-level UI interactions)
export interface UserInteractionRequest {
  interactionId: ID;
  operationType: UserInteractionOperationType; // "TOOL_APPROVAL" | "ASK_FOLLOWUP_QUESTION"
  prompt: string;
  timeout: number;
  metadata?: Metadata & {
    // Structured tool approval data
    toolData?: ToolApprovalRequestData;
    // Structured follow-up question data
    followupData?: FollowupQuestionRequestData;
  };
}

// NEW: Structured tool approval request data
export interface ToolApprovalRequestData {
  toolCallId: string;
  toolName: string;
  toolDescription?: string;
  parameters: Record<string, unknown>;
  riskLevel?: ToolRiskLevel;
  pendingQueue?: PendingToolCall[];
  autoExecutedTools?: ToolExecutionResult[];
  batchId?: string;
  toolIndex?: number;
  totalTools?: number;
}

// NEW: Structured tool approval response data
export interface ToolApprovalResponseData {
  approved: boolean;
  editedParameters?: Record<string, unknown>;
  userInstruction?: string;
  annotation?: string;
  rejectionReason?: string;
  continueBatch?: boolean;
}

// ENHANCE: UserInteractionContext - add cache management
export interface UserInteractionContext {
  executionId: ID;
  workflowId: ID;
  nodeId: ID;
  getVariable(variableName: string, scope?: VariableScope): unknown;
  setVariable(variableName: string, value: unknown, scope?: VariableScope): Promise<void>;
  getVariables(scope?: VariableScope): Record<string, unknown>;
  timeout: number;
  cancelToken: {
    cancelled: boolean;
    cancel(): void;
  };
  
  // NEW: Context cache access
  getTurnDynamicContext?(turnStartIndex: number): string | undefined;
  setTurnDynamicContext?(turnStartIndex: number, context: string): void;
}
```

**Rationale:**
- Separates structured data from generic metadata
- Enables type-safe access in handlers
- Adds optional cache methods for coordinators

---

### 1.2 Progressive Event Types

**File:** `packages/types/src/events/interaction-events.ts`

**Changes:**
```typescript
// KEEP: Existing event types (backward compatible)

// NEW: Tool execution start event
export interface ToolExecutionStartEvent extends BaseEvent {
  type: 'TOOL_EXECUTION_START';
  executionId: ID;
  nodeId?: ID;
  toolCallId: string;
  toolName: string;
  batchId?: string;
  toolIndex?: number;
  totalTools?: number;
  pendingQueue?: PendingToolCall[];
}

// NEW: Tool execution end event
export interface ToolExecutionEndEvent extends BaseEvent {
  type: 'TOOL_EXECUTION_END';
  executionId: ID;
  nodeId?: ID;
  toolCallId: string;
  toolName: string;
  batchId?: string;
  status: 'success' | 'error' | 'warning';
  result?: ToolExecutionResult;
  executionTime?: number;
}

// NEW: Tool queue update event
export interface ToolQueueUpdateEvent extends BaseEvent {
  type: 'TOOL_QUEUE_UPDATE';
  executionId: ID;
  nodeId?: ID;
  batchId: string;
  completedCount: number;
  totalCount: number;
  pendingQueue: PendingToolCall[];
}

// NEW: Tool approval with annotation
export interface ToolApprovalAnnotatedEvent extends BaseEvent {
  type: 'TOOL_APPROVAL_ANNOTATED';
  executionId: ID;
  nodeId?: ID;
  interactionId: ID;
  toolCallId: string;
  toolName: string;
  annotation: string;
  approved: boolean;
}
```

**Rationale:**
- Enables real-time progress tracking
- Supports batch execution visualization
- Allows annotation logging and analytics

---

**File:** `packages/types/src/events/base.ts`

**Changes:**
```typescript
export type EventType =
  // ... existing types ...
  
  // NEW: Progressive tool execution events
  | 'TOOL_EXECUTION_START'
  | 'TOOL_EXECUTION_END'
  | 'TOOL_QUEUE_UPDATE'
  | 'TOOL_APPROVAL_ANNOTATED';
```

**Rationale:**
- Registers new event types in the system
- Enables type guards and filtering

---

### 1.3 Event Builders

**File:** `sdk/core/utils/event/builders/interaction-events.ts`

**Changes:**
```typescript
// KEEP: Existing builders

// NEW: Tool execution progress builders
export const buildToolExecutionStartEvent = createBuilder<ToolExecutionStartEvent>(
  'TOOL_EXECUTION_START'
);

export const buildToolExecutionEndEvent = createBuilder<ToolExecutionEndEvent>(
  'TOOL_EXECUTION_END'
);

export const buildToolQueueUpdateEvent = createBuilder<ToolQueueUpdateEvent>(
  'TOOL_QUEUE_UPDATE'
);

export const buildToolApprovalAnnotatedEvent = (
  params: BuildParams<ToolApprovalAnnotatedEvent>
) => ({
  type: 'TOOL_APPROVAL_ANNOTATED',
  timestamp: now(),
  ...params,
}) as ToolApprovalAnnotatedEvent;
```

**Rationale:**
- Consistent event construction pattern
- Type-safe event creation
- Automatic timestamp injection

---

### 1.4 Export Updates

**File:** `packages/types/src/index.ts`

**Changes:**
```typescript
// ADD exports for new types
export type {
  // ... existing exports ...
  
  // NEW: Batch approval types
  ToolBatchResult,
  PendingToolCall,
  ToolApprovalRequestData,
  ToolApprovalResponseData,
  
  // NEW: Progressive events
  ToolExecutionStartEvent,
  ToolExecutionEndEvent,
  ToolQueueUpdateEvent,
  ToolApprovalAnnotatedEvent,
} from './tool/approval.js';
export type {
  // ... existing exports ...
} from './events/interaction-events.js';
```

**Rationale:**
- Makes new types available to consumers
- Maintains clean public API surface

---

### Phase 1 Deliverables

✅ Enhanced type definitions with batch support  
✅ New progressive event types  
✅ Event builders for new events  
✅ Updated exports  
✅ TypeScript compilation passes  
✅ No breaking changes to existing code

### Phase 1 Validation

```bash
cd packages/types
pnpm build
pnpm test

cd sdk
pnpm build
```

---

## Phase 2: Core Coordinator Logic

**Duration:** 5-6 days  
**Priority:** P0 (Core functionality)  
**Risk:** Medium (new logic, but isolated)

### 2.1 Tool Approval Coordinator Enhancement

**File:** `sdk/core/coordinators/tool-approval-coordinator.ts`

**Changes:**
```typescript
export class ToolApprovalCoordinator {
  // KEEP: Existing properties and constructor
  
  // KEEP: Existing processToolApproval method (backward compatibility)
  async processToolApproval(
    params: ExtendedToolApprovalCoordinatorParams
  ): Promise<ToolApprovalResult> {
    // ... existing implementation unchanged ...
  }
  
  // NEW: Batch processing with sequential execution
  async processToolBatch(
    toolCalls: LLMToolCall[],
    options: ToolApprovalOptions,
    contextId: string,
    nodeId: string,
    approvalHandler: ToolApprovalHandler,
    eventManager?: EventRegistry
  ): Promise<ToolBatchResult> {
    const batchId = generateId();
    logger.info('Starting batch tool approval', {
      batchId,
      contextId,
      nodeId,
      totalTools: toolCalls.length
    });
    
    // Step 1: Split into auto-execute prefix and first confirmation tool
    const { autoPrefix, firstConfirmTool, remainingAfterConfirm } = 
      this.splitToolBatch(toolCalls, options, contextId);
    
    logger.debug('Tool batch split', {
      batchId,
      autoCount: autoPrefix.length,
      hasConfirmation: !!firstConfirmTool,
      remainingCount: remainingAfterConfirm.length
    });
    
    // Step 2: Execute auto-approved prefix with progress events
    const autoResults: ToolExecutionResult[] = [];
    for (let i = 0; i < autoPrefix.length; i++) {
      const call = autoPrefix[i];
      
      // Emit start event
      if (eventManager) {
        await safeEmit(eventManager, buildToolExecutionStartEvent({
          executionId: contextId,
          nodeId,
          toolCallId: call.id,
          toolName: call.function?.name || 'unknown',
          batchId,
          toolIndex: i,
          totalTools: toolCalls.length,
          pendingQueue: this.buildPendingQueue(autoPrefix.slice(i + 1), firstConfirmTool, remainingAfterConfirm)
        }));
      }
      
      // Execute tool
      const startTime = now();
      const result = await this.executeAutoTool(call, options, contextId);
      const executionTime = diffTimestamp(startTime, now());
      
      autoResults.push(result);
      
      // Emit end event
      if (eventManager) {
        await safeEmit(eventManager, buildToolExecutionEndEvent({
          executionId: contextId,
          nodeId,
          toolCallId: call.id,
          toolName: call.function?.name || 'unknown',
          batchId,
          status: result.success ? 'success' : 'error',
          result,
          executionTime
        }));
        
        // Emit queue update
        await safeEmit(eventManager, buildToolQueueUpdateEvent({
          executionId: contextId,
          nodeId,
          batchId,
          completedCount: i + 1,
          totalCount: toolCalls.length,
          pendingQueue: this.buildPendingQueue(autoPrefix.slice(i + 1), firstConfirmTool, remainingAfterConfirm)
        }));
      }
    }
    
    // Step 3: If confirmation needed, pause and request approval
    if (firstConfirmTool) {
      const approvalResult = await this.requestUserApprovalForBatch(
        firstConfirmTool,
        options,
        contextId,
        nodeId,
        approvalHandler,
        batchId,
        autoPrefix.length,
        toolCalls.length,
        remainingAfterConfirm,
        autoResults,
        eventManager
      );
      
      logger.info('User approval received', {
        batchId,
        approved: approvalResult.approved,
        continueBatch: approvalResult.continueBatch
      });
      
      // Determine remaining queue based on approval decision
      const finalRemaining = approvalResult.continueBatch 
        ? remainingAfterConfirm 
        : [];
      
      return {
        batchId,
        autoExecuted: autoResults,
        confirmationRequired: firstConfirmTool,
        confirmationResult: approvalResult,
        remainingQueue: finalRemaining,
        allCompleted: !approvalResult.continueBatch || remainingAfterConfirm.length === 0
      };
    }
    
    // All tools auto-executed
    logger.info('All tools auto-executed', { batchId, count: autoResults.length });
    
    return {
      batchId,
      autoExecuted: autoResults,
      confirmationRequired: null,
      remainingQueue: [],
      allCompleted: true
    };
  }
  
  // NEW: Helper - Split batch into auto/confirm/remaining
  private splitToolBatch(
    toolCalls: LLMToolCall[],
    options: ToolApprovalOptions,
    contextId: string
  ): {
    autoPrefix: LLMToolCall[];
    firstConfirmTool: LLMToolCall | null;
    remainingAfterConfirm: LLMToolCall[];
  } {
    const autoPrefix: LLMToolCall[] = [];
    let firstConfirmTool: LLMToolCall | null = null;
    let firstConfirmIndex = -1;
    
    for (let i = 0; i < toolCalls.length; i++) {
      const call = toolCalls[i];
      const tool = this.toolService?.getTool(call.function?.name || '');
      
      if (this.requiresConfirmation(tool, options, contextId)) {
        if (!firstConfirmTool) {
          firstConfirmTool = call;
          firstConfirmIndex = i;
          break;  // Stop at first confirmation-required tool
        }
      } else {
        autoPrefix.push(call);
      }
    }
    
    const remainingAfterConfirm = firstConfirmIndex >= 0
      ? toolCalls.slice(firstConfirmIndex + 1)
      : [];
    
    return { autoPrefix, firstConfirmTool, remainingAfterConfirm };
  }
  
  // NEW: Helper - Check if tool requires confirmation
  private requiresConfirmation(
    tool: Tool | undefined,
    options: ToolApprovalOptions,
    contextId: string
  ): boolean {
    if (!options.autoApprovalEnabled) {
      return true;  // All tools require approval if disabled
    }
    
    // Use existing auto-approval logic
    const decision = checkAutoApproval({
      tool,
      options,
      context: this.extractAutoApprovalContext(tool, contextId)
    });
    
    return decision.decision !== 'approve';
  }
  
  // NEW: Helper - Execute auto-approved tool
  private async executeAutoTool(
    call: LLMToolCall,
    options: ToolApprovalOptions,
    contextId: string
  ): Promise<ToolExecutionResult> {
    // TODO: Integrate with ToolCallExecutor
    // For now, return placeholder - actual execution happens in coordinators
    return {
      toolCallId: call.id,
      toolName: call.function?.name || 'unknown',
      success: true,
      result: {},
      executionTime: 0
    };
  }
  
  // NEW: Helper - Request approval with batch context
  private async requestUserApprovalForBatch(
    toolCall: LLMToolCall,
    options: ToolApprovalOptions,
    contextId: string,
    nodeId: string,
    approvalHandler: ToolApprovalHandler,
    batchId: string,
    toolIndex: number,
    totalTools: number,
    pendingQueue: LLMToolCall[],
    autoExecutedResults: ToolExecutionResult[],
    eventManager?: EventRegistry
  ): Promise<ToolApprovalResult> {
    const interactionId = generateId();
    
    // Create approval request with batch context
    const request: ToolApprovalRequest = {
      toolCall,
      toolDescription: this.getToolDescription(toolCall),
      contextId,
      nodeId,
      interactionId,
      batchId,
      toolIndex,
      totalTools,
      pendingQueue,
      autoExecutedResults
    };
    
    // Delegate to handler
    const result = await approvalHandler.requestApproval(request);
    
    // Emit annotation event if provided
    if (result.annotation && eventManager) {
      await safeEmit(eventManager, buildToolApprovalAnnotatedEvent({
        executionId: contextId,
        nodeId,
        interactionId,
        toolCallId: toolCall.id,
        toolName: toolCall.function?.name || 'unknown',
        annotation: result.annotation,
        approved: result.approved
      }));
    }
    
    return result;
  }
  
  // NEW: Helper - Build pending queue for events
  private buildPendingQueue(
    remainingAuto: LLMToolCall[],
    confirmTool: LLMToolCall | null,
    remainingAfterConfirm: LLMToolCall[]
  ): PendingToolCall[] {
    const queue: PendingToolCall[] = [];
    
    if (confirmTool) {
      queue.push({
        id: confirmTool.id,
        name: confirmTool.function?.name || 'unknown',
        arguments: confirmTool.function?.arguments
      });
    }
    
    queue.push(...remainingAfterConfirm.map(call => ({
      id: call.id,
      name: call.function?.name || 'unknown',
      arguments: call.function?.arguments
    })));
    
    return queue;
  }
  
  // NEW: Helper - Extract context for auto-approval check
  private extractAutoApprovalContext(
    tool: Tool | undefined,
    contextId: string
  ): AutoApprovalContext {
    // TODO: Extract workspace boundary, file paths, domains, etc.
    // For now, return minimal context
    return {
      isOutsideWorkspace: false,
      isProtected: false,
      domain: undefined
    };
  }
  
  // NEW: Helper - Get tool description
  private getToolDescription(toolCall: LLMToolCall): string | undefined {
    const tool = this.toolService?.getTool(toolCall.function?.name || '');
    return tool?.description;
  }
  
  // NEW: Helper - Safe event emission
  private async safeEmit(eventManager: EventRegistry, event: Event): Promise<void> {
    try {
      await emit(eventManager, event);
    } catch (error) {
      logger.warn('Failed to emit event', {
        eventType: event.type,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
```

**Rationale:**
- Implements sequential execution logic from Lim-Code
- Emits progressive events for real-time feedback
- Maintains backward compatibility with single-tool approval
- Handles batch state tracking

---

### 2.2 Usage Limit Tracking Enhancement

**File:** `sdk/core/coordinators/tool-approval-coordinator.ts`

**Changes:**
```typescript
// ENHANCE: ApprovalState interface
interface ApprovalState {
  consecutiveAutoApprovedCount: number;
  lastResetPoint: number;  // Message index or timestamp
  totalAutoApprovedCost?: number;  // Track cumulative cost
  lastApprovalTime?: number;  // For time-based resets
}

// ENHANCE: checkUsageLimits method
private checkUsageLimits(
  state: ApprovalState,
  options: ToolApprovalOptions
): { allowed: boolean; reason?: string } {
  const maxConsecutive = options.maxAutoApprovedRequests ?? Infinity;
  
  if (state.consecutiveAutoApprovedCount >= maxConsecutive) {
    return {
      allowed: false,
      reason: `Exceeded maximum consecutive auto-approvals (${maxConsecutive})`
    };
  }
  
  // TODO: Add cost-based limits
  // TODO: Add time-window-based resets
  
  return { allowed: true };
}

// ENHANCE: Update state after auto-approval
private recordAutoApproval(contextId: string): void {
  const state = this.getOrCreateState(contextId);
  state.consecutiveAutoApprovedCount++;
  state.lastApprovalTime = Date.now();
}

// ENHANCE: Reset state after manual approval
private resetState(contextId: string): void {
  const state = this.getOrCreateState(contextId);
  state.consecutiveAutoApprovedCount = 0;
  state.totalAutoApprovedCost = 0;
}
```

**Rationale:**
- Prevents runaway auto-approval chains
- Provides safety net for unexpected behavior
- Enables future cost-tracking features

---

### Phase 2 Deliverables

✅ `processToolBatch()` method with sequential logic  
✅ Progressive event emission during batch execution  
✅ Enhanced usage limit tracking  
✅ Helper methods for batch splitting and queue management  
✅ Full backward compatibility maintained  
✅ Comprehensive logging for debugging

### Phase 2 Validation

```bash
cd sdk
pnpm build

# Run coordinator tests
pnpm test core/coordinators/tool-approval-coordinator

# Integration test
pnpm test core/coordinators
```

---

## Phase 3: Coordinator Integration

**Duration:** 5-6 days  
**Priority:** P0 (Integration)  
**Risk:** Medium-High (modifies existing coordinators)

### 3.1 LLM Execution Coordinator Integration

**File:** `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`

**Changes:**
```typescript
export class LLMExecutionCoordinator {
  // KEEP: Existing properties
  
  // ADD: Approval coordinator instance
  private approvalCoordinator: ToolApprovalCoordinator;
  
  constructor(
    private contextFactory: LLMContextFactory,
    // ... other params ...
  ) {
    // ... existing initialization ...
    
    // NEW: Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(
      this.contextFactory.getEventManager()
    );
  }
  
  // KEEP: Existing requestToolApproval method (for backward compatibility)
  private async requestToolApproval(
    toolCall: { id: string; name: string; arguments: string },
    approvalConfig: { approvalTimeout?: number } | undefined,
    executionId: string,
    nodeId: string
  ): Promise<ToolApprovalData> {
    // ... existing implementation unchanged ...
  }
  
  // NEW: Batch approval method
  private async requestToolBatchApproval(
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    approvalConfig: { approvalTimeout?: number } | undefined,
    executionId: string,
    nodeId: string,
    conversationState: ConversationSession
  ): Promise<ToolBatchResult> {
    logger.info('Starting batch tool approval', {
      executionId,
      nodeId,
      toolCount: toolCalls.length
    });
    
    // Create approval handler that integrates with event system
    const approvalHandler: ToolApprovalHandler = {
      requestApproval: async (request: ToolApprovalRequest) => {
        // Create checkpoint for long-running approval
        let checkpointId: string | undefined;
        if (this.contextFactory.hasToolApprovalSupport()) {
          checkpointId = await this.createApprovalCheckpoint(
            executionId,
            nodeId,
            request
          );
        }
        
        try {
          // Build rich prompt with batch context
          const prompt = this.buildBatchApprovalPrompt(request);
          
          // Emit USER_INTERACTION_REQUESTED with structured data
          const requestedEvent = buildUserInteractionRequestedEvent({
            executionId,
            nodeId,
            interactionId: request.interactionId,
            operationType: 'TOOL_APPROVAL',
            prompt,
            timeout: approvalConfig?.approvalTimeout || 0,
            contextData: {
              toolData: {
                toolCallId: request.toolCall.id,
                toolName: request.toolCall.function?.name || 'unknown',
                toolDescription: request.toolDescription,
                parameters: JSON.parse(request.toolCall.function?.arguments || '{}'),
                pendingQueue: request.pendingQueue?.map(tc => ({
                  id: tc.id,
                  name: tc.function?.name || 'unknown',
                  arguments: tc.function?.arguments
                })),
                autoExecutedTools: request.autoExecutedResults,
                batchId: request.batchId,
                toolIndex: request.toolIndex,
                totalTools: request.totalTools
              } as ToolApprovalRequestData
            }
          });
          
          await safeEmit(this.contextFactory.getEventManager(), requestedEvent);
          
          // Wait for user response
          const response = await this.waitForUserInteractionResponse(
            request.interactionId,
            approvalConfig?.approvalTimeout || 0
          );
          
          const approvalResult = response.inputData as ToolApprovalResponseData;
          
          // Emit USER_INTERACTION_PROCESSED
          const processedEvent = buildUserInteractionProcessedEvent({
            executionId,
            interactionId: request.interactionId,
            operationType: 'TOOL_APPROVAL',
            results: approvalResult
          });
          
          await safeEmit(this.contextFactory.getEventManager(), processedEvent);
          
          // Convert to ToolApprovalResult format
          return {
            approved: approvalResult.approved,
            toolCallId: request.toolCall.id,
            editedParameters: approvalResult.editedParameters,
            userInstruction: approvalResult.userInstruction,
            annotation: approvalResult.annotation,
            rejectionReason: approvalResult.rejectionReason,
            continueBatch: approvalResult.approved && approvalResult.continueBatch !== false
          };
        } finally {
          // Cleanup checkpoint
          if (checkpointId) {
            await this.cleanupCheckpoint(checkpointId);
          }
        }
      }
    };
    
    // Convert tool calls to LLMToolCall format
    const llmToolCalls: LLMToolCall[] = toolCalls.map(tc => ({
      id: tc.id,
      type: 'function',
      function: {
        name: tc.name,
        arguments: tc.arguments
      }
    }));
    
    // Delegate to approval coordinator
    const batchResult = await this.approvalCoordinator.processToolBatch(
      llmToolCalls,
      this.getApprovalOptions(),
      executionId,
      nodeId,
      approvalHandler,
      this.contextFactory.getEventManager()
    );
    
    logger.info('Batch approval completed', {
      executionId,
      batchId: batchResult.batchId,
      autoExecuted: batchResult.autoExecuted.length,
      confirmationRequired: !!batchResult.confirmationRequired,
      allCompleted: batchResult.allCompleted
    });
    
    return batchResult;
  }
  
  // NEW: Helper - Build rich approval prompt
  private buildBatchApprovalPrompt(request: ToolApprovalRequest): string {
    const toolName = request.toolCall.function?.name || 'unknown';
    let prompt = `Tool "${toolName}" requires approval.\n`;
    
    // Add batch context
    if (request.batchId) {
      prompt += `\n[Batch ${request.batchId.substring(0, 8)}] `;
      prompt += `Tool ${request.toolIndex! + 1} of ${request.totalTools}\n`;
    }
    
    // Add auto-executed context
    if (request.autoExecutedResults && request.autoExecutedResults.length > 0) {
      prompt += `\n✓ Previously auto-executed ${request.autoExecutedResults.length} tool(s):\n`;
      request.autoExecutedResults.forEach((result, i) => {
        const status = result.success ? '✓ Success' : '✗ Failed';
        prompt += `  ${i + 1}. ${result.toolName}: ${status}\n`;
      });
    }
    
    // Add pending queue info
    if (request.pendingQueue && request.pendingQueue.length > 0) {
      prompt += `\n⏳ Pending tools after this: ${request.pendingQueue.length}\n`;
      request.pendingQueue.slice(0, 3).forEach((tc, i) => {
        prompt += `  ${i + 1}. ${tc.function?.name || 'unknown'}\n`;
      });
      if (request.pendingQueue.length > 3) {
        prompt += `  ... and ${request.pendingQueue.length - 3} more\n`;
      }
    }
    
    // Add parameters
    try {
      const params = JSON.parse(request.toolCall.function?.arguments || '{}');
      prompt += `\nParameters:\n${JSON.stringify(params, null, 2)}\n`;
    } catch (e) {
      prompt += `\nParameters: ${request.toolCall.function?.arguments}\n`;
    }
    
    // Add description if available
    if (request.toolDescription) {
      prompt += `\nDescription: ${request.toolDescription}\n`;
    }
    
    return prompt;
  }
  
  // NEW: Helper - Get approval options from config
  private getApprovalOptions(): ToolApprovalOptions {
    // TODO: Read from workflow/node configuration
    // For now, return safe defaults
    return {
      autoApprovalEnabled: false,  // Require approval by default
      maxAutoApprovedRequests: 5
    };
  }
  
  // NEW: Helper - Create checkpoint for approval
  private async createApprovalCheckpoint(
    executionId: string,
    nodeId: string,
    request: ToolApprovalRequest
  ): Promise<string> {
    const dependencies = {
      workflowExecutionRegistry: this.contextFactory.getExecutionRegistry()!,
      checkpointStateManager: this.contextFactory.getCheckpointStateManager()!,
      workflowRegistry: this.contextFactory.getWorkflowRegistry()!,
      workflowGraphRegistry: this.contextFactory.getGraphRegistry()!
    };
    
    const checkpointId = await CheckpointCoordinator.createCheckpoint(
      executionId,
      dependencies,
      {
        description: `Waiting for approval: ${request.toolCall.function?.name}`,
        customFields: {
          toolApprovalState: {
            pendingToolCall: request.toolCall,
            interactionId: request.interactionId,
            batchId: request.batchId,
            toolIndex: request.toolIndex
          }
        }
      }
    );
    
    logger.debug('Created approval checkpoint', {
      checkpointId,
      executionId,
      toolName: request.toolCall.function?.name
    });
    
    return checkpointId;
  }
  
  // NEW: Helper - Cleanup checkpoint
  private async cleanupCheckpoint(checkpointId: string): Promise<void> {
    const checkpointStateManager = this.contextFactory.getCheckpointStateManager();
    if (checkpointStateManager) {
      try {
        await checkpointStateManager.delete(checkpointId);
        logger.debug('Cleaned up approval checkpoint', { checkpointId });
      } catch (error) {
        logger.warn('Failed to cleanup checkpoint', {
          checkpointId,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }
  }
  
  // NEW: Helper - Safe event emission
  private async safeEmit(eventManager: EventRegistry | undefined, event: Event): Promise<void> {
    if (!eventManager) return;
    
    try {
      await emit(eventManager, event);
    } catch (error) {
      logger.debug('Failed to emit event', {
        eventType: event.type,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
```

**Integration Points:**
- Replace direct approval calls with batch approval where appropriate
- Update tool execution flow to handle batch results
- Integrate with existing checkpoint system

---

### 3.2 Agent Execution Coordinator Implementation

**File:** `sdk/agent/execution/coordinators/agent-execution-coordinator.ts`

**Changes:**
```typescript
export class AgentExecutionCoordinator {
  // KEEP: Existing properties
  
  // ADD: Approval coordinator instance
  private approvalCoordinator: ToolApprovalCoordinator;
  
  constructor(
    // ... existing params ...
    private eventManager?: EventRegistry,
    private toolService?: ToolRegistry
  ) {
    // ... existing initialization ...
    
    // NEW: Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(this.eventManager);
  }
  
  // REPLACE: Placeholder implementation with full approval support
  private async requestAgentApproval(
    request: { toolCall: { id: string; function?: { name?: string; arguments?: string } } },
    entity: AgentLoopEntity
  ): Promise<{ 
    approved: boolean; 
    toolCallId: string; 
    editedParameters?: Record<string, unknown>; 
    userInstruction?: string; 
    rejectionReason?: string;
    annotation?: string;
  }> {
    const approvalHandler: ToolApprovalHandler = {
      requestApproval: async (approvalRequest: ToolApprovalRequest) => {
        const interactionId = generateId();
        
        // Emit USER_INTERACTION_REQUESTED
        const requestedEvent = buildUserInteractionRequestedEvent({
          executionId: entity.id,
          nodeId: entity.nodeId,
          interactionId,
          operationType: 'TOOL_APPROVAL',
          prompt: `Agent wants to call tool "${approvalRequest.toolCall.function?.name}"`,
          timeout: this.getApprovalTimeout(entity),
          contextData: {
            toolData: {
              toolCallId: approvalRequest.toolCall.id,
              toolName: approvalRequest.toolCall.function?.name || 'unknown',
              toolDescription: approvalRequest.toolDescription,
              parameters: JSON.parse(approvalRequest.toolCall.function?.arguments || '{}'),
              riskLevel: this.getToolRiskLevel(approvalRequest.toolCall)
            } as ToolApprovalRequestData
          }
        });
        
        await emit(this.eventManager, requestedEvent);
        
        // Wait for response
        const response = await this.waitForAgentInteractionResponse(interactionId);
        
        const approvalResult = response.inputData as ToolApprovalResponseData;
        
        // Emit USER_INTERACTION_PROCESSED
        const processedEvent = buildUserInteractionProcessedEvent({
          executionId: entity.id,
          interactionId,
          operationType: 'TOOL_APPROVAL',
          results: approvalResult
        });
        
        await emit(this.eventManager, processedEvent);
        
        return {
          approved: approvalResult.approved,
          toolCallId: approvalRequest.toolCall.id,
          editedParameters: approvalResult.editedParameters,
          userInstruction: approvalResult.userInstruction,
          annotation: approvalResult.annotation,
          rejectionReason: approvalResult.rejectionReason
        };
      }
    };
    
    // Use coordinator for consistent logic
    const result = await this.approvalCoordinator.processToolApproval({
      toolCall: request.toolCall,
      options: this.getApprovalOptions(entity),
      contextId: entity.id,
      nodeId: entity.nodeId,
      approvalHandler,
      tool: this.toolService?.getTool(request.toolCall.function?.name || '')
    });
    
    return {
      approved: result.approved,
      toolCallId: result.toolCallId,
      editedParameters: result.editedParameters,
      userInstruction: result.userInstruction,
      annotation: result.annotation,
      rejectionReason: result.rejectionReason
    };
  }
  
  // NEW: Batch approval for agent mode
  private async executeToolCallsWithApproval(
    entity: AgentLoopEntity,
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    conversationManager: ConversationSession
  ): Promise<void> {
    const approvalHandler: ToolApprovalHandler = {
      requestApproval: async (request: ToolApprovalRequest) => {
        // Similar to requestAgentApproval above
        // ... implementation ...
      }
    };
    
    const llmToolCalls: LLMToolCall[] = toolCalls.map(tc => ({
      id: tc.id,
      type: 'function',
      function: {
        name: tc.name,
        arguments: tc.arguments
      }
    }));
    
    const batchResult = await this.approvalCoordinator.processToolBatch(
      llmToolCalls,
      this.getApprovalOptions(entity),
      entity.id,
      entity.nodeId,
      approvalHandler,
      this.eventManager
    );
    
    // Execute approved tools
    for (const result of batchResult.autoExecuted) {
      await this.executeSingleTool(entity, result, conversationManager);
    }
    
    if (batchResult.confirmationResult?.approved) {
      // Execute confirmed tool
      if (batchResult.confirmationRequired) {
        await this.executeSingleTool(
          entity,
          {
            toolCallId: batchResult.confirmationRequired.id,
            toolName: batchResult.confirmationRequired.function?.name || 'unknown',
            success: true,
            result: {},
            executionTime: 0
          },
          conversationManager
        );
      }
      
      // Continue with remaining if flag set
      if (batchResult.confirmationResult.continueBatch && batchResult.remainingQueue.length > 0) {
        const remainingCalls = batchResult.remainingQueue.map(tc => ({
          id: tc.id,
          name: tc.function?.name || 'unknown',
          arguments: tc.function?.arguments || '{}'
        }));
        
        await this.executeToolCallsWithApproval(entity, remainingCalls, conversationManager);
      }
    }
  }
  
  // NEW: Helper - Get approval timeout
  private getApprovalTimeout(entity: AgentLoopEntity): number {
    // TODO: Read from agent configuration
    return 300000;  // 5 minutes default
  }
  
  // NEW: Helper - Get tool risk level
  private getToolRiskLevel(toolCall: LLMToolCall): ToolRiskLevel | undefined {
    const tool = this.toolService?.getTool(toolCall.function?.name || '');
    return tool?.riskLevel;
  }
  
  // NEW: Helper - Get approval options
  private getApprovalOptions(entity: AgentLoopEntity): ToolApprovalOptions {
    // TODO: Read from agent configuration
    return {
      autoApprovalEnabled: false,
      maxAutoApprovedRequests: 3
    };
  }
  
  // NEW: Helper - Execute single tool
  private async executeSingleTool(
    entity: AgentLoopEntity,
    result: ToolExecutionResult,
    conversationManager: ConversationSession
  ): Promise<void> {
    // TODO: Integrate with existing tool execution logic
    logger.debug('Executing approved tool', {
      agentLoopId: entity.id,
      toolCallId: result.toolCallId,
      toolName: result.toolName
    });
  }
  
  // NEW: Helper - Wait for agent interaction response
  private async waitForAgentInteractionResponse(
    interactionId: string
  ): Promise<UserInteractionResponse> {
    // TODO: Implement event-based waiting
    // For now, return placeholder
    return new Promise((resolve) => {
      // This will be implemented with event subscription
      setTimeout(() => {
        resolve({
          interactionId,
          inputData: { approved: false, rejectionReason: 'Not implemented' },
          timestamp: Date.now()
        });
      }, 1000);
    });
  }
}
```

**Rationale:**
- Implements previously missing approval logic
- Uses same coordinator for consistency across modes
- Supports both single and batch approval
- No checkpoint support (agent loops are transient)

---

### 3.3 Conversation Session Context Caching

**File:** `sdk/core/services/conversation/conversation-session.ts`

**Changes:**
```typescript
export class ConversationSession {
  // KEEP: Existing properties
  
  // NEW: Turn-based context cache
  private turnContextCache: Map<number, string> = new Map();
  
  // KEEP: Existing methods
  
  // NEW: Get cached dynamic context for a turn
  /**
   * Get cached dynamic context for a specific turn
   * @param turnStartIndex Index of the turn-start user message
   * @returns Cached context text or undefined if not cached
   */
  getTurnDynamicContext(turnStartIndex: number): string | undefined {
    return this.turnContextCache.get(turnStartIndex);
  }
  
  // NEW: Cache dynamic context for a turn
  /**
   * Cache dynamic context for a specific turn
   * @param turnStartIndex Index of the turn-start user message
   * @param context Generated dynamic context text
   */
  setTurnDynamicContext(turnStartIndex: number, context: string): void {
    this.turnContextCache.set(turnStartIndex, context);
    logger.debug('Cached turn dynamic context', {
      turnStartIndex,
      contextLength: context.length
    });
  }
  
  // NEW: Clear cached context from index onwards
  /**
   * Clear cached context from a specific index onwards
   * Used when editing or deleting messages
   * @param index Message index to clear from
   */
  clearTurnContextFromIndex(index: number): void {
    const clearedKeys: number[] = [];
    
    for (const [key] of this.turnContextCache) {
      if (key >= index) {
        this.turnContextCache.delete(key);
        clearedKeys.push(key);
      }
    }
    
    if (clearedKeys.length > 0) {
      logger.debug('Cleared turn context cache', {
        fromIndex: index,
        clearedCount: clearedKeys.length,
        clearedIndices: clearedKeys
      });
    }
  }
  
  // NEW: Clear entire cache
  /**
   * Clear all cached turn contexts
   * Used when resetting conversation
   */
  clearAllTurnContexts(): void {
    const clearedCount = this.turnContextCache.size;
    this.turnContextCache.clear();
    
    if (clearedCount > 0) {
      logger.debug('Cleared all turn context cache', { clearedCount });
    }
  }
  
  // ENHANCE: addMessage to invalidate cache
  addMessage(message: Message): void {
    // ... existing logic ...
    
    // NEW: Clear cache from this point forward
    const messageIndex = this.messages.length;
    this.clearTurnContextFromIndex(messageIndex);
  }
  
  // ENHANCE: deleteMessagesFromIndex to invalidate cache
  deleteMessagesFromIndex(index: number): void {
    // ... existing logic ...
    
    // NEW: Clear cache from deleted point
    this.clearTurnContextFromIndex(index);
  }
  
  // ENHANCE: updateMessage to invalidate cache
  updateMessage(index: number, updates: Partial<Message>): void {
    // ... existing logic ...
    
    // NEW: Clear cache from updated point
    this.clearTurnContextFromIndex(index);
  }
  
  // NEW: Reset cache on conversation reset
  reset(): void {
    // ... existing reset logic ...
    
    // NEW: Clear all cached contexts
    this.clearAllTurnContexts();
  }
}
```

**Rationale:**
- Reduces redundant dynamic context generation
- Automatically invalidated on message changes
- Improves performance for multi-iteration turns
- Simple Map-based implementation (can optimize later)

---

### Phase 3 Deliverables

✅ LLMExecutionCoordinator integrates batch approval  
✅ AgentExecutionCoordinator implements full approval logic  
✅ ConversationSession supports context caching  
✅ Checkpoint integration for long-running approvals  
✅ Rich approval prompts with batch context  
✅ All existing tests pass  
✅ New integration tests for batch approval

### Phase 3 Validation

```bash
cd sdk

# Build all modules
pnpm build

# Run coordinator tests
pnpm test workflow/execution/coordinators
pnpm test agent/execution/coordinators

# Run conversation service tests
pnpm test core/services/conversation

# Integration tests
pnpm test workflow/execution
pnpm test agent/execution
```

---

## Phase 4: CLI Handler Enhancement & Testing

**Duration:** 4-5 days  
**Priority:** P1 (UX improvement)  
**Risk:** Low (isolated to CLI app, but we're only doing SDK-side prep)

### 4.1 Handler Interface Documentation

**File:** `packages/types/src/interaction.ts`

**Add comprehensive JSDoc:**
```typescript
/**
 * User Interaction Handler Interface
 * 
 * Application layers must implement this interface to provide
 * platform-specific user interaction capabilities.
 * 
 * @example CLI Implementation
 * ```typescript
 * class CLIInteractionHandler implements UserInteractionHandler {
 *   async handle(request: UserInteractionRequest, context: UserInteractionContext) {
 *     switch (request.operationType) {
 *       case 'TOOL_APPROVAL':
 *         return this.handleToolApproval(request, context);
 *       // ... other cases
 *     }
 *   }
 * }
 * ```
 * 
 * @example Web/VSCode Implementation
 * ```typescript
 * class WebInteractionHandler implements UserInteractionHandler {
 *   constructor(private websocket: WebSocketClient) {}
 *   
 *   async handle(request: UserInteractionRequest, context: UserInteractionContext) {
 *     // Send to frontend via WebSocket
 *     this.websocket.send('USER_INTERACTION_REQUESTED', request);
 *     
 *     // Wait for response
 *     return await this.websocket.waitForResponse('USER_INTERACTION_RESPONDED');
 *   }
 * }
 * ```
 */
export interface UserInteractionHandler {
  /**
   * Handle a user interaction request
   * 
   * @param request - The interaction request with operation details
   * @param context - Execution context with variable access and helpers
   * @returns User's response data (structure depends on operationType)
   * 
   * @throws {Error} If interaction fails or times out
   */
  handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown>;
}
```

---

### 4.2 Example Handler Template

**File:** `sdk/examples/interaction-handler-template.ts` (NEW)

```typescript
/**
 * Example User Interaction Handler Implementation
 * 
 * This template demonstrates how to implement a UserInteractionHandler
 * for different application layers (CLI, Web, VSCode, etc.)
 */

import type {
  UserInteractionHandler,
  UserInteractionRequest,
  UserInteractionContext,
  ToolApprovalResponseData,
  ToolApprovalRequestData
} from '@wf-agent/types';

/**
 * Example handler showing all supported operation types
 */
export class ExampleInteractionHandler implements UserInteractionHandler {
  async handle(
    request: UserInteractionRequest,
    context: UserInteractionContext
  ): Promise<unknown> {
    switch (request.operationType) {
      case 'TOOL_APPROVAL':
        return this.handleToolApproval(request, context);
      
      case 'UPDATE_VARIABLES':
        return this.handleVariableUpdate(request, context);
      
      case 'ADD_MESSAGE':
        return this.handleAddMessage(request, context);
      
      default:
        throw new Error(`Unsupported operation type: ${request.operationType}`);
    }
  }
  
  /**
   * Handle tool approval request
   * 
   * This is the most common interaction type. The handler should:
   * 1. Display tool information and parameters
   * 2. Show batch context if available (auto-executed tools, pending queue)
   * 3. Prompt user for approve/reject decision
   * 4. Optionally collect annotation/comment
   * 5. Optionally allow parameter editing
   * 6. Return structured approval response
   */
  private async handleToolApproval(
    request: UserInteractionRequest,
    context: UserInteractionContext
  ): Promise<ToolApprovalResponseData> {
    const toolData = request.metadata?.toolData as ToolApprovalRequestData | undefined;
    
    if (!toolData) {
      throw new Error('Tool approval request missing toolData in metadata');
    }
    
    console.log('\n=== TOOL APPROVAL REQUEST ===');
    console.log(`Tool: ${toolData.toolName}`);
    console.log(`Description: ${toolData.toolDescription || 'N/A'}`);
    console.log(`Risk Level: ${toolData.riskLevel || 'UNKNOWN'}`);
    
    // Display batch context
    if (toolData.autoExecutedTools && toolData.autoExecutedTools.length > 0) {
      console.log(`\n✓ Auto-executed ${toolData.autoExecutedTools.length} tool(s):`);
      toolData.autoExecutedTools.forEach((t, i) => {
        console.log(`  ${i + 1}. ${t.toolName}: ${t.success ? 'Success' : 'Failed'}`);
      });
    }
    
    if (toolData.pendingQueue && toolData.pendingQueue.length > 0) {
      console.log(`\n⏳ Pending tools after this: ${toolData.pendingQueue.length}`);
      toolData.pendingQueue.slice(0, 3).forEach((t, i) => {
        console.log(`  ${i + 1}. ${t.name}`);
      });
    }
    
    console.log('\nParameters:');
    console.log(JSON.stringify(toolData.parameters, null, 2));
    console.log(`\n${request.prompt}`);
    
    // TODO: Implement platform-specific UI
    // For CLI: use readline
    // For Web: send to frontend via WebSocket
    // For VSCode: show modal dialog
    
    // Placeholder response
    return {
      approved: false,
      rejectionReason: 'Handler not fully implemented'
    };
  }
  
  /**
   * Handle variable update request
   */
  private async handleVariableUpdate(
    request: UserInteractionRequest,
    context: UserInteractionContext
  ): Promise<Record<string, unknown>> {
    console.log('\n=== VARIABLE UPDATE REQUEST ===');
    console.log(`Prompt: ${request.prompt}`);
    
    if (request.variables) {
      console.log('Variables to update:');
      request.variables.forEach(v => {
        console.log(`  - ${v.variableName} (${v.scope}): ${v.expression}`);
      });
    }
    
    // TODO: Prompt user for input values
    // Return updated variable values
    
    return {};
  }
  
  /**
   * Handle add message request
   */
  private async handleAddMessage(
    request: UserInteractionRequest,
    context: UserInteractionContext
  ): Promise<{ content: string }> {
    console.log('\n=== ADD MESSAGE REQUEST ===');
    console.log(`Prompt: ${request.prompt}`);
    
    if (request.message) {
      console.log(`Message template: ${request.message.contentTemplate}`);
    }
    
    // TODO: Prompt user for message content
    // Return message content
    
    return { content: '' };
  }
}
```

**Rationale:**
- Provides clear implementation guide for app developers
- Shows all operation types and expected responses
- Demonstrates batch context handling
- Includes TODO markers for platform-specific code

---

### 4.3 Comprehensive Test Suite

**File:** `sdk/__tests__/core/coordinators/tool-approval-coordinator.spec.ts` (NEW)

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ToolApprovalCoordinator } from '../../../core/coordinators/tool-approval-coordinator';
import type { 
  ToolApprovalHandler, 
  ToolApprovalRequest,
  ToolApprovalResult,
  LLMToolCall,
  ToolApprovalOptions
} from '@wf-agent/types';
import { EventRegistry } from '../../../core/registry/event-registry';

describe('ToolApprovalCoordinator', () => {
  let coordinator: ToolApprovalCoordinator;
  let eventManager: EventRegistry;
  let mockHandler: ToolApprovalHandler;
  
  beforeEach(() => {
    eventManager = new EventRegistry();
    coordinator = new ToolApprovalCoordinator(eventManager);
    
    mockHandler = {
      requestApproval: vi.fn().mockResolvedValue({
        approved: true,
        toolCallId: 'test-tool-1'
      })
    };
  });
  
  describe('processToolBatch', () => {
    it('should auto-execute all tools if none require approval', async () => {
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('safe_tool_1'),
        createMockToolCall('safe_tool_2')
      ];
      
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: { READ_ONLY: true }
      };
      
      const result = await coordinator.processToolBatch(
        toolCalls,
        options,
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      expect(result.allCompleted).toBe(true);
      expect(result.confirmationRequired).toBeNull();
      expect(result.autoExecuted.length).toBe(2);
      expect(result.remainingQueue.length).toBe(0);
    });
    
    it('should pause at first confirmation-required tool', async () => {
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('safe_tool'),
        createMockToolCall('dangerous_tool'),
        createMockToolCall('another_tool')
      ];
      
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: { READ_ONLY: true, WRITE: false }
      };
      
      const result = await coordinator.processToolBatch(
        toolCalls,
        options,
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      expect(result.allCompleted).toBe(false);
      expect(result.confirmationRequired).toBeDefined();
      expect(result.confirmationRequired?.function?.name).toBe('dangerous_tool');
      expect(result.autoExecuted.length).toBe(1);
      expect(result.remainingQueue.length).toBe(1);
    });
    
    it('should emit progressive events during execution', async () => {
      const emittedEvents: any[] = [];
      
      eventManager.on('TOOL_EXECUTION_START', (event) => {
        emittedEvents.push(event);
      });
      
      eventManager.on('TOOL_EXECUTION_END', (event) => {
        emittedEvents.push(event);
      });
      
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('safe_tool_1'),
        createMockToolCall('safe_tool_2')
      ];
      
      await coordinator.processToolBatch(
        toolCalls,
        { autoApprovalEnabled: true },
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      expect(emittedEvents.length).toBe(4); // 2 start + 2 end events
      expect(emittedEvents[0].type).toBe('TOOL_EXECUTION_START');
      expect(emittedEvents[1].type).toBe('TOOL_EXECUTION_END');
    });
    
    it('should include batch context in approval request', async () => {
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('safe_tool'),
        createMockToolCall('dangerous_tool')
      ];
      
      await coordinator.processToolBatch(
        toolCalls,
        { autoApprovalEnabled: true, categories: { READ_ONLY: true, WRITE: false } },
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      // Verify handler received batch context
      expect(mockHandler.requestApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          batchId: expect.any(String),
          toolIndex: 1,
          totalTools: 2,
          pendingQueue: expect.arrayContaining([expect.any(Object)]),
          autoExecutedResults: expect.arrayContaining([expect.any(Object)])
        })
      );
    });
    
    it('should respect continueBatch flag', async () => {
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('dangerous_tool_1'),
        createMockToolCall('dangerous_tool_2')
      ];
      
      // Mock handler to approve but not continue
      mockHandler.requestApproval = vi.fn().mockResolvedValue({
        approved: true,
        toolCallId: 'dangerous_tool_1',
        continueBatch: false
      });
      
      const result = await coordinator.processToolBatch(
        toolCalls,
        {},
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      expect(result.remainingQueue.length).toBe(0); // Should be empty due to continueBatch=false
    });
    
    it('should track usage limits', async () => {
      const toolCalls: LLMToolCall[] = [
        createMockToolCall('safe_tool_1'),
        createMockToolCall('safe_tool_2'),
        createMockToolCall('safe_tool_3'),
        createMockToolCall('safe_tool_4'),
        createMockToolCall('safe_tool_5'),
        createMockToolCall('safe_tool_6')  // Should trigger limit
      ];
      
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        maxAutoApprovedRequests: 5
      };
      
      const result = await coordinator.processToolBatch(
        toolCalls,
        options,
        'test-context',
        'test-node',
        mockHandler,
        eventManager
      );
      
      // 6th tool should require approval due to limit
      expect(result.confirmationRequired).toBeDefined();
      expect(result.autoExecuted.length).toBe(5);
    });
  });
  
  describe('processToolApproval (backward compatibility)', () => {
    it('should still work for single tool approval', async () => {
      const toolCall: LLMToolCall = createMockToolCall('test_tool');
      
      const result = await coordinator.processToolApproval({
        toolCall,
        options: {},
        contextId: 'test-context',
        nodeId: 'test-node',
        approvalHandler: mockHandler
      });
      
      expect(result.approved).toBe(true);
      expect(result.toolCallId).toBe('test_tool');
    });
  });
});

// Helper function
function createMockToolCall(name: string): LLMToolCall {
  return {
    id: name,
    type: 'function',
    function: {
      name,
      arguments: '{}'
    }
  };
}
```

**File:** `sdk/__tests__/core/services/conversation/conversation-session.spec.ts` (ENHANCE)

```typescript
describe('ConversationSession - Context Caching', () => {
  let session: ConversationSession;
  
  beforeEach(() => {
    session = new ConversationSession('test-workflow', 'test-thread');
  });
  
  it('should cache and retrieve turn context', () => {
    session.setTurnDynamicContext(0, 'cached context');
    
    const retrieved = session.getTurnDynamicContext(0);
    expect(retrieved).toBe('cached context');
  });
  
  it('should return undefined for uncached turn', () => {
    const retrieved = session.getTurnDynamicContext(5);
    expect(retrieved).toBeUndefined();
  });
  
  it('should clear cache from index onwards', () => {
    session.setTurnDynamicContext(0, 'context 0');
    session.setTurnDynamicContext(1, 'context 1');
    session.setTurnDynamicContext(2, 'context 2');
    
    session.clearTurnContextFromIndex(1);
    
    expect(session.getTurnDynamicContext(0)).toBe('context 0');
    expect(session.getTurnDynamicContext(1)).toBeUndefined();
    expect(session.getTurnDynamicContext(2)).toBeUndefined();
  });
  
  it('should clear all contexts', () => {
    session.setTurnDynamicContext(0, 'context 0');
    session.setTurnDynamicContext(1, 'context 1');
    
    session.clearAllTurnContexts();
    
    expect(session.getTurnDynamicContext(0)).toBeUndefined();
    expect(session.getTurnDynamicContext(1)).toBeUndefined();
  });
  
  it('should invalidate cache when adding message', () => {
    session.setTurnDynamicContext(0, 'context 0');
    session.setTurnDynamicContext(1, 'context 1');
    
    session.addMessage({ role: 'user', content: 'new message' });
    
    // Cache from index 1 onwards should be cleared
    expect(session.getTurnDynamicContext(0)).toBe('context 0');
    expect(session.getTurnDynamicContext(1)).toBeUndefined();
  });
  
  it('should invalidate cache when deleting messages', () => {
    session.setTurnDynamicContext(0, 'context 0');
    session.setTurnDynamicContext(1, 'context 1');
    session.setTurnDynamicContext(2, 'context 2');
    
    session.deleteMessagesFromIndex(1);
    
    expect(session.getTurnDynamicContext(0)).toBe('context 0');
    expect(session.getTurnDynamicContext(1)).toBeUndefined();
    expect(session.getTurnDynamicContext(2)).toBeUndefined();
  });
});
```

---

### Phase 4 Deliverables

✅ Comprehensive JSDoc for handler interface  
✅ Example handler template for reference  
✅ Unit tests for batch approval logic  
✅ Unit tests for context caching  
✅ Integration tests for coordinator workflows  
✅ Event emission validation tests  
✅ 90%+ code coverage on new code

### Phase 4 Validation

```bash
cd sdk

# Run all tests
pnpm test

# Check coverage
pnpm test --coverage

# Verify coverage targets
# - tool-approval-coordinator: >90%
# - conversation-session: >90%
# - interaction-events: 100%
```

---

## Migration Guide for Existing Code

### For SDK Consumers

**No changes required!** All enhancements are backward compatible.

Optional migration to use new features:

```typescript
// OLD: Single tool approval
const result = await coordinator.processToolApproval({
  toolCall,
  options,
  contextId,
  nodeId,
  approvalHandler
});

// NEW: Batch approval (recommended for multiple tools)
const batchResult = await coordinator.processToolBatch(
  toolCalls,
  options,
  contextId,
  nodeId,
  approvalHandler,
  eventManager  // Optional: for progressive events
);

// Handle batch result
for (const autoResult of batchResult.autoExecuted) {
  console.log(`Auto-executed: ${autoResult.toolName}`);
}

if (batchResult.confirmationRequired) {
  console.log(`User approved: ${batchResult.confirmationRequired.function?.name}`);
  
  if (batchResult.confirmationResult?.annotation) {
    console.log(`Annotation: ${batchResult.confirmationResult.annotation}`);
  }
}
```

### For Handler Implementers

Update your handler to support structured tool data:

```typescript
// OLD: Generic metadata access
const toolInfo = request.metadata?.toolInfo;

// NEW: Type-safe structured data
const toolData = request.metadata?.toolData as ToolApprovalRequestData;

if (toolData) {
  console.log(`Tool: ${toolData.toolName}`);
  console.log(`Batch position: ${toolData.toolIndex} / ${toolData.totalTools}`);
  
  if (toolData.autoExecutedTools) {
    console.log(`Previously executed: ${toolData.autoExecutedTools.length}`);
  }
}

// Return with annotation
return {
  approved: true,
  annotation: 'Approved for testing purposes',  // NEW field
  continueBatch: true  // NEW field
};
```

---

## Risk Assessment & Mitigation

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Breaking existing integrations | High | Low | All changes additive, extensive backward compatibility tests |
| Performance degradation from events | Medium | Low | Async event emission, graceful failure on event errors |
| Context cache memory leak | Medium | Low | Automatic invalidation on message changes, clear API |
| Complex batch logic bugs | High | Medium | Comprehensive test suite, phased rollout |
| Handler implementation complexity | Low | Medium | Provide templates and examples, clear documentation |

---

## Success Metrics

### Technical Metrics
- ✅ All existing tests pass (100%)
- ✅ New test coverage >90% for modified modules
- ✅ Zero breaking changes (validated by integration tests)
- ✅ Build time increase <5%
- ✅ Runtime overhead <2% (measured via benchmarks)

### Feature Metrics
- ✅ Sequential tool execution works correctly
- ✅ Progressive events emitted during batch execution
- ✅ Annotations captured and stored
- ✅ Context cache reduces redundant computation by >50%
- ✅ Usage limits prevent runaway auto-approval

### Developer Experience
- ✅ Handler template provides clear implementation guide
- ✅ Migration guide enables easy adoption
- ✅ JSDoc covers all new APIs
- ✅ Example code compiles and runs without errors

---

## Next Steps After Completion

1. **Phase 5 (Future):** Implement message edit/retry handlers
2. **Phase 6 (Future):** Add checkpoint-aware deletion
3. **Phase 7 (Future):** Implement window management utilities
4. **App Layer Integration:** Update CLI, Web, and VSCode apps to use new features
5. **Documentation:** Create user-facing docs for approval workflows
6. **Monitoring:** Add metrics for approval rates, timeouts, annotations

---

## Appendix: File Change Summary

### Packages (`packages/`)

| File | Changes | Lines Changed |
|------|---------|---------------|
| `types/src/tool/approval.ts` | Enhanced interfaces | +80 |
| `types/src/interaction.ts` | Enhanced interfaces | +40 |
| `types/src/events/interaction-events.ts` | New event types | +60 |
| `types/src/events/base.ts` | New event type strings | +5 |
| `types/src/index.ts` | New exports | +15 |

### SDK (`sdk/`)

| File | Changes | Lines Changed |
|------|---------|---------------|
| `core/coordinators/tool-approval-coordinator.ts` | Batch processing logic | +350 |
| `core/utils/event/builders/interaction-events.ts` | New event builders | +30 |
| `workflow/execution/coordinators/llm-execution-coordinator.ts` | Batch approval integration | +250 |
| `agent/execution/coordinators/agent-execution-coordinator.ts` | Full approval implementation | +200 |
| `core/services/conversation/conversation-session.ts` | Context caching | +80 |
| `examples/interaction-handler-template.ts` | New example file | +150 |
| `__tests__/core/coordinators/tool-approval-coordinator.spec.ts` | New test file | +200 |
| `__tests__/core/services/conversation/conversation-session.spec.ts` | Enhanced tests | +60 |

**Total Estimated Changes:** ~1,520 lines added/modified

---

*Document Version: 1.0*  
*Last Updated: 2026-05-06*  
*Author: AI Assistant*  
*Review Status: Pending*
