# Tool Approval Logic Consolidation Analysis

## Executive Summary

**Current State**: The tool approval logic is **fragmented and inconsistent** across different execution contexts (Workflow, Agent, Graph).

**Recommendation**: **YES, consolidate to a single implementation** by removing duplicate code and ensuring all execution paths use the existing `ToolApprovalCoordinator`.

---

## 1. Current Fragmentation Analysis

### 1.1 Multiple Approval Implementations

The project currently has **three separate approval mechanisms**:

#### A. Workflow-Level Simple Whitelist

**Location**: [`sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`](file:///d:/项目/agent/wf-agent/sdk/workflow/execution/coordinators/llm-execution-coordinator.ts#L527-L535)

```typescript
private requiresHumanApproval(toolId: ID, workflowConfig: WorkflowConfig | undefined): boolean {
  if (!workflowConfig?.toolApproval) {
    return false; // No config = auto-execute all
  }

  const autoApproved = workflowConfig.toolApproval.autoApprovedTools || [];
  return !autoApproved.includes(toolId); // Simple whitelist check
}
```

**Characteristics**:

- ✅ Simple and easy to understand
- ❌ Only supports basic whitelist
- ❌ No risk-level awareness
- ❌ No fine-grained controls (workspace boundaries, command whitelists, etc.)
- ❌ Limited to workflow context only

#### B. Advanced Auto-Approval System

**Location**: [`sdk/core/coordinators/tool-approval-coordinator.ts`](file:///d:/项目/agent/wf-agent/sdk/core/coordinators/tool-approval-coordinator.ts#L81-L128)

```typescript
async processToolApproval(params): Promise<ToolApprovalResult> {
  // 1. Check auto-approval (risk-based)
  if (options?.autoApprovalEnabled && tool) {
    const decision = this.checkAutoApproval(tool, toolCall, options);
    // Returns approve/deny/ask/timeout decisions
  }

  // 2. Legacy check: whitelist
  if (!this.requiresApproval(toolName, options)) {
    return { approved: true };
  }

  // 3. Request user approval
  return this.requestUserApproval(params);
}
```

**Characteristics**:

- ✅ Supports risk-based classification (READ_ONLY, WRITE, EXECUTE, MCP, NETWORK, SYSTEM, INTERACTION)
- ✅ Fine-grained controls (file permissions, workspace boundaries, command/domain whitelists)
- ✅ Category-based approval settings
- ✅ Timeout auto-response for interactions
- ⚠️ Only used when explicitly instantiated with `ToolApprovalCoordinator`
- ❌ Not integrated into Agent execution flow

#### C. No Approval in Agent Execution

**Location**: [`sdk/agent/execution/coordinators/agent-execution-coordinator.ts`](file:///d:/项目/agent/wf-agent/sdk/agent/execution/coordinators/agent-execution-coordinator.ts#L809-L845)

```typescript
private async executeToolCalls(entity, conversationManager, toolCalls): Promise<void> {
  // Directly executes tools WITHOUT any approval check!
  const toolResults = await this.toolCallExecutor.executeToolCalls(
    toolCalls.map(tc => ({ id: tc.id, name: tc.name, arguments: tc.arguments })),
    conversationManager,
    entity.id,
    entity.nodeId,
    { abortSignal: entity.getAbortSignal() },
  );
  // ...
}
```

**Characteristics**:

- ❌ **NO approval mechanism at all**
- ❌ All tools execute automatically without user consent
- ❌ Security risk for high-risk operations (EXECUTE, WRITE, etc.)

---

## 2. Inconsistency Problems

### 2.1 Configuration Inconsistency

| Context              | Configuration Type               | Supported Features                     |
| -------------------- | -------------------------------- | -------------------------------------- |
| **Workflow**         | `ToolApprovalConfig` (simple)    | - Whitelist only (`autoApprovedTools`) |
| **Agent**            | None                             | - No configuration available           |
| **Graph**            | Unknown (not checked)            | - Likely missing                       |
| **Core Coordinator** | `ToolApprovalOptions` (advanced) | - Full feature set                     |

**Problem**: Two completely different configuration interfaces:

- `ToolApprovalConfig` (workflow/config.ts) - simple whitelist
- `ToolApprovalOptions` (tool/approval.ts) - advanced options

These are **NOT compatible** and serve different purposes, causing confusion.

### 2.2 Behavioral Inconsistency

Same tool call behaves differently depending on execution context:

```typescript
// Scenario: LLM calls "execute_shell" with command "rm -rf /"

// In Workflow LLM Node:
if (workflowConfig.toolApproval.autoApprovedTools.includes("execute_shell")) {
  // Executes directly ❌ DANGEROUS
} else {
  // Requests approval ✅ SAFE
}

// In Agent Loop:
// Always executes directly ❌❌ VERY DANGEROUS
await toolCallExecutor.executeToolCalls(...);

// With ToolApprovalCoordinator:
const decision = checkAutoApproval({ tool, options, context });
if (decision.decision === "deny") {
  // Blocked ✅ SAFE
} else if (decision.decision === "ask") {
  // Requests approval ✅ SAFE
}
```

### 2.3 Code Duplication

Both coordinators implement similar approval logic:

**LLMExecutionCoordinator** (lines 527-535):

```typescript
private requiresHumanApproval(toolId: ID, workflowConfig: WorkflowConfig | undefined): boolean {
  if (!workflowConfig?.toolApproval) {
    return false;
  }
  const autoApproved = workflowConfig.toolApproval.autoApprovedTools || [];
  return !autoApproved.includes(toolId);
}
```

**ToolApprovalCoordinator** (lines 217-232):

```typescript
requiresApproval(toolName: string, options?: ToolApprovalOptions): boolean {
  if (!options) {
    return false;
  }
  if (options.autoApprovalEnabled) {
    return true;
  }
  const autoApproved = options.autoApprovedTools || [];
  return !autoApproved.includes(toolName);
}
```

**Duplication Issues**:

- Similar logic implemented twice
- Different parameter types
- Different behavior when config is missing
- Maintenance burden

---

## 3. Architecture Problems

### 3.1 Missing Integration Points

```
┌─────────────────────────────────────────────┐
│         Tool Approval Ecosystem             │
├─────────────────────────────────────────────┤
│                                             │
│  ┌──────────────┐   ┌──────────────────┐   │
│  │  Workflow     │   │  ToolApproval    │   │
│  │  Coordinator  │   │  Coordinator     │   │
│  │              │   │                  │   │
│  │ • Simple      │   │ • Advanced       │   │
│  │ • Whitelist   │   │ • Risk-based     │   │
│  │ • No agent    │   │ • Full features  │   │
│  └──────────────┘   └──────────────────┘   │
│         │                    │              │
│         │                    │              │
│         ▼                    ▼              │
│  ┌──────────────────────────────────┐      │
│  │   ToolCallExecutor               │      │
│  │   (No approval logic)            │      │
│  └──────────────────────────────────┘      │
│         │                                  │
│         ▼                                  │
│  ┌──────────────┐   ┌──────────────────┐   │
│  │  Agent        │   │  Graph (?)       │   │
│  │  Coordinator  │   │  Coordinator     │   │
│  │              │   │                  │   │
│  │ • NO APPROVAL│   │ • ???            │   │
│  │ • Unsafe     │   │ • Unknown        │   │
│  └──────────────┘   └──────────────────┘   │
│                                             │
└─────────────────────────────────────────────┘

Problems:
1. Two parallel approval systems (not consolidated)
2. Agent has NO approval at all
3. ToolCallExecutor doesn't enforce approval
4. No standard interface between layers
```

### 3.2 Responsibility Confusion

**Who is responsible for approval?**

1. **LLMExecutionCoordinator**: Implements its own simple approval
2. **ToolApprovalCoordinator**: Provides advanced approval (but not always used)
3. **ToolCallExecutor**: Should enforce approval but doesn't
4. **AgentExecutionCoordinator**: Completely ignores approval

**Result**: Unclear responsibility leads to gaps in security.

### 3.3 Type Inconsistency

Two different type definitions for similar concepts:

```typescript
// packages/types/src/workflow/config.ts
export interface ToolApprovalConfig {
  autoApprovedTools: string[];
}

// packages/types/src/tool/approval.ts
export interface ToolApprovalOptions {
  autoApprovalEnabled?: boolean;
  filePermissions?: FilePermissionSettings;
  categories?: Partial<Record<AutoApprovalCategory, boolean>>;
  workspaceBoundary?: WorkspaceBoundarySettings;
  allowWriteProtected?: boolean;
  command?: CommandExecutionSettings;
  network?: NetworkSettings;
  mcp?: McpApprovalSettings;
  interaction?: InteractionSettings;
  autoApprovedTools?: string[]; // Same field but optional!
  approvalTimeout?: number;
}
```

**Issues**:

- Different names (`Config` vs `Options`)
- Different required/optional fields
- `ToolApprovalConfig` is a subset of `ToolApprovalOptions`
- No clear migration path

---

## 4. Impact Analysis

### 4.1 Security Risks

| Risk                                               | Severity        | Affected Context   |
| -------------------------------------------------- | --------------- | ------------------ |
| Agent executes dangerous tools without approval    | 🔴 **CRITICAL** | Agent Loop         |
| Workflow uses simple whitelist (no risk awareness) | 🟡 **MEDIUM**   | Workflow LLM Nodes |
| No centralized enforcement                         | 🟡 **MEDIUM**   | All contexts       |
| Inconsistent behavior confuses users               | 🟢 **LOW**      | User Experience    |

**Example Attack Vector**:

```typescript
// Agent mode: User asks "delete all files in /tmp"
// LLM generates: execute_shell(command="rm -rf /tmp/*")
// Current behavior: EXECUTES IMMEDIATELY without approval ❌
// Expected behavior: Should require approval for EXECUTE risk level ✅
```

### 4.2 Maintenance Burden

- **3 separate code paths** to maintain
- **Bug fixes** must be applied in multiple places
- **New features** (e.g., new risk levels) require updates everywhere
- **Testing** must cover all variations

### 4.3 Developer Confusion

Developers asking:

- "Which approval system should I use?"
- "Why does my workflow approval config not work in agent mode?"
- "How do I enable approval for custom tools?"

---

## 5. Consolidation Strategy

### 5.1 Proposed Architecture

```
┌─────────────────────────────────────────────┐
│      Consolidated Approval System           │
├─────────────────────────────────────────────┤
│                                             │
│  ┌──────────────────────────────────────┐   │
│  │  ToolApprovalCoordinator             │   │
│  │  (KEEP - Single source of truth)     │   │
│  │                                      │   │
│  │  • Risk-based classification         │   │
│  │  • Fine-grained controls             │   │
│  │  • Full feature set                  │   │
│  │  • Already exists in sdk/core        │   │
│  └──────────────────────────────────────┘   │
│                  │                          │
│                  ▼                          │
│  ┌──────────────────────────────────────┐   │
│  │  Integration Layer                   │   │
│  │  (Modify existing components)        │   │
│  │                                      │   │
│  │  • LLMExecutionCoordinator: REMOVE   │   │
│  │    duplicate approval logic          │   │
│  │  • ToolCallExecutor: ADD optional    │   │
│  │    approval integration              │   │
│  │  • AgentExecutionCoordinator: ADD    │   │
│  │    approval support                  │   │
│  └──────────────────────────────────────┘   │
│                  │                          │
│         ┌────────┼────────┐                │
│         ▼        ▼        ▼                │
│  ┌──────────┐ ┌──────┐ ┌────────┐         │
│  │Workflow  │ │Agent │ │ Graph  │         │
│  │Context   │ │Ctx   │ │ Context│         │
│  └──────────┘ └──────┘ └────────┘         │
│                                             │
└─────────────────────────────────────────────┘

Benefits:
✅ NO new modules created
✅ Remove duplicate implementations
✅ All contexts use same ToolApprovalCoordinator
✅ Consistent behavior across all contexts
✅ Centralized security enforcement
✅ Easier to maintain and extend
✅ Clear responsibility
```

### 5.2 Implementation Plan

#### Phase 1: Standardize Configuration Type

**Action**: Replace `ToolApprovalConfig` with `ToolApprovalOptions` everywhere

**Rationale**:

- `ToolApprovalOptions` already has full feature set
- `ToolApprovalConfig` is just a subset (only whitelist)
- No need to create new types

**Migration**:

```typescript
// packages/types/src/workflow/config.ts

// OLD - Remove this
export interface ToolApprovalConfig {
  autoApprovedTools: string[];
}

// NEW - Use existing type
import type { ToolApprovalOptions } from "../tool/approval.js";

export interface WorkflowConfig {
  timeout?: number;
  maxSteps?: number;
  enableCheckpoints?: boolean;
  checkpointConfig?: CheckpointConfig;
  retryPolicy?: {
    maxRetries?: number;
    retryDelay?: number;
    backoffMultiplier?: number;
  };
  // CHANGE: Use ToolApprovalOptions instead of ToolApprovalConfig
  toolApproval?: ToolApprovalOptions;
}
```

**Backward Compatibility Helper**:

```typescript
// packages/types/src/tool/approval-migration.ts

/**
 * Migrate old simple config to full options
 * Preserves backward compatibility
 */
export function migrateSimpleConfigToOptions(
  simpleConfig: { autoApprovedTools: string[] } | undefined,
): ToolApprovalOptions {
  if (!simpleConfig) {
    return {};
  }

  // Convert simple whitelist to advanced format
  return {
    autoApprovalEnabled: false, // Disabled by default for safety
    autoApprovedTools: simpleConfig.autoApprovedTools,
  };
}
```

#### Phase 2: Remove Duplicate Approval Logic from LLMExecutionCoordinator

**Action**: Delete the duplicate `requiresHumanApproval` method and use `ToolApprovalCoordinator` instead

**Current Code** (TO BE REMOVED):

```typescript
// sdk/workflow/execution/coordinators/llm-execution-coordinator.ts
// Lines 527-535 - DELETE THIS

private requiresHumanApproval(toolId: ID, workflowConfig: WorkflowConfig | undefined): boolean {
  if (!workflowConfig?.toolApproval) {
    return false;
  }
  const autoApproved = workflowConfig.toolApproval.autoApprovedTools || [];
  return !autoApproved.includes(toolId);
}
```

**Replacement**:

```typescript
// Add ToolApprovalCoordinator as dependency
import { ToolApprovalCoordinator } from "../../../core/coordinators/tool-approval-coordinator.js";

export class LLMExecutionCoordinator {
  private contextFactory: LLMContextFactory;
  private interruptionDetector?: InterruptionDetector;
  // ADD: Approval coordinator
  private approvalCoordinator: ToolApprovalCoordinator;

  constructor(config: LLMContextFactoryConfig) {
    this.contextFactory = new LLMContextFactory(config);

    if (config.interruptionDetector) {
      this.interruptionDetector = config.interruptionDetector;
    } else if (config.executionRegistry) {
      this.interruptionDetector = new InterruptionDetectorImpl(config.executionRegistry);
    }

    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(config.eventManager);
  }

  // Modified method to use ToolApprovalCoordinator
  private async executeToolCallsWithApproval(
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    conversationState: ConversationSession,
    executionId: string,
    nodeId: string,
    workflowConfig: WorkflowConfig | undefined,
    toolCallExecutor: ToolCallExecutor,
    options?: { abortSignal?: AbortSignal },
  ): Promise<void> {
    for (const toolCall of toolCalls) {
      // Get tool definition for risk assessment
      const tool = this.contextFactory.getToolService().getTool(toolCall.name);

      // Use ToolApprovalCoordinator for approval check
      const approvalResult = await this.approvalCoordinator.processToolApproval({
        toolCall: {
          id: toolCall.id,
          function: {
            name: toolCall.name,
            arguments: toolCall.arguments,
          },
        },
        tool,
        options: workflowConfig?.toolApproval,
        contextId: executionId,
        nodeId,
        approvalHandler: {
          requestApproval: async request => {
            return this.requestToolApprovalInternal(request, executionId, nodeId);
          },
        },
      });

      if (!approvalResult.approved) {
        // User refused; skip this tool call
        const toolMessage = {
          role: "tool" as MessageRole,
          content: JSON.stringify({
            error: approvalResult.rejectionReason || "Tool call was rejected",
            rejected: true,
          }),
          toolCallId: toolCall.id,
        };
        conversationState.addMessage(toolMessage);
        continue;
      }

      // If user provides edited parameters
      if (approvalResult.editedParameters) {
        toolCall.arguments = JSON.stringify(approvalResult.editedParameters);
      }

      // If user provides additional instructions
      if (approvalResult.userInstruction) {
        conversationState.addMessage({
          role: "user" as MessageRole,
          content: approvalResult.userInstruction,
        });
      }

      // Execute tool invocation
      await toolCallExecutor.executeToolCalls(
        [toolCall],
        conversationState,
        executionId,
        nodeId,
        options,
      );
    }
  }
}
```

#### Phase 3: Add Approval Support to AgentExecutionCoordinator

**Action**: Integrate `ToolApprovalCoordinator` into agent loop

**Current Code** (MISSING APPROVAL):

```typescript
// sdk/agent/execution/coordinators/agent-execution-coordinator.ts
// Lines 809-845 - Currently executes tools without approval

private async executeToolCalls(entity, conversationManager, toolCalls): Promise<void> {
  // Directly executes tools WITHOUT any approval check!
  const toolResults = await this.toolCallExecutor.executeToolCalls(
    toolCalls.map(tc => ({ id: tc.id, name: tc.name, arguments: tc.arguments })),
    conversationManager,
    entity.id,
    entity.nodeId,
    { abortSignal: entity.getAbortSignal() },
  );
  // ...
}
```

**Modified Code** (WITH APPROVAL):

```typescript
// Add dependencies
import { ToolApprovalCoordinator } from "../../../core/coordinators/tool-approval-coordinator.js";

export class AgentExecutionCoordinator {
  // ... existing fields ...
  private approvalCoordinator: ToolApprovalCoordinator;

  constructor(/* existing params */) {
    // ... existing initialization ...

    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(eventManager);
  }

  private async executeToolCalls(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
  ): Promise<void> {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    logger.debug("Executing tool calls", {
      agentLoopId,
      iteration,
      toolCallCount: toolCalls.length,
    });

    // Check interruption before tool execution
    const preToolInterruption = checkInterruption(entity.getAbortSignal());
    if (preToolInterruption.type === "paused" || preToolInterruption.type === "stopped") {
      logger.info("Interrupted before tool execution", {
        agentLoopId,
        iteration,
        interruptionType: preToolInterruption.type,
        pendingToolCalls: toolCalls.length,
      });
      for (const tc of toolCalls) {
        entity.state.recordToolCallEnd(tc.id, undefined, "Cancelled due to interruption");
      }
      return;
    }

    // NEW: Process each tool call through approval coordinator
    const approvedToolCalls: Array<{ id: string; name: string; arguments: string }> = [];

    for (const toolCall of toolCalls) {
      const tool = this.toolService.getTool(toolCall.name);

      // Get approval config from agent configuration
      const approvalOptions = this.getApprovalOptions(entity);

      const approvalResult = await this.approvalCoordinator.processToolApproval({
        toolCall: {
          id: toolCall.id,
          function: {
            name: toolCall.name,
            arguments: toolCall.arguments,
          },
        },
        tool,
        options: approvalOptions,
        contextId: entity.id,
        nodeId: entity.nodeId,
        approvalHandler: {
          requestApproval: async request => {
            return this.requestAgentApproval(request, entity);
          },
        },
      });

      if (!approvalResult.approved) {
        logger.warn("Tool call rejected by approval", {
          agentLoopId,
          toolCallId: toolCall.id,
          toolName: toolCall.name,
          reason: approvalResult.rejectionReason,
        });

        // Record rejection
        entity.state.recordToolCallEnd(
          toolCall.id,
          undefined,
          approvalResult.rejectionReason || "Rejected by user",
        );
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
          content: approvalResult.userInstruction,
        });
      }

      approvedToolCalls.push(toolCall);
    }

    // Execute only approved tool calls
    if (approvedToolCalls.length === 0) {
      logger.debug("No tool calls approved, skipping execution", {
        agentLoopId,
        iteration,
      });
      return;
    }

    const toolResults = await this.toolCallExecutor.executeToolCalls(
      approvedToolCalls.map(tc => ({ id: tc.id, name: tc.name, arguments: tc.arguments })),
      conversationManager,
      entity.id,
      entity.nodeId,
      { abortSignal: entity.getAbortSignal() },
    );

    // Rest of existing logic...
    for (const result of toolResults) {
      // ... existing hook execution and result recording ...
    }
  }

  private getApprovalOptions(entity: AgentLoopEntity): ToolApprovalOptions {
    // Get from agent configuration or use defaults
    return (
      entity.config?.approvalOptions || {
        autoApprovalEnabled: false, // Safe default
      }
    );
  }

  private async requestAgentApproval(
    request: ToolApprovalRequest,
    entity: AgentLoopEntity,
  ): Promise<ToolApprovalResult> {
    // Implement agent-specific approval UI/logic
    // This could be CLI prompt, TUI interaction, etc.
    // For now, return rejection as safe default
    return {
      approved: false,
      toolCallId: request.toolCall.id,
      rejectionReason: "Interactive approval not yet implemented for agent mode",
    };
  }
}
```

#### Phase 4: Update DI Container Configuration

**Action**: Ensure `ToolApprovalCoordinator` is properly injected where needed

```typescript
// sdk/core/di/container-config.ts

// ToolApprovalCoordinator - Already registered as singleton
container
  .bind(Identifiers.ToolApprovalCoordinator)
  .toDynamicValue((c: IContainer): ToolApprovalCoordinator => {
    const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
    return new ToolApprovalCoordinator(eventManager);
  })
  .inSingletonScope();

// LLMExecutionCoordinator - Inject approval coordinator
container
  .bind(Identifiers.LLMExecutionCoordinator)
  .toDynamicValue((c: IContainer) => {
    const config: LLMContextFactoryConfig = {
      // ... existing config ...
      eventManager: c.get(Identifiers.EventRegistry) as EventRegistry,
    };
    return new LLMExecutionCoordinator(config);
  })
  .inTransientScope();

// AgentExecutionCoordinator - Inject approval coordinator
container
  .bind(Identifiers.AgentExecutionCoordinator)
  .toDynamicValue((c: IContainer) => {
    return new AgentExecutionCoordinator(
      // ... existing params ...
      c.get(Identifiers.EventRegistry) as EventRegistry,
    );
  })
  .inTransientScope();
```

---

## 6. Benefits of Consolidation

### 6.1 Security Improvements

✅ **Consistent protection** across all execution contexts
✅ **Risk-aware approval** for all tools (not just workflows)
✅ **Centralized enforcement** prevents bypass
✅ **Audit trail** from single approval service
✅ **Eliminate critical gap** in Agent mode

### 6.2 Maintainability

✅ **Remove duplicate code** (~200 lines deleted)
✅ **Single source of truth** (`ToolApprovalCoordinator`)
✅ **Easier bug fixes** (one place to update)
✅ **Simpler testing** (test once, works everywhere)
✅ **Clear documentation** (one approval system to explain)

### 6.3 Developer Experience

✅ **Consistent API** across contexts
✅ **Clear configuration** (single `ToolApprovalOptions` type)
✅ **Better defaults** (secure by default)
✅ **Straightforward migration** (no new concepts)

### 6.4 Extensibility

✅ **Easy to add** new approval rules (modify `ToolApprovalCoordinator` only)
✅ **Plugin architecture** via `ToolApprovalHandler` interface
✅ **Future-proof** design

---

## 7. Migration Guide

### For Existing Workflows

**Before** (simple config):

```toml
[workflow.config.toolApproval]
autoApprovedTools = ["read_file", "search_files"]
```

**After** (same behavior, using `ToolApprovalOptions`):

```toml
[workflow.config.toolApproval]
autoApprovalEnabled = false  # Explicit disable for safety
autoApprovedTools = ["read_file", "search_files"]
```

**Or upgrade to advanced mode**:

```toml
[workflow.config.toolApproval]
autoApprovalEnabled = true
categories.alwaysAllowReadOnly = true
categories.alwaysAllowWrite = false
workspaceBoundary.allowReadOnlyOutsideWorkspace = true
```

### For Agent Mode

**Before**: No approval (unsafe)

**After**:

```typescript
const agent = new AgentLoop({
  approvalOptions: {
    autoApprovalEnabled: true,
    categories: {
      alwaysAllowReadOnly: true,
      alwaysAllowExecute: false, // Require approval for commands
    },
  },
});
```

### Code Changes Required

1. **Replace imports**:

   ```typescript
   // OLD
   import type { ToolApprovalConfig } from "@wf-agent/types";

   // NEW
   import type { ToolApprovalOptions } from "@wf-agent/types";
   ```

2. **Update type references**:

   ```typescript
   // OLD
   toolApproval?: ToolApprovalConfig;

   // NEW
   toolApproval?: ToolApprovalOptions;
   ```

3. **Use migration helper if needed**:

   ```typescript
   import { migrateSimpleConfigToOptions } from "@wf-agent/types";

   const options = migrateSimpleConfigToOptions(oldConfig);
   ```

---

## 8. Recommendation

### ✅ YES, Consolidate to Single Implementation

**Priority**: **HIGH** (Security + Maintainability)

**Approach**:

- **REMOVE** duplicate approval logic from `LLMExecutionCoordinator`
- **ADD** approval support to `AgentExecutionCoordinator`
- **USE** existing `ToolApprovalCoordinator` everywhere
- **REPLACE** `ToolApprovalConfig` with `ToolApprovalOptions`
- **NO** new modules or services created

**Timeline**:

- **Phase 1** (Standardize config type): 2-3 days
- **Phase 2** (Remove duplicate from Workflow): 2-3 days
- **Phase 3** (Add to Agent): 3-4 days
- **Phase 4** (Update DI & tests): 2-3 days

**Total Estimated Time**: 2-3 weeks

**Risk Level**: **LOW** (Refactoring existing code, backward compatible)

**Key Success Factors**:

1. Keep `ToolApprovalCoordinator` as the single implementation
2. Remove ALL duplicate approval logic
3. Comprehensive testing across all contexts
4. Clear migration documentation
5. Gradual deprecation of `ToolApprovalConfig`

---

## 9. Conclusion

The current fragmented approval system poses **security risks** (especially in Agent mode) and creates **maintenance challenges**. Consolidating to the existing `ToolApprovalCoordinator` will:

1. **Eliminate security gaps** in Agent execution
2. **Remove ~200 lines of duplicate code**
3. **Improve consistency** across all execution contexts
4. **Simplify future enhancements** (single codebase)
5. **NO new modules required** - use what already exists

**This consolidation is strongly recommended and should be prioritized.**

The key principle: **One implementation (`ToolApprovalCoordinator`), used everywhere, with duplicates removed.**
