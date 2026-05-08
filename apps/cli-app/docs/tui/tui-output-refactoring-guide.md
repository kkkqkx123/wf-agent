# TUI Migration - Output System Refactoring Guide

## Overview

This document identifies which parts of the **already implemented** TUI components (Phases 1-6) need to be modified when integrating with the new SDK message bus and file IO system.

**Current Status**: Phases 1-6 are implemented but use direct adapter calls instead of the message-based architecture defined in the specification documents.

---

## 1. Files Requiring Modification

### 1.1 Agent Screen (`src/tui/screens/agent-screen.ts`)

**Current Implementation Issues:**
- Uses `AgentLoopAdapter` directly with event callbacks
- No integration with SDK message bus
- Hardcoded event type handling (`"text"`, `"tool_call_start"`, etc.)
- No entity hierarchy tracking
- No output routing decisions

**Required Changes:**

#### A. Replace Adapter with Message Bus Subscription

```typescript
// CURRENT (Lines 8, 22, 30):
import { AgentLoopAdapter } from "../../adapters/agent-loop-adapter.js";
private adapter: AgentLoopAdapter;
this.adapter = new AgentLoopAdapter();

// SHOULD BECOME:
import type { MessageBusAPI } from "@wf-agent/sdk";
import { MessageCategory, AgentMessageType } from "@wf-agent/types";

private messageBus: MessageBusAPI;
private currentAgentId?: string;

constructor(messageBus: MessageBusAPI, onBack?: () => void) {
  this.messageBus = messageBus;
  // ... rest of constructor
}
```

#### B. Update Event Handling to Message Subscription (Lines 157-187)

```typescript
// CURRENT: Direct event callback in handleEvent()
private handleEvent(event: any) {
  switch (event.type) {
    case "text":
      this.appendLog(event.delta, "assistant");
      break;
    // ... other cases
  }
}

// SHOULD BECOME: Message subscription with proper typing
private setupMessageSubscriptions() {
  if (!this.currentAgentId) return;
  
  this.messageBus.subscribe(
    {
      categories: [MessageCategory.AGENT],
      entityIds: [this.currentAgentId],
    },
    (message) => this.handleAgentMessage(message)
  );
}

private handleAgentMessage(message: BaseMessage) {
  switch (message.type) {
    case AgentMessageType.LLM_STREAM:
      // Route: TUI only (per message-output-prd.md)
      const data = message.data as AgentLLMStreamData;
      this.appendLog(data.chunk, "assistant", { stream: true });
      break;
      
    case AgentMessageType.TOOL_CALL_START:
      // Route: TUI summary only
      const toolData = message.data as AgentToolCallData;
      this.appendLog(`Calling: ${toolData.toolName}`, "tool");
      break;
      
    case AgentMessageType.TOOL_RESULT:
      // Route: FILE_DISPLAY only - don't show full result in TUI
      this.appendLog(`Tool result saved to output.md`, "system");
      break;
      
    case AgentMessageType.ITERATION_START:
      const iterData = message.data as AgentIterationData;
      this.updateStatus("running", iterData);
      this.appendLog(`Iteration ${iterData.iteration}/${iterData.maxIterations}`, "system");
      break;
      
    case AgentMessageType.HUMAN_RELAY_REQUEST:
      // This will trigger TUIHumanRelayHandler overlay
      this.updateStatus("waiting_human_relay");
      this.appendLog("Human Relay requested - check instructions", "warn");
      break;
  }
}
```

#### C. Update startAgent Method (Lines 122-155)

```typescript
// CURRENT: Direct adapter call
public async startAgent(config: AgentLoopRuntimeConfig) {
  const result = await this.adapter.executeAgentLoopStream(
    config,
    {},
    (event) => this.handleEvent(event)
  );
}

// SHOULD BECOME: Publish start message, let SDK handle execution
public async startAgent(config: AgentLoopRuntimeConfig) {
  this.currentAgentId = config.loopId;
  this.isRunning = true;
  this.updateStatus("starting");
  
  // SDK publishes AGENT_START message automatically
  // This screen subscribes and displays
  
  // Setup message subscriptions for this agent
  this.setupMessageSubscriptions();
}
```

#### D. Add Entity Context Tracking

```typescript
// ADD: Track iteration count and status
private iterationCount: number = 0;
private toolCallCount: number = 0;

private updateStatus(status: string, iterationData?: AgentIterationData) {
  this.statusPanel.clear();
  this.statusPanel.addChild(new Text(`Status: ${status.toUpperCase()}`, 1, 0));
  this.statusPanel.addChild(new Text(`Agent ID: ${this.currentAgentId || "N/A"}`, 1, 0));
  
  if (iterationData) {
    this.iterationCount = iterationData.iteration;
    this.toolCallCount = iterationData.toolCallCount;
    this.statusPanel.addChild(new Text(`Iteration: ${iterationData.iteration}/${iterationData.maxIterations}`, 1, 0));
    this.statusPanel.addChild(new Text(`Tool Calls: ${iterationData.toolCallCount}`, 1, 0));
  }
}
```

---

### 1.2 Human Relay Handler

**Implementation**: `src/tui/handlers/tui-human-relay-handler.ts`

The TUI uses a file-based Human Relay workflow integrated with FileIOService.

**Note**: The old `CLIHumanRelayHandler` (readline-based) has been removed. All modes now use the file-based approach.



---

### 1.3 Main App (`src/tui/app.ts`)

**Current Implementation Issues:**
- No message bus initialization
- No FileIOService integration
- Screens created without message bus dependency

**Required Changes:**

#### A. Add Message Bus and File IO Initialization

```typescript
// src/tui/app.ts
import { MessageBusAPI } from "@wf-agent/sdk";
import { FileIOService } from "../io/file-io-service.js";
import { TUIHumanRelayHandler } from "./handlers/tui-human-relay-handler.js";

export class CLIAppTUI {
  private tui: TUI;
  private messageBus: MessageBusAPI;
  private fileIO: FileIOService;
  private humanRelayHandler: TUIHumanRelayHandler;
  
  constructor() {
    this.tui = new TUI(new ProcessTerminal());
    
    // Initialize message bus
    this.messageBus = initializeMessageBus();
    
    // Initialize file IO service
    this.fileIO = new FileIOService({ baseDir: ".wf-agent" });
    
    // Initialize human relay handler with file IO
    this.humanRelayHandler = new TUIHumanRelayHandler(this.tui, this.fileIO);
    
    this.initializeScreens();
  }
  
  private initializeScreens() {
    // Pass message bus to screens
    this.screens.set("agent", new AgentScreen(this.messageBus, () => this.showScreen("dashboard")));
    // ... other screens
  }
  
  private initializeMessageBus(): MessageBusAPI {
    const decider = new OutputDeciderImpl(DEFAULT_ROUTING_RULES);
    const bus = new MessageBusImpl(decider);
    
    // Register handlers
    bus.registerHandler(new TUIHandler(this.tui));
    bus.registerHandler(new FunctionalFileHandler(this.fileIO));
    bus.registerHandler(new DisplayFileHandler(this.fileIO));
    
    return bus;
  }
}
```

---

### 1.4 Workflow Screen (`src/tui/screens/workflow-screen.ts`)

**Current Implementation Issues:**
- Uses `WorkflowAdapter` directly
- No message subscription for real-time updates

**Required Changes:**

#### A. Add Message Subscription for Workflow Events

```typescript
// src/tui/screens/workflow-screen.ts
import type { MessageBusAPI } from "@wf-agent/sdk";
import { MessageCategory, ThreadMessageType } from "@wf-agent/types";

export class WorkflowScreen implements Screen {
  private messageBus: MessageBusAPI;
  
  constructor(messageBus: MessageBusAPI, onBack?: () => void) {
    this.messageBus = messageBus;
    // ... rest
  }
  
  private setupMessageSubscriptions() {
    // Subscribe to thread/workflow events
    this.messageBus.subscribe(
      {
        categories: [MessageCategory.THREAD],
        types: [
          ThreadMessageType.NODE_START,
          ThreadMessageType.NODE_END,
        ],
      },
      (message) => this.handleWorkflowMessage(message)
    );
  }
  
  private handleWorkflowMessage(message: BaseMessage) {
    switch (message.type) {
      case ThreadMessageType.NODE_START:
        const data = message.data as ThreadNodeData;
        this.appendLog(`Node started: ${data.nodeId} (${data.nodeType})`, "system");
        break;
        
      case ThreadMessageType.NODE_END:
        const endData = message.data as ThreadNodeData;
        this.appendLog(
          `Node completed: ${endData.nodeId} (${endData.duration}ms)`,
          "system"
        );
        break;
    }
  }
}
```

---

### 1.5 Dashboard Screen (`src/tui/screens/dashboard-screen.ts`)

**Required Changes:**

#### A. Add Real-Time Status Updates

```typescript
// src/tui/screens/dashboard-screen.ts
import type { MessageBusAPI } from "@wf-agent/sdk";

export class DashboardScreen implements Screen {
  private messageBus: MessageBusAPI;
  private activeAgents: number = 0;
  private runningThreads: number = 0;
  
  constructor(messageBus: MessageBusAPI) {
    this.messageBus = messageBus;
    this.setupLiveUpdates();
  }
  
  private setupLiveUpdates() {
    // Subscribe to agent lifecycle events
    this.messageBus.subscribe(
      {
        categories: [MessageCategory.AGENT],
        types: [
          AgentMessageType.START,
          AgentMessageType.END,
        ],
      },
      (message) => {
        if (message.type === AgentMessageType.START) {
          this.activeAgents++;
        } else if (message.type === AgentMessageType.END) {
          this.activeAgents--;
        }
        this.updateStatusPanel();
      }
    );
    
    // Subscribe to thread lifecycle events
    this.messageBus.subscribe(
      {
        categories: [MessageCategory.THREAD],
        types: [
          ThreadMessageType.START,
          ThreadMessageType.END,
        ],
      },
      (message) => {
        if (message.type === ThreadMessageType.START) {
          this.runningThreads++;
        } else if (message.type === ThreadMessageType.END) {
          this.runningThreads--;
        }
        this.updateStatusPanel();
      }
    );
  }
  
  private updateStatusPanel() {
    // Update the status panel with live counts
    this.statusPanel.clear();
    this.statusPanel.addChild(new Text(`Active Agents: ${this.activeAgents}`));
    this.statusPanel.addChild(new Text(`Running Threads: ${this.runningThreads}`));
    this.statusPanel.addChild(new Text(`Last Updated: ${new Date().toLocaleTimeString()}`));
  }
}
```

---

## 2. New Files to Create

### 2.1 File IO Service

Create: `src/io/file-io-service.ts`

Following `file-io-prd.md` specification:

```typescript
import * as fs from "fs/promises";
import * as path from "path";

export interface FileIOServiceOptions {
  baseDir: string;
}

export interface HumanRelayPaths {
  functional: {
    humanRelayOutput: string;
    humanRelayInput: string;
  };
  display: {
    output: string;
  };
}

export class FileIOService {
  private baseDir: string;
  
  constructor(options: FileIOServiceOptions) {
    this.baseDir = options.baseDir;
  }
  
  /**
   * Generate session directory structure
   */
  getSessionPaths(sessionId: string): HumanRelayPaths {
    const functionDir = path.join(this.baseDir, "function", sessionId);
    const displayDir = path.join(this.baseDir, "display", sessionId);
    
    return {
      functional: {
        humanRelayOutput: path.join(functionDir, "human-relay-output.txt"),
        humanRelayInput: path.join(functionDir, "human-relay-input.txt"),
      },
      display: {
        output: path.join(displayDir, "output.md"),
      },
    };
  }
  
  /**
   * Write Human Relay output (prompt)
   */
  async writeHumanRelayOutput(params: {
    sessionId: string;
    content: string;
  }): Promise<void> {
    const paths = this.getSessionPaths(params.sessionId);
    const dir = path.dirname(paths.functional.humanRelayOutput);
    
    // Ensure directory exists
    await fs.mkdir(dir, { recursive: true });
    
    // Write pure text (no formatting)
    await fs.writeFile(paths.functional.humanRelayOutput, params.content, "utf-8");
  }
  
  /**
   * Watch Human Relay input file
   */
  watchHumanRelayInput(params: {
    sessionId: string;
    timeout: number;
    onResponse: (content: string) => void;
    onTimeout?: () => void;
  }): void {
    const paths = this.getSessionPaths(params.sessionId);
    const inputFile = paths.functional.humanRelayInput;
    
    // Create empty file if doesn't exist
    fs.writeFile(inputFile, "", "utf-8").catch(() => {});
    
    let responded = false;
    
    // Watch for file changes
    const watcher = fs.watch(path.dirname(inputFile), async (eventType, filename) => {
      if (responded) return;
      if (filename !== path.basename(inputFile)) return;
      
      try {
        const content = await fs.readFile(inputFile, "utf-8");
        if (content.trim().length > 0) {
          responded = true;
          watcher.close();
          params.onResponse(content);
        }
      } catch (error) {
        // Ignore read errors
      }
    });
    
    // Timeout
    setTimeout(() => {
      if (!responded) {
        responded = true;
        watcher.close();
        params.onTimeout?.();
      }
    }, params.timeout);
  }
  
  /**
   * Update display output (output.md)
   */
  async updateDisplayOutput(params: {
    sessionId: string;
    sections: DisplaySection[];
  }): Promise<void> {
    const paths = this.getSessionPaths(params.sessionId);
    const dir = path.dirname(paths.display.output);
    
    await fs.mkdir(dir, { recursive: true });
    
    // Build markdown content
    const content = this.buildOutputMarkdown(params.sections);
    
    await fs.writeFile(paths.display.output, content, "utf-8");
  }
  
  private buildOutputMarkdown(sections: DisplaySection[]): string {
    // Follow format from file-io-prd.md
    let md = "# Workflow Execution Output\n\n======\n\n";
    
    for (const section of sections) {
      md += `## ${section.title}\n\n`;
      md += `${section.content}\n\n`;
      md += "══════════════════════════════\n\n";
    }
    
    return md;
  }
}
```

### 2.2 Message Handlers

Create: `src/messaging/handlers/tui-handler.ts`

```typescript
import type { OutputHandler, BaseMessage, OutputTarget } from "@wf-agent/sdk";
import type { TUI } from "../../tui/core/tui.js";

export class TUIHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = "tui";
  
  constructor(private tui: TUI) {}
  
  supports(message: BaseMessage): boolean {
    // White-list of message types for TUI
    const supportedTypes = new Set([
      "agent.llm.stream",
      "agent.tool.call_start",
      "agent.tool.call_end",
      "agent.human_relay.request",
      "thread.node.start",
      "thread.node.end",
    ]);
    return supportedTypes.has(message.type);
  }
  
  async handle(message: BaseMessage): Promise<void> {
    // Delegate to appropriate screen/component
    // This is a simplified version - actual implementation would route to specific UI components
  }
}
```

Create: `src/messaging/handlers/functional-file-handler.ts`

```typescript
import type { OutputHandler, BaseMessage, OutputTarget } from "@wf-agent/sdk";
import type { FileIOService } from "../../io/file-io-service.js";

export class FunctionalFileHandler implements OutputHandler {
  readonly target = OutputTarget.FILE_FUNCTIONAL;
  readonly name = "file_functional";
  
  constructor(private fileIO: FileIOService) {}
  
  supports(message: BaseMessage): boolean {
    return message.type === "agent.human_relay.request";
  }
  
  async handle(message: BaseMessage): Promise<void> {
    if (message.type === "agent.human_relay.request") {
      const { prompt } = message.data as any;
      
      await this.fileIO.writeHumanRelayOutput({
        sessionId: message.entity.id,
        content: prompt,
      });
      
      // File watching is handled by TUIHumanRelayHandler
    }
  }
}
```

Create: `src/messaging/handlers/display-file-handler.ts`

```typescript
import type { OutputHandler, BaseMessage, OutputTarget } from "@wf-agent/sdk";
import type { FileIOService } from "../../io/file-io-service.js";

export class DisplayFileHandler implements OutputHandler {
  readonly target = OutputTarget.FILE_DISPLAY;
  readonly name = "file_display";
  
  constructor(private fileIO: FileIOService) {}
  
  supports(message: BaseMessage): boolean {
    return [
      "tool.result",
      "thread.node.start",
      "thread.node.end",
      "checkpoint.create",
      "agent.iteration.start",
    ].includes(message.type);
  }
  
  async handle(message: BaseMessage): Promise<void> {
    // Buffer and batch writes to output.md
    // Implementation follows message-output-prd.md
  }
}
```

---

## 3. Configuration Updates

### 3.1 Update CLI Config

Add to `cli-config.toml`:

```toml
[message_output]
default_level = "info"

[message_output.decider]
# Custom routing rules per message-output-prd.md

[[message_output.decider.rules]]
name = "agent-llm-stream"
types = ["agent.llm.stream"]
targets = ["tui"]
priority = 100

[[message_output.decider.rules]]
name = "human-relay-request"
types = ["agent.human_relay.request"]
targets = ["tui", "file_functional", "file_display"]
priority = 100

[[message_output.decider.rules]]
name = "tool-result"
types = ["tool.result"]
targets = ["file_display"]
priority = 100

[io]
base_dir = ".wf-agent"

[io.functional]
dir = ".wf-agent/function"
auto_cleanup = true
retention_days = 7

[io.display]
dir = ".wf-agent/display"
enable_markdown = true
```

---

## 4. Migration Checklist

### Phase 1: Core Infrastructure
- [ ] Create `FileIOService` class
- [ ] Implement message bus initialization in `CLIAppTUI`
- [ ] Create TUI, FunctionalFile, and DisplayFile handlers
- [ ] Update `cli-config.toml` with message routing rules

### Phase 2: Agent Screen Refactoring
- [ ] Replace `AgentLoopAdapter` with message bus subscription
- [ ] Update event handling to use typed message types
- [ ] Add entity context tracking (iteration count, tool calls)
- [ ] Test streaming LLM output via message bus

### Phase 3: Human Relay Migration
- [x] Create `TUIHumanRelayHandler` with file-based workflow
- [x] Integrate with `FileIOService`
- [x] Remove old `CLIHumanRelayHandler` (file-based approach used for all modes)
- [x] Test file watcher functionality
- [x] Verify timeout handling

### Phase 4: Other Screens
- [ ] Update `WorkflowScreen` with message subscriptions
- [ ] Update `DashboardScreen` with live status updates
- [ ] Add message bus dependency to all screens
- [ ] Test cross-screen message routing

### Phase 5: Testing & Validation
- [ ] Test message routing decisions match spec
- [ ] Verify file IO paths follow `file-io-prd.md`
- [ ] Test multi-session isolation
- [ ] Validate output.md format
- [ ] Performance testing with high message volume

---

## 5. Backward Compatibility

### Removed Deprecated Code

The old `CLIHumanRelayHandler` (readline-based) has been **removed** as of this refactoring.

**Migration Path**:
- All modes (TUI and headless) now use the file-based Human Relay workflow
- Users should copy prompts from `.wf-agent/function/{sessionId}/human-relay-output.txt`
- Users should paste responses to `.wf-agent/function/{sessionId}/human-relay-input.txt`
- The TUI provides visual overlays with instructions when available

### Headless Mode Support

For headless mode (non-TUI environments), the file-based workflow still works:
1. Prompt is written to the functional file
2. User manually copies/pastes using their preferred editor
3. File watcher detects the response and continues execution
4. No readline interaction required

This approach provides consistency across all execution modes.

---

## 6. References

- `file-io-prd.md`: Functional vs Presentation IO separation
- `graph-agent-message-classification.md`: Entity hierarchy and message types
- `message-output-prd.md`: Output routing decision matrix
- `message-types-migration-spec.md`: SDK-level message type definitions

---

## 7. Summary

This refactoring transforms the TUI from a **direct adapter-based architecture** to a **message-driven architecture** that:

1. ✅ Integrates with SDK message bus
2. ✅ Follows file-based Human Relay workflow (all modes)
3. ✅ Implements proper output routing (TUI vs files)
4. ✅ Supports entity hierarchy tracking
5. ✅ Enables real-time status updates across screens
6. ✅ Removes deprecated readline-based handler for consistency

**Deprecated Code Removed**: `CLIHumanRelayHandler` has been completely removed.

**Estimated Effort**: 2-3 weeks for complete refactoring of phases 1-6 implementations.
