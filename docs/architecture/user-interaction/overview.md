# User Interaction Architecture

## Overview

The user interaction system provides a flexible, event-driven architecture for pausing workflow execution to collect user input. It supports three primary operation types: tool approval, message addition, and variable updates.

## Architecture Layers

```
┌─────────────────────────────────────────────────┐
│           Application Layer (UI/CLI)            │
│  - CLI Handler (readline interface)             │
│  - Web Frontend (Vue/Svelte components)         │
│  - VSCode Extension (Webview dialogs)           │
└──────────────────┬──────────────────────────────┘
                   │ implements UserInteractionHandler
┌──────────────────▼──────────────────────────────┐
│              SDK Coordination Layer             │
│  - ToolApprovalCoordinator                      │
│  - UserInteractionNodeHandler                   │
│  - Event System (USER_INTERACTION_*)            │
└──────────────────┬──────────────────────────────┘
                   │ emits events
┌──────────────────▼──────────────────────────────┐
│          Core Execution Infrastructure          │
│  - Checkpoint System (state persistence)        │
│  - Conversation Manager                         │
│  - Variable Management                          │
└─────────────────────────────────────────────────┘
```

## Core Components

### 1. Type Definitions

Located in `packages/types/src/interaction.ts`:

**Operation Types:**
- `UPDATE_VARIABLES` - Update workflow variables based on user input
- `ADD_MESSAGE` - Add user messages to LLM conversations
- `TOOL_APPROVAL` - Approve or reject tool calls

**Key Interfaces:**

```typescript
interface UserInteractionRequest {
  interactionId: ID;
  operationType: UserInteractionOperationType;
  variables?: VariableUpdateConfig[];  // For UPDATE_VARIABLES
  message?: MessageConfig;              // For ADD_MESSAGE
  prompt: string;                       // Display text for user
  timeout: number;                      // Timeout in milliseconds
  metadata?: Metadata;
}

interface UserInteractionContext {
  executionId: ID;
  workflowId: ID;
  nodeId: ID;
  getVariable(name: string, scope?: VariableScope): unknown;
  setVariable(name: string, value: unknown, scope?: VariableScope): Promise<void>;
  getVariables(scope?: VariableScope): Record<string, unknown>;
  timeout: number;
  cancelToken: { cancelled: boolean; cancel(): void };
}

interface UserInteractionHandler {
  handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown>;
}
```

### 2. Event System

Four lifecycle events manage interactions (`packages/types/src/events/interaction-events.ts`):

1. **USER_INTERACTION_REQUESTED** - Triggered when user input is needed
2. **USER_INTERACTION_RESPONDED** - User provides input
3. **USER_INTERACTION_PROCESSED** - Input processed successfully
4. **USER_INTERACTION_FAILED** - Interaction failed or timed out

Event builders are located in `sdk/core/utils/event/builders/interaction-events.ts`.

### 3. Tool Approval System

#### Approval Coordinator (`sdk/core/coordinators/tool-approval-coordinator.ts`)

The coordinator manages the complete approval flow:

1. **Usage Limit Check** - Prevents excessive auto-approvals
2. **Auto-Approval Evaluation** - Checks security presets and rules
3. **Manual Approval Request** - Prompts user if auto-approval fails
4. **Result Processing** - Applies edited parameters and instructions

#### Auto-Approval Engine (`sdk/services/auto-approval/auto-approval-checker.ts`)

Evaluates tools based on:
- **Risk Level**: READ_ONLY, WRITE, COMMAND, NETWORK, MCP, INTERACTION
- **Security Presets**: SAFE, BALANCED, PERMISSIVE
- **Workspace Boundaries**: Allow/deny operations outside workspace
- **File Permissions**: Protected file access controls
- **Command Lists**: Allowed/denied command prefixes
- **Network Domains**: Domain whitelisting/blacklisting
- **MCP Servers**: Server-specific approval rules

Configuration options in `packages/types/src/tool/approval.ts`:

```typescript
interface ToolApprovalOptions {
  autoApprovalEnabled?: boolean;
  securityPreset?: SecurityPreset;
  filePermissions?: FilePermissionSettings;
  categories?: Partial<Record<AutoApprovalCategory, boolean>>;
  workspaceBoundary?: WorkspaceBoundarySettings;
  command?: CommandExecutionSettings;
  network?: NetworkSettings;
  mcp?: McpApprovalSettings;
  maxAutoApprovedRequests?: number;
  approvalTimeout?: number;
}
```

## Execution Flow

### Workflow Mode

In `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`:

```typescript
async requestToolApproval(toolCall, approvalConfig, executionId, nodeId) {
  const interactionId = generateId();
  
  // 1. Create checkpoint for state persistence
  let checkpointId;
  if (this.contextFactory.hasToolApprovalSupport()) {
    checkpointId = await CheckpointCoordinator.createCheckpoint(
      executionId, dependencies, {
        description: "Waiting for tool approval",
        customFields: { toolApprovalState: { pendingToolCall, interactionId } }
      }
    );
  }
  
  try {
    // 2. Emit USER_INTERACTION_REQUESTED event
    await emit(eventManager, buildUserInteractionRequestedEvent({
      executionId, nodeId, interactionId,
      operationType: "TOOL_APPROVAL",
      prompt: `Do you approve calling the tool "${toolCall.id}"?`,
      timeout: approvalConfig?.approvalTimeout || 0
    }));
    
    // 3. Wait for user response (Promise-based)
    const response = await this.waitForUserInteractionResponse(
      interactionId, approvalConfig?.approvalTimeout || 0
    );
    
    // 4. Parse approval result
    const approvalResult = response.inputData as ToolApprovalData;
    
    // 5. Emit USER_INTERACTION_PROCESSED event
    await emit(eventManager, buildUserInteractionProcessedEvent({
      executionId, interactionId,
      operationType: "TOOL_APPROVAL",
      results: approvalResult
    }));
    
    return approvalResult;
  } finally {
    // 6. Cleanup checkpoint
    if (checkpointId) {
      await checkpointStateManager.delete(checkpointId);
    }
  }
}
```

### Agent Mode

In `sdk/agent/execution/coordinators/agent-execution-coordinator.ts`:

```typescript
async executeToolCalls(entity, conversationManager, toolCalls) {
  const approvedToolCalls = [];
  
  for (const toolCall of toolCalls) {
    const tool = this.toolService?.getTool(toolCall.name);
    const approvalOptions = this.getApprovalOptions(entity);
    
    // Process through approval coordinator
    const approvalResult = await this.approvalCoordinator.processToolApproval({
      toolCall: { id, type: "function", function: { name, arguments } },
      tool,
      options: approvalOptions,
      contextId: entity.id,
      nodeId: entity.nodeId,
      approvalHandler: {
        requestApproval: async (request) => {
          return this.requestAgentApproval(request, entity);
        }
      }
    });
    
    if (!approvalResult.approved) {
      // Record rejection and skip
      continue;
    }
    
    // Apply edited parameters if provided
    if (approvalResult.editedParameters) {
      toolCall.arguments = JSON.stringify(approvalResult.editedParameters);
    }
    
    // Add user instruction if provided
    if (approvalResult.userInstruction) {
      conversationManager.addMessage({
        role: "user",
        content: approvalResult.userInstruction
      });
    }
    
    approvedToolCalls.push(toolCall);
  }
  
  // Execute only approved tool calls
  await this.executeApprovedToolCalls(approvedToolCalls, ...);
}
```

## Timeout & Cancellation

Implemented in `sdk/workflow/execution/handlers/node-handlers/user-interaction-handler.ts`:

```typescript
async function getUserInput(request, context, handler) {
  // Timeout control
  const timeoutPromise = new Promise((_, reject) => {
    setTimeout(() => {
      reject(new Error(`User interaction timeout after ${request.timeout}ms`));
    }, request.timeout);
  });
  
  // Cancel token control
  const cancelPromise = new Promise((_, reject) => {
    const checkCancel = setInterval(() => {
      if (context.cancelToken.cancelled) {
        clearInterval(checkCancel);
        reject(new Error("User interaction cancelled"));
      }
    }, 100);
  });
  
  try {
    // Three-way race: user input vs timeout vs cancel
    return await Promise.race([
      handler.handle(request, context),
      timeoutPromise,
      cancelPromise
    ]);
  } finally {
    context.cancelToken.cancel(); // Cleanup
  }
}
```

## Application Layer Implementations

### CLI Implementation

Located in `apps/cli-app/src/handlers/cli-human-relay-handler.ts`:

```typescript
export class CLIHumanRelayHandler implements HumanRelayHandler {
  async handle(request: HumanRelayRequest, context: HumanRelayContext) {
    // Display formatted request
    output.infoLog("HUMAN RELAY REQUEST");
    output.infoLog(`Request ID: ${request.requestId}`);
    output.infoLog(`Timeout: ${request.timeout}ms`);
    
    // Show conversation history
    if (request.messages.length > 0) {
      for (const msg of request.messages) {
        output.infoLog(`[${msg.role.toUpperCase()}]: ${msg.content}`);
      }
    }
    
    // Display prompt
    output.infoLog(request.prompt);
    output.infoLog("Please Enter Your Response (Empty line to finish)");
    
    // Read user input via readline
    const content = await this.promptUser();
    
    return { requestId: request.requestId, content, timestamp: Date.now() };
  }
  
  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout
      });
      
      let content = "";
      let isFirstLine = true;
      
      rl.setPrompt("> ");
      rl.prompt();
      
      rl.on("line", line => {
        if (line.trim() === "" && !isFirstLine) {
          rl.close();
          resolve(content.trim());
        } else {
          if (!isFirstLine) content += "\n";
          content += line;
          isFirstLine = false;
          rl.prompt();
        }
      });
      
      rl.on("SIGINT", () => {
        rl.close();
        reject(new Error("User cancelled"));
      });
    });
  }
}
```

### Web Frontend (Reference from Lim-Code)

From `ref/Lim-Code-1.0.93/frontend/src/components/message/ToolMessage.vue`:

- Displays tool calls with approve/reject buttons
- Shows tool parameters and descriptions
- Tracks pending confirmations via computed properties
- Integrates with ConfirmDialog component for modal interactions

### VSCode Extension (Reference from Lim-Code)

From `ref/Lim-Code-1.0.93/webview/ChatViewProvider.ts`:

- Uses Webview postMessage API for communication
- Manages diff states and tool execution status
- Provides checkpoint restore functionality
- Implements human relay with file I/O support

## Design Principles

1. **Layer Separation**: SDK handles coordination logic, application layers handle UI
2. **Event-Driven**: Asynchronous communication via strongly-typed events
3. **Flexible Handlers**: Platform-specific implementations (CLI/Web/VSCode)
4. **Safety First**: Configurable auto-approval with risk assessment
5. **Resilience**: Checkpoint-based persistence for long-running operations
6. **Extensibility**: Plugin architecture for custom interaction types
7. **Timeout Control**: Built-in timeout and cancellation mechanisms

## Key Files

- **Types**: `packages/types/src/interaction.ts`, `packages/types/src/tool/approval.ts`
- **Events**: `packages/types/src/events/interaction-events.ts`
- **Coordinators**: 
  - `sdk/core/coordinators/tool-approval-coordinator.ts`
  - `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`
  - `sdk/agent/execution/coordinators/agent-execution-coordinator.ts`
- **Handlers**: `sdk/workflow/execution/handlers/node-handlers/user-interaction-handler.ts`
- **Auto-Approval**: `sdk/services/auto-approval/auto-approval-checker.ts`
- **CLI Handler**: `apps/cli-app/src/handlers/cli-human-relay-handler.ts`
- **Event Builders**: `sdk/core/utils/event/builders/interaction-events.ts`
