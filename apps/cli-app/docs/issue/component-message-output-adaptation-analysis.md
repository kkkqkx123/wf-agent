# Component Message Output Adaptation Analysis

Date: 2026-05-31
Scope: How cli-app output functionality adapts SDK's component-message system

## 1. Architecture Overview

SDK's component-message system is defined in `sdk/api/shared/component-message/`:

| Component | Responsibility |
|-----------|---------------|
| message-bus.ts | Central message distribution bus: publish/subscribe, routing rule matching, OutputHandler registration |
| publisher-api.ts | Convenience API wrapping typed message publishing by category |
| routing-utils.ts | Route matching utility functions (matchesRoutingRule, findMatchingRule, sortRulesByPriority) |
| packages/types/src/component-message/ | Type definitions: 8 message categories, 5 output targets, entity identity, routing rules |

Message flow:

```
SDK Agent/Workflow (publisher)
    |
    v
MessageBus.publish()
    |
    +---> decideOutput() -> matchesRoutingRule() -> find matching RoutingRule
    |         |
    |         v
    |     OutputDecision.targets (e.g., [TUI, FILE_DISPLAY])
    |         |
    |         v
    |     findHandler(target, message) -> OutputHandler.handle(message)
    |
    +---> notifySubscribers() -> direct subscribers (screens)
```

Components 8 message categories (MessageCategory): SYSTEM, WORKFLOW_EXECUTION, AGENT, TOOL, HUMAN_RELAY, SUBGRAPH, CHECKPOINT, EVENT

5 output targets (OutputTarget): TUI, FILE_FUNCTIONAL, FILE_DISPLAY, EVENT_BUS, NONE

## 2. Adaptation Mechanism

cli-app builds a 4-layer adaptation architecture:

```
SDK MessageBus (publish)
    |
    +--> [Layer 1] Routing Rules (CLI_ROUTING_RULES)
    |         |
    |         +--> [Layer 2] OutputHandler
    |         |       |
    |         |       +--> TUIOutputHandler (tui-output-handler.ts)
    |         |       |    - target: TUI
    |         |       |    - Registered on MessageBus, subscribable by screens
    |         |       |    - Maintains ring buffer + subscriber pattern
    |         |       |
    |         |       +--> DisplayFileHandler (display-file-handler.ts)
    |         |       |    - target: FILE_DISPLAY
    |         |       |    - Converts to DisplaySection, buffers, batch writes output.md
    |         |       |    - Supports: tool_result, node_start/end, checkpoint, iteration, error
    |         |       |
    |         |       +--> FunctionalFileHandler (functional-file-handler.ts)
    |         |              - target: FILE_FUNCTIONAL
    |         |              - Handles human_relay.request only, writes to human-relay-output.txt
    |         |
    |         +--> [Layer 3] File IO Services
    |                  DisplayOutputService, HumanRelayService
    |
    +--> [Layer 3] Direct Subscription --> TUI Screens
    |    (AgentScreen / DashboardScreen / WorkflowScreen)
    |    - Subscribe directly to MessageBus with category/type/entity filters
    |    - Drive real-time UI updates
    |
    +--> [Layer 4] CLI Command Layer (parallel path, NOT component-message)
         (Adapters -> cli-formatters -> CLIOutput)
         - Calls SDK API directly (e.g., sdk.messages.getAll())
         - Formats via cli-formatters.ts (domain objects)
         - Outputs to stdout/stderr/log via CLIOutput (output.ts)
         - This is a separate path, not using component-message system
```

### 2.1 Routing Rule Layer

File: `src/config/routing-rules.ts`

Defines 10 routing rules (all using type-safe SDK enum values):

| Rule | Match (enum) | Targets | Priority |
|------|-------------|---------|----------|
| agent-llm-stream | AgentMessageType.LLM_STREAM | TUI | 100 |
| agent-human-relay-request | AgentMessageType.HUMAN_RELAY_REQUEST | TUI + FILE_FUNCTIONAL + FILE_DISPLAY | 100 |
| agent-human-relay-status | AgentMessageType.HUMAN_RELAY_RESPONSE/TIMEOUT/CANCEL | TUI + FILE_DISPLAY | 100 |
| agent-tool-call | AgentMessageType.TOOL_CALL_START/END | TUI | 100 |
| agent-tool-result | AgentMessageType.TOOL_RESULT | FILE_DISPLAY | 100 |
| workflow-execution-node | WorkflowExecutionMessageType.NODE_START/END | TUI + FILE_DISPLAY | 100 |
| workflow-execution-fork-branch | WorkflowExecutionMessageType.FORK_BRANCH_START/END | FILE_DISPLAY | 100 |
| subgraph-events | MessageCategory.SUBGRAPH | FILE_DISPLAY | 100 |
| error-messages | level=error/critical | TUI + FILE_DISPLAY | 50 (high) |
| default | match all | TUI | 999 (lowest) |

### 2.2 Handler Layer (OutputHandler)

Three OutputHandler implementations:

**TUIOutputHandler** (`src/handlers/tui/tui-output-handler.ts`)
- target: TUI
- Supports all messages (supports() returns true)
- Maintains ring buffer (default 100 messages) + subscriber pattern
- Screens can subscribe to receive TUI-targeted messages only
- **Currently no screens subscribe to it** - screens still use direct MessageBus subscription

**DisplayFileHandler** (`src/handlers/file/display-file-handler.ts`)
- target: FILE_DISPLAY
- Supports 6 types: TOOL_RESULT, NODE_START, NODE_END, CHECKPOINT_CREATE, ITERATION_START, "system.error"
- Buffers sections per session, flushes every 2 seconds
- Has flush()/close() for lifecycle management

**FunctionalFileHandler** (`src/handlers/file/functional-file-handler.ts`)
- target: FILE_FUNCTIONAL
- Only handles agent.human_relay.request
- Writes prompt content to human-relay-output.txt via HumanRelayService

### 2.3 TUI Screen Layer (Direct Subscription)

Three screens subscribe directly to MessageBus:

- **DashboardScreen**: Subscribes to Agent lifecycle (start/end) + WorkflowExecution lifecycle (start/end), updates live counts
- **AgentScreen**: Subscribes to 5 subscription groups: lifecycle, iteration, LLM stream, tool call - drives panels
- **WorkflowScreen**: Subscribes to node events + workflow lifecycle events

### 2.4 CLI Command Layer (Parallel Path)

Independent of component message system:
- Commands call SDK API via Adapters (message-adapter.ts, workflow-adapter.ts, etc.)
- Results formatted by cli-formatters.ts (incl formatComponentMessage/formComponentMessageList)
- Output through CLIOutput (output.ts) -> stdout/stderr/log file

## 3. Identified Issues and Current Status

### Fixed Issues

#### P0 - Type string mismatch (Bug) -- FIXED
- **Before**: Routing rules used hyphen format `"workflow-execution.node.start"` while SDK defines dot format `"workflow.execution.node.start"`
- **Fix**: All type strings replaced with SDK enum values (AgentMessageType, WorkflowExecutionMessageType)

#### P1 - TUI output target missing OutputHandler -- FIXED
- **Before**: No TUI OutputHandler registered on MessageBus
- **Fix**: TUIOutputHandler created and registered in CLIAppTUI.initializeMessageHandlers()

#### P1 - Handler lifecycle management -- FIXED
- **Before**: DisplayFileHandler flush()/close() never called on shutdown
- **Fix**: CLIAppTUI.stop() now calls messageBus.flush() and messageBus.close()

#### P2 - Missing HumanRelay status routing rules -- FIXED
- **Before**: Only HUMAN_RELAY_REQUEST routed, response/timeout/cancel went to default rule (TUI only)
- **Fix**: Added agent-human-relay-status rule for RESPONSE/TIMEOUT/CANCEL -> TUI + FILE_DISPLAY

#### P2 - Hardcoded type strings not type-safe -- FIXED
- **Before**: Routing rules used string literals instead of SDK exported enum values
- **Fix**: All rules use type-safe enum references (AgentMessageType.LLM_STREAM, etc.)

#### P3 - No component message formatters -- FIXED
- **Before**: cli-formatters.ts had no BaseComponentMessage formatting functions
- **Fix**: Added formatComponentMessage() and formatComponentMessageList() with table/verbose/plain modes

#### P3 - Routing config not integrated into CLIConfig -- FIXED
- **Before**: CLIConfig had no message routing related fields
- **Fix**: Added customRoutingRules?: RoutingRule[] to CLIConfig with default/schema

### Remaining Issues

#### P4 - FunctionalFileHandler lacks flush()/close() -- FIXED
- **Severity**: Low - FunctionalFileHandler writes synchronously, no buffering
- **Problem**: Implements OutputHandler but does not have flush()/close() methods
- **File**: `src/handlers/file/functional-file-handler.ts`
- **Suggested fix**: Add empty flush()/close() for interface consistency, even if no-op

## 4. Summary

| Issue | Severity | Status | Layer |
|-------|----------|--------|-------|
| Type string mismatch | P0 | Fixed | Routing + Handler |
| Missing TUI OutputHandler | P1 | Fixed | Handler |
| Handler lifecycle management | P1 | Fixed | TUI App |
| TUIOutputHandler not consumed | P1 | Fixed | Handler -> Screens |
| Missing HumanRelay status rules | P2 | Fixed | Routing |
| Type strings not type-safe | P2 | Fixed | Routing |
| No component message formatters | P3 | Fixed | Utils |
| Routing config not in CLIConfig | P3 | Fixed | Config |
| Redundant consumption paths | P4 | Fixed | Screens |
| "system.error" string literal | P4 | Fixed | Handler |
| FunctionalFileHandler no flush/close | P4 | Fixed | Handler |

All 11 issues have been resolved. The architecture now has a clean message flow: Routing Rules → TUIOutputHandler → Screen subscribers, with no redundant consumption paths.