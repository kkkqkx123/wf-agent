# User Interaction System - Quick Reference Guide

## System Overview

The user interaction system enables workflows to pause execution and collect user input through three operation types:

1. **TOOL_APPROVAL** - Approve/reject tool executions
2. **ADD_MESSAGE** - Add user messages to LLM conversations
3. **UPDATE_VARIABLES** - Update workflow variables with user input

## Architecture at a Glance

```
Application Layer (CLI/Web/VSCode)
         ↓ implements UserInteractionHandler
SDK Coordination Layer
         ↓ emits events
Core Infrastructure (Events, Checkpoints, State)
```

## Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Type Definitions | `packages/types/src/interaction.ts` | Core interfaces and types |
| Event Types | `packages/types/src/events/interaction-events.ts` | Lifecycle events |
| Tool Approval | `sdk/core/coordinators/tool-approval-coordinator.ts` | Approval coordination |
| Auto-Approval | `sdk/services/auto-approval/auto-approval-checker.ts` | Risk-based decisions |
| Node Handler | `sdk/workflow/execution/handlers/node-handlers/user-interaction-handler.ts` | ADD_MESSAGE & UPDATE_VARIABLES |
| Event Builders | `sdk/core/utils/event/builders/interaction-events.ts` | Event construction |

## Event Lifecycle

```
USER_INTERACTION_REQUESTED
    ↓ (Application displays UI and collects input)
USER_INTERACTION_RESPONDED
    ↓ (SDK processes input)
USER_INTERACTION_PROCESSED
    ↓ (Workflow continues)
    
OR on error/timeout:
USER_INTERACTION_FAILED
```

## Operation Types Quick Comparison

### TOOL_APPROVAL

**Purpose**: Control tool execution safety

**Configuration**:
```typescript
interface ToolApprovalOptions {
  autoApprovalEnabled?: boolean;
  securityPreset?: "SAFE" | "BALANCED" | "PERMISSIVE";
  categories?: { READ_ONLY?: boolean; WRITE?: boolean; ... };
  workspaceBoundary?: { allowReadOnlyOutsideWorkspace?: boolean; ... };
  maxAutoApprovedRequests?: number;
}
```

**Flow**:
1. Check usage limits
2. Evaluate auto-approval rules
3. Request manual approval if needed
4. Apply edited parameters/instructions
5. Execute or reject tool call

**Use Cases**:
- Preventing dangerous file operations
- Controlling network access
- Managing MCP server permissions
- Limiting command execution

---

### ADD_MESSAGE

**Purpose**: Incorporate user feedback into conversations

**Configuration**:
```toml
[nodes.config]
operationType = "ADD_MESSAGE"
prompt = "Please provide feedback:"

[nodes.config.message]
role = "user"
contentTemplate = "User feedback: {{input}}"
```

**Processing**:
1. Display prompt to user
2. Collect input via handler
3. Replace `{{input}}` in template
4. Add message to conversation manager
5. Continue workflow with updated context

**Use Cases**:
- Code review feedback
- Clarification requests
- Preference collection
- Conversation steering

---

### UPDATE_VARIABLES

**Purpose**: Store user decisions as workflow state

**Configuration**:
```toml
[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Select environment:"

[[nodes.config.variables]]
variableName = "deployEnv"
expression = "{{input}}"
scope = "workflowExecution"
```

**Processing**:
1. Display prompt to user
2. Collect input via handler
3. Evaluate expressions for each variable
4. Update workflow variable scopes
5. Variables available to downstream nodes

**Use Cases**:
- Configuration selection
- Decision points
- Parameter customization
- State persistence

---

## Template System

Both ADD_MESSAGE and UPDATE_VARIABLES support the `{{input}}` placeholder:

**Examples**:
```typescript
// Simple replacement
"{{input}}" → "production"

// Prefix/suffix
"Deploying to: {{input}}" → "Deploying to: production"

// Multiple occurrences
"Selected: {{input}}, confirmed: {{input}}" → "Selected: production, confirmed: production"
```

**Limitations**:
- String-based processing only
- No arithmetic or complex expressions
- All replacements use same input value

---

## Timeout & Cancellation

All operations support timeout and cancellation:

```typescript
// Timeout configuration
timeout: 30000  // 30 seconds (default)

// Cancellation via context
context.cancelToken.cancel()

// Three-way race condition
await Promise.race([
  handler.handle(request, context),  // User input
  timeoutPromise,                     // Timeout rejection
  cancelPromise                       // Cancel rejection
])
```

---

## Application Implementation Guide

### Minimal Handler Implementation

```typescript
import type { UserInteractionHandler, UserInteractionRequest, UserInteractionContext } from "@wf-agent/types";

export class MyHandler implements UserInteractionHandler {
  async handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown> {
    // 1. Display prompt to user
    console.log(request.prompt);
    
    // 2. Collect input (platform-specific)
    const input = await this.collectInput();
    
    // 3. Return input data
    return input;
  }
  
  private async collectInput(): Promise<string> {
    // Platform-specific implementation
    // CLI: readline interface
    // Web: Form/modal dialog
    // VSCode: Webview input
  }
}
```

### CLI Example

```typescript
import * as readline from "readline";

class CLIHandler implements UserInteractionHandler {
  async handle(request: UserInteractionRequest): Promise<unknown> {
    console.log(`\n${request.prompt}\n`);
    
    return new Promise((resolve) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout
      });
      
      rl.question("> ", (answer) => {
        rl.close();
        resolve(answer.trim());
      });
    });
  }
}
```

### Web Example (Conceptual)

```typescript
class WebHandler implements UserInteractionHandler {
  async handle(request: UserInteractionRequest): Promise<unknown> {
    // Show modal/dialog
    const modal = showModal({
      prompt: request.prompt,
      timeout: request.timeout
    });
    
    // Wait for user response
    const result = await modal.waitForInput();
    
    return result;
  }
}
```

---

## Tool Approval Configuration Examples

### Safe Preset (Most Restrictive)

```typescript
const options: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  securityPreset: "SAFE",
  categories: {
    READ_ONLY: true,      // Auto-approve read operations
    WRITE: false,         // Require approval for writes
    COMMAND: false,       // Require approval for commands
    NETWORK: false,       // Require approval for network
    MCP: false,           // Require approval for MCP
  },
  maxAutoApprovedRequests: 5
};
```

### Balanced Preset

```typescript
const options: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  securityPreset: "BALANCED",
  categories: {
    READ_ONLY: true,
    WRITE: true,          // Auto-approve safe writes
    COMMAND: false,
    NETWORK: true,        // Auto-approve known domains
    MCP: false,
  },
  workspaceBoundary: {
    allowReadOnlyOutsideWorkspace: true,
    allowWriteOutsideWorkspace: false
  }
};
```

### Permissive Preset

```typescript
const options: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  securityPreset: "PERMISSIVE",
  categories: {
    READ_ONLY: true,
    WRITE: true,
    COMMAND: true,        // Auto-approve commands
    NETWORK: true,
    MCP: true,            // Auto-approve MCP tools
  },
  maxAutoApprovedRequests: 20
};
```

---

## Common Patterns

### Pattern 1: Confirmation Before Action

```toml
[[nodes]]
id = "confirm_deployment"
type = "USER_INTERACTION"

[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Ready to deploy to production? (yes/no)"

[[nodes.config.variables]]
variableName = "deploymentConfirmed"
expression = "{{input}}"

[[nodes]]
id = "route_based_on_confirmation"
type = "ROUTE"

[nodes.config.routes]
proceed = "deploymentConfirmed == 'yes'"
cancel = "deploymentConfirmed == 'no'"
```

### Pattern 2: Iterative Refinement

```toml
[[nodes]]
id = "generate_code"
type = "LLM"

[[nodes]]
id = "get_feedback"
type = "USER_INTERACTION"

[nodes.config]
operationType = "ADD_MESSAGE"
prompt = "Review the code. What changes are needed?"

[nodes.config.message]
role = "user"
contentTemplate = "Please revise the code based on this feedback: {{input}}"

[[nodes]]
id = "revise_code"
type = "LLM"
# Receives updated conversation with feedback
```

### Pattern 3: Configuration Collection

```toml
[[nodes]]
id = "collect_config"
type = "USER_INTERACTION"

[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Enter deployment region:"

[[nodes.config.variables]]
variableName = "region"
expression = "{{input}}"

[[nodes.config.variables]]
variableName = "regionTimestamp"
expression = "{{input}}_at_{{timestamp}}"
# Note: Would need timestamp injection in real implementation
```

---

## Debugging Tips

### Enable Event Logging

```typescript
eventManager.on("USER_INTERACTION_REQUESTED", (event) => {
  console.log("Interaction requested:", {
    id: event.interactionId,
    type: event.operationType,
    prompt: event.prompt
  });
});

eventManager.on("USER_INTERACTION_PROCESSED", (event) => {
  console.log("Interaction processed:", {
    id: event.interactionId,
    results: event.results
  });
});

eventManager.on("USER_INTERACTION_FAILED", (event) => {
  console.error("Interaction failed:", {
    id: event.interactionId,
    reason: event.reason
  });
});
```

### Check Checkpoint State

```typescript
// For long-running approvals, check checkpoint state
const checkpoint = await checkpointStateManager.get(checkpointId);
console.log("Pending approval:", checkpoint.customFields.toolApprovalState);
```

### Validate Handler Implementation

```typescript
// Test handler with mock request
const handler = new MyHandler();
const mockRequest: UserInteractionRequest = {
  interactionId: "test-123",
  operationType: "ADD_MESSAGE",
  prompt: "Test prompt",
  timeout: 5000
};

const mockContext: UserInteractionContext = {
  executionId: "exec-123",
  workflowId: "wf-123",
  nodeId: "node-123",
  getVariable: () => undefined,
  setVariable: async () => {},
  getVariables: () => ({}),
  timeout: 5000,
  cancelToken: { cancelled: false, cancel: () => {} }
};

const result = await handler.handle(mockRequest, mockContext);
console.log("Handler result:", result);
```

---

## Performance Considerations

### Timeout Settings

- **Short interactions** (confirmations): 5-10 seconds
- **Medium interactions** (short text): 30-60 seconds
- **Long interactions** (detailed feedback): 2-5 minutes
- **Complex decisions** (code review): 5-10 minutes

### Memory Management

- Clean up checkpoints after approval completion
- Limit concurrent pending interactions
- Monitor event listener memory leaks

### Scalability

- Use async handlers for non-blocking I/O
- Implement connection pooling for web handlers
- Consider queue-based processing for high volume

---

## Security Best Practices

### Input Validation

```typescript
// Always validate user input in handlers
async handle(request: UserInteractionRequest): Promise<unknown> {
  const input = await this.collectInput();
  
  // Validate length
  if (typeof input === "string" && input.length > 10000) {
    throw new Error("Input too long");
  }
  
  // Sanitize if needed
  const sanitized = this.sanitizeInput(input);
  
  return sanitized;
}
```

### Approval Limits

```typescript
// Prevent approval fatigue
const options: ToolApprovalOptions = {
  maxAutoApprovedRequests: 10,  // Force manual review after 10 auto-approvals
  approvalTimeout: 300000        // 5 minute timeout for approvals
};
```

### Workspace Boundaries

```typescript
// Restrict operations outside workspace
const options: ToolApprovalOptions = {
  workspaceBoundary: {
    allowReadOnlyOutsideWorkspace: false,
    allowWriteOutsideWorkspace: false
  }
};
```

---

## Troubleshooting

### Issue: Interaction Times Out

**Symptoms**: `USER_INTERACTION_FAILED` with timeout error

**Causes**:
- Handler not implemented correctly
- UI not displaying prompt
- Network latency (web)
- User didn't respond in time

**Solutions**:
1. Verify handler is registered
2. Check UI rendering logic
3. Increase timeout value
4. Add loading indicators

### Issue: Variables Not Updating

**Symptoms**: Variable remains unchanged after UPDATE_VARIABLES

**Causes**:
- Wrong variable scope
- Expression evaluation error
- Workflow state not persisted

**Solutions**:
1. Check variable name spelling
2. Verify scope configuration
3. Log expression evaluation
4. Ensure workflow execution state is accessible

### Issue: Messages Not Added to Conversation

**Symptoms**: LLM doesn't see user feedback

**Causes**:
- Conversation manager not provided
- Message format incorrect
- Timing issue with async operations

**Solutions**:
1. Verify conversationManager is passed to handler
2. Check message role is "user"
3. Confirm addMessage() is called
4. Add logging to track message flow

### Issue: Auto-Approval Not Working

**Symptoms**: All tools require manual approval despite configuration

**Causes**:
- autoApprovalEnabled not set to true
- Security preset too restrictive
- Usage limit exceeded

**Solutions**:
1. Set `autoApprovalEnabled: true`
2. Review security preset settings
3. Check `maxAutoApprovedRequests` limit
4. Verify tool risk level categorization

---

## Related Documentation

- [User Interaction Architecture Overview](./overview.md)
- [ADD_MESSAGE and UPDATE_VARIABLES Deep Dive](./add-message-update-variables.md)
- [Tool Approval System Design](../../architecture/tool/approval-system.md) *(if exists)*
- [Event System Documentation](../../sdk/event-system-analysis.md)
- [Checkpoint Mechanism](../../storage/checkpoint-design.md)

---

## Quick Links

### Source Files
- Types: `packages/types/src/interaction.ts`
- Events: `packages/types/src/events/interaction-events.ts`
- Handlers: `sdk/workflow/execution/handlers/node-handlers/user-interaction-handler.ts`
- Approval: `sdk/core/coordinators/tool-approval-coordinator.ts`

### Examples
- CLI Handler: `apps/cli-app/src/handlers/cli-human-relay-handler.ts`
- Reference Web UI: `ref/Lim-Code-1.0.93/frontend/src/components/`

### Tests
- Integration tests in app test directories
- Unit tests in SDK `__tests__` folders
