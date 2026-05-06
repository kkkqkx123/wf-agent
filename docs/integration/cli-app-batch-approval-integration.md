# CLI App Integration Analysis for Batch Approval & Context Caching

## Overview

This document analyzes how the CLI app should correctly integrate the new batch approval and context caching features from Phase 4 of the user interaction enhancement plan.

## Current State

### What's Already Implemented in SDK

1. **ToolApprovalCoordinator** (`sdk/core/coordinators/tool-approval-coordinator.ts`)
   - ✅ `processToolBatch()` method with full batch processing logic
   - ✅ Sequential auto-approval + pause at first confirmation-required tool
   - ✅ Progressive event emission (PROGRESSIVE_TOOL_EXECUTION_START/END)
   - ✅ Batch state management (batchId, queue tracking)
   - ✅ Comprehensive unit tests (19 test cases)

2. **ConversationSession** (`sdk/core/messaging/conversation-session.ts`)
   - ✅ Turn-based context caching (`setTurnDynamicContext`, `getTurnDynamicContext`)
   - ✅ Automatic cache invalidation on message operations
   - ✅ Cache cleanup utilities (`clearTurnContextFromIndex`, `clearAllTurnContexts`)
   - ✅ Comprehensive unit tests (19 test cases)

3. **AgentExecutionCoordinator** (`sdk/agent/execution/coordinators/agent-execution-coordinator.ts`)
   - ✅ Already calls `approvalCoordinator.processToolBatch()` at line 864
   - ⚠️ BUT `requestAgentApproval()` is NOT IMPLEMENTED (line 1061-1080)
   - Currently returns rejection with message: "Interactive approval not yet implemented"

### Current CLI App Architecture

```
apps/cli-app/src/
├── adapters/
│   ├── agent-loop-adapter.ts          # High-level adapter for Agent Loop
│   └── base-adapter.ts                # Base adapter with error handling
├── commands/agent/
│   └── index.ts                       # CLI commands: run, start, pause, resume
├── handlers/
│   └── cli-human-relay-handler.ts     # Example handler pattern (Human Relay)
└── index.ts                           # Main entry point, registers HumanRelayHandler
```

**Key Finding**: The CLI app currently has NO tool approval handler implementation. When tools require approval, they are rejected by default.

## Integration Requirements

### 1. Implement ToolApprovalHandler for CLI

**Location**: Create new file `apps/cli-app/src/handlers/cli-tool-approval-handler.ts`

**Pattern to Follow**: Similar to `CLIHumanRelayHandler` but for tool approval

**Implementation Steps**:

```typescript
// apps/cli-app/src/handlers/cli-tool-approval-handler.ts

import type { 
  ToolApprovalHandler, 
  ToolApprovalRequest, 
  ToolApprovalResponse 
} from "@wf-agent/types";
import { getOutput } from "../utils/output.js";
import readline from "readline";

export class CLIToolApprovalHandler implements ToolApprovalHandler {
  private output = getOutput();

  async requestApproval(request: ToolApprovalRequest): Promise<ToolApprovalResponse> {
    // 1. Display tool information
    this.output.infoLog("\n╔════════════════════════════════════════════════════════════╗");
    this.output.infoLog("║                 TOOL APPROVAL REQUEST                      ║");
    this.output.infoLog("╚════════════════════════════════════════════════════════════╝");
    
    // 2. Show batch context if available
    if (request.batchId) {
      this.output.infoLog(`\nBatch ID: ${request.batchId}`);
      this.output.infoLog(`Tool ${request.toolIndex + 1} of ${request.totalTools}`);
      
      if (request.pendingQueue && request.pendingQueue.length > 0) {
        this.output.infoLog(`\nRemaining tools in queue:`);
        request.pendingQueue.forEach((tc, idx) => {
          const name = tc.function?.name || 'unknown';
          this.output.infoLog(`  ${idx + 1}. ${name}`);
        });
      }
    }
    
    // 3. Show tool details
    this.output.infoLog(`\nTool Name: ${request.toolCall.function?.name || 'unknown'}`);
    this.output.infoLog(`Tool Call ID: ${request.toolCall.id}`);
    
    try {
      const args = JSON.parse(request.toolCall.function?.arguments || '{}');
      this.output.infoLog(`\nParameters:`);
      this.output.infoLog(JSON.stringify(args, null, 2));
    } catch (e) {
      this.output.infoLog(`Arguments: ${request.toolCall.function?.arguments}`);
    }
    
    // 4. Show risk assessment if available
    if (request.riskAssessment) {
      this.output.infoLog(`\nRisk Level: ${request.riskAssessment.riskLevel}`);
      if (request.riskAssessment.reason) {
        this.output.infoLog(`Reason: ${request.riskAssessment.reason}`);
      }
    }
    
    // 5. Prompt user for decision
    this.output.infoLog("\n--- Approve this tool? [y/n/edit/skip] ---");
    this.output.infoLog("  y = approve and continue");
    this.output.infoLog("  n = reject and stop");
    this.output.infoLog("  edit = modify parameters");
    this.output.infoLog("  skip = skip this tool, continue with next");
    
    const decision = await this.promptUser();
    
    // 6. Process decision
    switch (decision.toLowerCase()) {
      case 'y':
      case 'yes':
        return {
          approved: true,
          toolCallId: request.toolCall.id,
          continueBatch: true,
        };
        
      case 'n':
      case 'no':
        return {
          approved: false,
          toolCallId: request.toolCall.id,
          continueBatch: false,
          rejectionReason: "User rejected",
        };
        
      case 'edit':
        const editedParams = await this.promptEditParameters(request.toolCall.function?.arguments);
        return {
          approved: true,
          toolCallId: request.toolCall.id,
          editedParameters: editedParams,
          continueBatch: true,
        };
        
      case 'skip':
        return {
          approved: false,
          toolCallId: request.toolCall.id,
          continueBatch: true, // Skip but continue batch
        };
        
      default:
        return {
          approved: false,
          toolCallId: request.toolCall.id,
          continueBatch: false,
          rejectionReason: "Invalid response",
        };
    }
  }
  
  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });
      
      rl.question("> ", answer => {
        rl.close();
        resolve(answer.trim());
      });
      
      rl.on("SIGINT", () => {
        rl.close();
        reject(new Error("User cancelled"));
      });
    });
  }
  
  private async promptEditParameters(currentArgs: string | undefined): Promise<Record<string, unknown>> {
    this.output.infoLog("\n--- Enter new parameters as JSON ---");
    this.output.infoLog(`Current: ${currentArgs || '{}'}`);
    
    const jsonInput = await this.promptUser();
    
    try {
      return JSON.parse(jsonInput);
    } catch (e) {
      this.output.errorLog("Invalid JSON, using original parameters");
      return currentArgs ? JSON.parse(currentArgs) : {};
    }
  }
}
```

### 2. Register Handler in CLI Entry Point

**Location**: Modify `apps/cli-app/src/index.ts`

**Current Code** (around line 149-152):
```typescript
// 8. Register Human Relay Handler
const humanRelayHandler = new CLIHumanRelayHandler();
sdk.humanRelay.registerHandler(humanRelayHandler);
```

**Add After**:
```typescript
// 9. Register Tool Approval Handler
const toolApprovalHandler = new CLIToolApprovalHandler();
sdk.toolApproval.registerHandler(toolApprovalHandler);
```

**Note**: Need to verify that `sdk.toolApproval` API exists. If not, need to add it to SDK.

### 3. Connect Handler to AgentExecutionCoordinator

**Problem**: The `AgentExecutionCoordinator.requestAgentApproval()` method (line 1061) currently rejects all requests.

**Solution**: The coordinator needs access to the registered handler.

**Two Options**:

#### Option A: Pass Handler via Dependencies (Recommended)

Modify `AgentLoopExecutor` constructor to accept optional handler:

```typescript
// sdk/agent/execution/executors/agent-loop-executor.ts

export interface AgentLoopExecutorDependencies {
  llmExecutor: LLMExecutor;
  toolService: ToolRegistry;
  eventManager?: EventRegistry;
  emitEvent?: (event: AgentHookTriggeredEvent) => Promise<void>;
  toolApprovalHandler?: import("@wf-agent/types").ToolApprovalHandler; // NEW
}

export class AgentLoopExecutor {
  private toolApprovalHandler?: import("@wf-agent/types").ToolApprovalHandler;
  
  constructor(deps: AgentLoopExecutorDependencies) {
    this.toolApprovalHandler = deps.toolApprovalHandler;
  }
  
  private createCoordinator(): AgentExecutionCoordinator {
    return new AgentExecutionCoordinator({
      llmExecutor: this.llmExecutor,
      toolCallExecutor: this.toolCallExecutor,
      emitAgentEvent: this.emitAgentEvent.bind(this),
      eventManager: this.eventManager,
      toolService: this.toolService,
      toolApprovalHandler: this.toolApprovalHandler, // PASS TO COORDINATOR
    });
  }
}
```

Then modify `AgentExecutionCoordinator`:

```typescript
// sdk/agent/execution/coordinators/agent-execution-coordinator.ts

interface AgentExecutionCoordinatorDependencies {
  // ... existing fields ...
  toolApprovalHandler?: import("@wf-agent/types").ToolApprovalHandler;
}

private async requestAgentApproval(
  request: { toolCall: { id: string; function?: { name?: string; arguments?: string } } },
  entity: AgentLoopEntity,
): Promise<{ approved: boolean; toolCallId: string; ... }> {
  // USE REGISTERED HANDLER IF AVAILABLE
  if (this.toolApprovalHandler) {
    try {
      return await this.toolApprovalHandler.requestApproval({
        toolCall: request.toolCall,
        // Add other required fields from ToolApprovalRequest
        batchId: undefined, // Will be set by processToolBatch
        toolIndex: 0,
        totalTools: 1,
        pendingQueue: [],
        contextId: entity.id,
        nodeId: entity.nodeId || "unknown",
      });
    } catch (error) {
      logger.error("Tool approval handler failed", { error });
      return {
        approved: false,
        toolCallId: request.toolCall.id,
        rejectionReason: `Handler error: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }
  
  // Fallback to current behavior
  logger.warn("No tool approval handler configured, rejecting by default", {
    agentLoopId: entity.id,
    toolCallId: request.toolCall.id,
  });
  
  return {
    approved: false,
    toolCallId: request.toolCall.id,
    rejectionReason: "No approval handler configured",
  };
}
```

#### Option B: Global Registry Pattern

Create a global registry similar to HumanRelay:

```typescript
// sdk/core/registry/tool-approval-registry.ts

export class ToolApprovalRegistry {
  private static instance: ToolApprovalRegistry;
  private handler?: import("@wf-agent/types").ToolApprovalHandler;
  
  static getInstance(): ToolApprovalRegistry {
    if (!ToolApprovalRegistry.instance) {
      ToolApprovalRegistry.instance = new ToolApprovalRegistry();
    }
    return ToolApprovalRegistry.instance;
  }
  
  registerHandler(handler: import("@wf-agent/types").ToolApprovalHandler): void {
    this.handler = handler;
  }
  
  getHandler(): import("@wf-agent/types").ToolApprovalHandler | undefined {
    return this.handler;
  }
}
```

Then in `AgentExecutionCoordinator`:

```typescript
private async requestAgentApproval(...) {
  const registry = ToolApprovalRegistry.getInstance();
  const handler = registry.getHandler();
  
  if (handler) {
    return await handler.requestApproval(...);
  }
  
  // Fallback...
}
```

**Recommendation**: Option A is cleaner and more testable. Option B is easier to integrate but creates hidden dependencies.

### 4. Update CLI Adapter to Pass Handler

**Location**: `apps/cli-app/src/adapters/agent-loop-adapter.ts`

**Current Constructor** (lines 31-46):
```typescript
constructor() {
  super();
  this.registry = new AgentLoopRegistry();
  this.eventRegistry = new EventRegistry();

  const llmWrapper = new LLMWrapper(this.eventRegistry);
  const llmExecutor = new LLMExecutor(llmWrapper);
  const toolRegistry = new ToolRegistry();
  const executor = new AgentLoopExecutor({
    llmExecutor,
    toolService: toolRegistry,
    eventManager: this.eventRegistry,
  });
  this.coordinator = new AgentLoopCoordinator(this.registry, executor);
}
```

**Modified**:
```typescript
import { CLIToolApprovalHandler } from "../handlers/cli-tool-approval-handler.js";

constructor() {
  super();
  this.registry = new AgentLoopRegistry();
  this.eventRegistry = new EventRegistry();

  const llmWrapper = new LLMWrapper(this.eventRegistry);
  const llmExecutor = new LLMExecutor(llmWrapper);
  const toolRegistry = new ToolRegistry();
  const toolApprovalHandler = new CLIToolApprovalHandler(); // CREATE HANDLER
  
  const executor = new AgentLoopExecutor({
    llmExecutor,
    toolService: toolRegistry,
    eventManager: this.eventRegistry,
    toolApprovalHandler, // PASS HANDLER
  });
  this.coordinator = new AgentLoopCoordinator(this.registry, executor);
}
```

### 5. Integrate Context Caching

**Location**: Already integrated! The `AgentLoopEntity` uses `ConversationSession` which already has context caching.

**Verification**: Check that context caching is being used properly:

```typescript
// In agent execution loop, after each iteration:

// Get cached context for current turn
const turnIndex = entity.state.currentIteration;
const cachedContext = conversationManager.getTurnDynamicContext(turnIndex);

if (cachedContext) {
  logger.debug("Using cached dynamic context", { turnIndex });
  // Use cached context instead of regenerating
} else {
  // Generate new context
  const dynamicContext = await this.buildDynamicContext(...);
  
  // Cache it for future turns
  conversationManager.setTurnDynamicContext(turnIndex, dynamicContext);
}
```

**Current Status**: The caching infrastructure is in place, but the agent execution loop needs to actually use it. This is a separate optimization task.

## Integration Checklist

### Phase 1: Basic Handler Implementation
- [ ] Create `CLIToolApprovalHandler` class
- [ ] Implement interactive prompts (approve/reject/edit/skip)
- [ ] Handle batch context display
- [ ] Add parameter editing support

### Phase 2: SDK Integration
- [ ] Add `toolApprovalHandler` to `AgentLoopExecutorDependencies`
- [ ] Pass handler to `AgentExecutionCoordinator`
- [ ] Update `requestAgentApproval()` to use handler
- [ ] Add fallback behavior when no handler configured

### Phase 3: CLI Registration
- [ ] Update `AgentLoopAdapter` to create and pass handler
- [ ] Test with simple agent loop execution
- [ ] Verify progressive events are emitted

### Phase 4: Context Caching Usage
- [ ] Identify where dynamic context is generated in agent loop
- [ ] Add cache lookup before generation
- [ ] Add cache storage after generation
- [ ] Verify cache invalidation works correctly

### Phase 5: Testing
- [ ] Manual test: Run agent with tools requiring approval
- [ ] Test batch scenario: Multiple tools in one iteration
- [ ] Test edit scenario: Modify tool parameters
- [ ] Test skip scenario: Skip tool, continue batch
- [ ] Verify context caching improves performance

## Potential Issues & Solutions

### Issue 1: SDK API May Not Expose Handler Registration

**Problem**: The SDK may not have a `sdk.toolApproval.registerHandler()` method.

**Solution**: Use dependency injection through `AgentLoopExecutor` instead (Option A above).

### Issue 2: Blocking Prompts in Async Execution

**Problem**: CLI prompts block the event loop, which may interfere with streaming.

**Solution**: 
- For sync mode (`executeAgentLoop`): Blocking is acceptable
- For stream mode (`executeAgentLoopStream`): Consider non-blocking approach or disable interactive approval in stream mode

### Issue 3: Headless Mode Compatibility

**Problem**: CLI prompts don't work in headless/non-interactive mode.

**Solution**: Detect TTY availability:
```typescript
if (!process.stdin.isTTY) {
  // Auto-reject or use default policy
  return {
    approved: false,
    toolCallId: request.toolCall.id,
    rejectionReason: "Non-interactive mode, approval not available",
  };
}
```

### Issue 4: Timeout Handling

**Problem**: User may not respond to prompts.

**Solution**: Implement timeout with readline:
```typescript
const timeout = setTimeout(() => {
  rl.close();
  reject(new Error("Approval timeout"));
}, request.timeout || 30000);

rl.question("> ", answer => {
  clearTimeout(timeout);
  resolve(answer.trim());
});
```

## Next Steps

1. **Immediate**: Implement `CLIToolApprovalHandler` (Phase 1)
2. **Short-term**: Integrate handler into SDK execution flow (Phase 2-3)
3. **Medium-term**: Optimize with context caching (Phase 4)
4. **Long-term**: Add WebSocket/Web UI handlers for remote approval

## References

- Phase 4 Plan: `docs/plan/user-interaction-enhancement-plan.md`
- ToolApprovalCoordinator Tests: `sdk/core/coordinators/__tests__/tool-approval-coordinator.test.ts`
- ConversationSession Tests: `sdk/core/messaging/__tests__/conversation-session.test.ts`
- HumanRelayHandler Pattern: `apps/cli-app/src/handlers/cli-human-relay-handler.ts`
- AgentExecutionCoordinator: `sdk/agent/execution/coordinators/agent-execution-coordinator.ts`
