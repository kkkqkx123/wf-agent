# Shared Common Coordinators

## 1. Overview

The shared coordinators provide common coordination logic used by both workflow and agent execution engines. These are stateless services that orchestrate specific execution domains.

## 2. LLMExecutionCoordinator

Coordinates LLM execution across both workflow and agent contexts:

```
LLMExecutionCoordinator
├── execute(context) → Promise<LLMResult>
│   ├── Prepare messages (system prompt + conversation history)
│   ├── Resolve LLM profile (model, provider, parameters)
│   ├── Select tool format (compatible with provider)
│   ├── Call LLM provider via LLMWrapper
│   ├── Process response:
│   │   ├── Extract text content
│   │   ├── Extract tool calls
│   │   └── Track token usage
│   └── Return LLMResult
│
├── executeStream(context) → AsyncIterable<LLMStreamEvent>
│   ├── Similar to execute() but returns streaming events
│   ├── Token chunks, tool call deltas, metadata
│   └── Return async iterable for real-time processing
│
└── transformContext(context, transform) → LLMContext
    └── Apply context transformation (for tool format, message filtering)
```

### LLMContext

```typescript
interface LLMContext {
  messages: LLMMessage[];
  profileId?: string;
  llmProfile?: LLMProfile;
  toolSchemas?: ToolSchema[];
  toolFormat?: ToolCallFormatConfig;
  maxTokens?: number;
  temperature?: number;
  signal?: AbortSignal;
  metadata?: Record<string, unknown>;
}
```

### LLMResult

```typescript
interface LLMResult {
  content: string;
  toolCalls?: LLMToolCall[];
  usage: TokenUsageStats;
  finishReason: "stop" | "tool_calls" | "length" | "error";
  model: string;
  metadata?: Record<string, unknown>;
}
```

## 3. ToolApprovalCoordinator

Coordinates tool call approval workflows:

```
ToolApprovalCoordinator
├── executeToolWithApproval(toolCall, handler, options) → Promise<ToolApprovalResult>
│   ├── Check approval level:
│   │   ├── autoApproved → execute immediately
│   │   ├── confirmationRequired → request approval
│   │   └── denied → skip with rejection
│   ├── If approval required:
│   │   ├── Emit TOOL_APPROVAL_REQUESTED event
│   │   ├── Wait for approval handler response
│   │   ├── Handle timeout
│   │   └── Return approval result
│   └── Return ToolApprovalResult
│
├── batchApproval(toolCalls, handler) → Promise<ToolApprovalResult[]>
│   └── Batch approval for multiple tool calls
│
├── setApprovalLevel(toolName, level) → void
│   └── Configure approval level per tool
│
└── getApprovalLevel(toolName) → ApprovalLevel
    └── Query approval level
```

### Approval Levels

```
enum ApprovalLevel {
  AUTO_APPROVED = "auto_approved",   // Execute without confirmation
  CONFIRMATION = "confirmation",      // Require user confirmation
  DENIED = "denied",                  // Not allowed
}
```

### ToolApprovalResult

```typescript
interface ToolApprovalResult {
  toolCallId: string;
  toolName: string;
  approved: boolean;
  reason?: string;
  result?: unknown;  // Tool execution result if approved
}
```

## 4. RetryBudget

Manages retry budgets for execution retry with time tracking:

```
RetryBudget
├── Configuration
│   ├── budget: number  // Total budget in ms
│   ├── mode: "delay-only" | "total-time"
│   └── reset() → void
│
├── Budget Tracking
│   ├── isExhausted() → boolean
│   ├── recordRetryDelay(delay) → void
│   ├── recordExecutionTime(time) → void
│   ├── getTotalRetryDelay() → number
│   ├── getTotalExecutionTime() → number
│   └── getRemainingBudget() → number
│
├── Checkpoint Support
│   ├── createSnapshot() → RetryBudgetSnapshot
│   └── restoreFromSnapshot(snapshot) → void
│
└── Branch Allocation (for fork/join)
    ├── allocateBranchBudget(branchId, share) → void
    └── releaseBranchBudget(branchId) → void
```

### Time Budget Modes

- **delay-only**: Only retry delays count toward the budget
- **total-time**: Both retry delays and execution time count toward the budget

## 5. ToolPermissionManager

Manages tool permissions and access control:

```
ToolPermissionManager
├── checkPermission(toolName, context) → PermissionResult
│   ├── Check tool visibility
│   ├── Check tool allowlist/blocklist
│   ├── Check role-based permissions
│   └── Return PermissionResult
│
├── configurePermissions(config) → void
│   └── Configure tool permissions from config
│
└── getEffectivePermissions(toolName) → ToolPermission
    └── Get effective permission for a tool
```

## 6. RejectionMessageBuilder

Builds structured rejection messages for denied tool calls:

```
RejectionMessageBuilder
├── buildRejectionMessage(toolCall, reason) → LLMMessage
│   ├── Tool not found
│   ├── Tool not allowed
│   ├── Approval denied
│   └── Execution error
│
└── buildRejectionContent(toolName, reason) → string
    └── Human-readable rejection explanation
```

## 7. FailurePolicyManager

Manages failure handling policies:

```
FailurePolicyManager
├── getFailurePolicy(nodeType) → FailurePolicy
│   ├── fail: Stop execution on failure
│   ├── retry: Retry with backoff
│   ├── continue: Skip and continue
│   └── fallback: Execute fallback
│
├── configureFailurePolicy(config) → void
│   └── Configure policies from config
│
└── evaluateFailure(error, policy) → FailureAction
    └── Determine action based on error and policy
```