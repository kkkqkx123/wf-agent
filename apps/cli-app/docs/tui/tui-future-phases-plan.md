# TUI Migration - Future Phases Implementation Plan (Phases 7-11)

## Overview

This document outlines the implementation plan for **future phases** (7-11) of the TUI migration, focusing on advanced features that require the message-based architecture established in the refactoring guide.

**Prerequisites**: Complete the output system refactoring documented in `tui-output-refactoring-guide.md` before starting these phases.

---

## Phase 7: Agent Loop Module with Message Routing (1-2 weeks)

### Goals

Enhance the Agent screen with full message routing support and real-time status tracking based on SDK message types.

### Tasks

#### 7.1 Enhanced Message Subscription

Implement granular message filtering by entity hierarchy:

```typescript
// src/tui/screens/agent-screen.ts

private setupMessageSubscriptions() {
  if (!this.currentAgentId) return;
  
  // Subscribe to all agent messages for this instance
  this.messageBus.subscribe(
    {
      categories: [MessageCategory.AGENT],
      entityIds: [this.currentAgentId],
    },
    (message) => this.handleAgentMessage(message)
  );
  
  // Also subscribe to child entities (if this agent spawns sub-agents)
  this.messageBus.subscribe(
    {
      categories: [MessageCategory.AGENT],
      custom: (msg) => msg.entity.parentId === this.currentAgentId,
    },
    (message) => this.handleChildAgentMessage(message)
  );
}
```

#### 7.2 Iteration Tracking Panel

Add a dedicated panel showing iteration progress:

```typescript
interface IterationPanel extends Component {
  private iterations: Map<number, IterationInfo> = new Map();
  
  updateIteration(data: AgentIterationData) {
    this.iterations.set(data.iteration, {
      number: data.iteration,
      toolCallCount: data.toolCallCount,
      messageCount: data.messageCount,
      status: data.status,
      startTime: Date.now(),
    });
    
    this.render();
  }
  
  render(): string[] {
    const lines: string[] = [];
    lines.push("=== Iteration Progress ===");
    
    for (const [num, info] of this.iterations.entries()) {
      const icon = info.status === "running" ? "▶️" : 
                   info.status === "waiting" ? "⏸️" : "✓";
      lines.push(
        `${icon} Iteration ${num}: ` +
        `${info.toolCallCount} tools, ` +
        `${info.messageCount} messages`
      );
    }
    
    return lines;
  }
}
```

#### 7.3 Tool Call Visualization

Create a visual indicator for tool calls:

```typescript
interface ToolCallIndicator extends Component {
  private activeCalls: Map<string, ToolCallInfo> = new Map();
  
  handleToolCallStart(data: AgentToolCallData) {
    this.activeCalls.set(data.toolCallId, {
      id: data.toolCallId,
      name: data.toolName,
      arguments: data.arguments,
      startTime: Date.now(),
      status: "running",
    });
    this.invalidate();
  }
  
  handleToolCallEnd(toolCallId: string, success: boolean) {
    const call = this.activeCalls.get(toolCallId);
    if (call) {
      call.status = success ? "completed" : "failed";
      call.endTime = Date.now();
      call.duration = call.endTime - call.startTime;
    }
    this.invalidate();
  }
  
  render(): string[] {
    const lines: string[] = [];
    lines.push("=== Active Tool Calls ===");
    
    for (const call of this.activeCalls.values()) {
      const icon = call.status === "running" ? "🔄" :
                   call.status === "completed" ? "✓" : "✗";
      const duration = call.duration ? `${call.duration}ms` : "...";
      
      lines.push(`${icon} ${call.name} (${duration})`);
    }
    
    return lines;
  }
}
```

#### 7.4 Streaming Performance Optimization

Implement efficient streaming updates:

```typescript
private appendLog(
  message: string, 
  type: LogEntry["type"], 
  options?: { stream?: boolean }
) {
  if (options?.stream) {
    // For streaming, append without full re-render
    this.logPanel.appendText(message);
  } else {
    // For non-streaming, add as new entry
    this.logEntries.push({
      timestamp: new Date(),
      type,
      message,
    });
    
    // Keep only last 100 entries for performance
    if (this.logEntries.length > 100) {
      this.logEntries.shift();
    }
    
    this.refreshLogPanel();
  }
}
```

### Deliverables

- ✅ Enhanced agent screen with message bus integration
- ✅ Iteration tracking panel
- ✅ Tool call visualization
- ✅ Optimized streaming performance
- ✅ Unit tests for message handling

---

## Phase 8: Human Relay File-Based Workflow (1 week)

### Goals

Implement the complete file-based Human Relay workflow as specified in `file-io-prd.md`.

### Tasks

#### 8.1 TUIHumanRelayHandler Implementation

Already documented in `tui-output-refactoring-guide.md`, but here are additional details:

**Key Features:**
- Instruction overlay with clear file paths
- Recent message preview (last 3 messages, truncated)
- File watcher with timeout
- No inline editor (pure file-based)

**Testing Scenarios:**
1. Normal flow: User saves file within timeout
2. Timeout: User doesn't respond in time
3. Empty file: User saves empty response
4. Large response: Handle multi-KB responses
5. Concurrent requests: Multiple agents waiting simultaneously

#### 8.2 File Watcher Service

Create a robust file watcher:

```typescript
// src/io/file-watcher.ts

export interface WatchOptions {
  path: string;
  timeout: number;
  pollInterval?: number; // Default: 500ms
  onChange: (content: string) => void;
  onTimeout?: () => void;
  onError?: (error: Error) => void;
}

export class FileWatcher {
  private watchers: Map<string, fs.FSWatcher> = new Map();
  
  watch(options: WatchOptions): void {
    const { path, timeout, pollInterval = 500 } = options;
    
    // Ensure file exists
    fs.writeFile(path, "", "utf-8").catch(() => {});
    
    let responded = false;
    let lastContent = "";
    
    // Use polling for cross-platform compatibility
    const interval = setInterval(async () => {
      if (responded) {
        clearInterval(interval);
        return;
      }
      
      try {
        const content = await fs.readFile(path, "utf-8");
        
        // Check if content changed and is non-empty
        if (content !== lastContent && content.trim().length > 0) {
          responded = true;
          clearInterval(interval);
          this.watchers.delete(path);
          options.onChange(content);
        }
        
        lastContent = content;
      } catch (error) {
        options.onError?.(error as Error);
      }
    }, pollInterval);
    
    // Timeout
    setTimeout(() => {
      if (!responded) {
        responded = true;
        clearInterval(interval);
        this.watchers.delete(path);
        options.onTimeout?.();
      }
    }, timeout);
    
    this.watchers.set(path, { close: () => clearInterval(interval) } as any);
  }
  
  unwatch(path: string): void {
    const watcher = this.watchers.get(path);
    if (watcher) {
      watcher.close();
      this.watchers.delete(path);
    }
  }
}
```

#### 8.3 Session Directory Management

Implement automatic session cleanup:

```typescript
// src/io/session-manager.ts

export class SessionManager {
  private baseDir: string;
  private retentionDays: number;
  
  constructor(baseDir: string, retentionDays: number = 7) {
    this.baseDir = baseDir;
    this.retentionDays = retentionDays;
  }
  
  /**
   * Generate unique session ID
   */
  generateSessionId(name: string): string {
    const sanitized = name
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, '-')
      .substring(0, 30);
    const timestamp = Date.now();
    return `session-${sanitized}-${timestamp}`;
  }
  
  /**
   * Create session directories
   */
  async createSession(sessionId: string): Promise<void> {
    const functionDir = path.join(this.baseDir, "function", sessionId);
    const displayDir = path.join(this.baseDir, "display", sessionId);
    
    await fs.mkdir(functionDir, { recursive: true });
    await fs.mkdir(displayDir, { recursive: true });
  }
  
  /**
   * Cleanup old sessions
   */
  async cleanupOldSessions(): Promise<void> {
    const cutoffTime = Date.now() - (this.retentionDays * 24 * 60 * 60 * 1000);
    
    const functionDir = path.join(this.baseDir, "function");
    const sessions = await fs.readdir(functionDir);
    
    for (const session of sessions) {
      const sessionPath = path.join(functionDir, session);
      const stats = await fs.stat(sessionPath);
      
      if (stats.mtimeMs < cutoffTime) {
        // Delete old session
        await fs.rm(sessionPath, { recursive: true, force: true });
        
        // Also delete display directory
        const displayPath = path.join(this.baseDir, "display", session);
        await fs.rm(displayPath, { recursive: true, force: true });
      }
    }
  }
}
```

### Deliverables

- ✅ Complete TUIHumanRelayHandler implementation
- ✅ Robust file watcher service
- ✅ Session directory management
- ✅ Automatic cleanup of old sessions
- ✅ Integration tests for file-based workflow

---

## Phase 9: Thread Execution with Hierarchy Support (1-2 weeks)

### Goals

Support complex execution scenarios including Fork/Join parallel branches, nested Subgraphs, and hierarchical navigation.

### Tasks

#### 9.1 Instance Manager

Track all execution instances and their relationships:

```typescript
// src/tui/instance-manager.ts

export interface InstanceContext {
  instanceId: string;
  type: 'thread' | 'agent' | 'subgraph';
  parentId?: string;
  rootId: string;
  depth: number;
  parallelGroup?: {
    groupId: string;
    branchIndex: number;
    totalBranches: number;
  };
  status: 'idle' | 'running' | 'paused' | 'waiting' | 'completed' | 'error';
  metadata: Record<string, unknown>;
  startedAt?: number;
  completedAt?: number;
}

export class InstanceManager {
  private instances: Map<string, InstanceContext> = new Map();
  private messageBus: MessageBusAPI;
  
  constructor(messageBus: MessageBusAPI) {
    this.messageBus = messageBus;
    this.setupMessageListeners();
  }
  
  private setupMessageListeners() {
    // Listen for instance lifecycle events
    this.messageBus.subscribe(
      {},
      (message) => this.handleInstanceEvent(message)
    );
  }
  
  private handleInstanceEvent(message: BaseMessage) {
    switch (message.type) {
      case ThreadMessageType.START:
      case AgentMessageType.START:
      case SubgraphMessageType.START:
        this.registerInstance(message);
        break;
        
      case ThreadMessageType.END:
      case AgentMessageType.END:
      case SubgraphMessageType.END:
        this.updateInstanceStatus(message.entity.id, 'completed');
        break;
        
      case ThreadMessageType.PAUSE:
      case AgentMessageType.PAUSE:
        this.updateInstanceStatus(message.entity.id, 'paused');
        break;
        
      case ThreadMessageType.RESUME:
      case AgentMessageType.RESUME:
        this.updateInstanceStatus(message.entity.id, 'running');
        break;
    }
  }
  
  registerInstance(message: BaseMessage) {
    const context: InstanceContext = {
      instanceId: message.entity.id,
      type: message.entity.type,
      parentId: message.entity.parentId,
      rootId: message.entity.rootId,
      depth: message.entity.depth,
      parallelGroup: message.entity.parallelGroup,
      status: 'running',
      metadata: message.data as Record<string, unknown>,
      startedAt: message.timestamp,
    };
    
    this.instances.set(context.instanceId, context);
  }
  
  get(instanceId: string): InstanceContext | undefined {
    return this.instances.get(instanceId);
  }
  
  getChildren(parentId: string): InstanceContext[] {
    return Array.from(this.instances.values())
      .filter(ctx => ctx.parentId === parentId);
  }
  
  getDescendants(rootId: string): InstanceContext[] {
    const result: InstanceContext[] = [];
    const collect = (id: string) => {
      const children = this.getChildren(id);
      for (const child of children) {
        result.push(child);
        collect(child.instanceId);
      }
    };
    collect(rootId);
    return result;
  }
  
  updateInstanceStatus(instanceId: string, status: InstanceContext['status']) {
    const ctx = this.instances.get(instanceId);
    if (ctx) {
      ctx.status = status;
      if (status === 'completed' || status === 'error') {
        ctx.completedAt = Date.now();
      }
    }
  }
  
  getAllInstances(): InstanceContext[] {
    return Array.from(this.instances.values());
  }
  
  getRootInstances(): InstanceContext[] {
    return Array.from(this.instances.values())
      .filter(ctx => ctx.depth === 0);
  }
}
```

#### 9.2 Hierarchical Thread Screen

Create a thread screen that shows parent-child relationships:

```typescript
// src/tui/screens/thread-hierarchy-screen.ts

export class ThreadHierarchyScreen implements Screen {
  private container: Container;
  private mainThreadPanel: Box;
  private subInstancesPanel: SelectList;
  private instanceManager: InstanceManager;
  private currentThreadId: string;
  private fileIO: FileIOService;
  
  constructor(
    threadId: string,
    instanceManager: InstanceManager,
    fileIO: FileIOService,
    onBack?: () => void
  ) {
    this.currentThreadId = threadId;
    this.instanceManager = instanceManager;
    this.fileIO = fileIO;
    this.setupLayout();
    this.loadSubInstances();
  }
  
  private loadSubInstances() {
    const children = this.instanceManager.getChildren(this.currentThreadId);
    
    this.subInstancesPanel.setItems(
      children.map(child => ({
        id: child.instanceId,
        label: this.formatInstanceLabel(child),
        description: this.formatInstanceDescription(child),
      }))
    );
  }
  
  private formatInstanceLabel(ctx: InstanceContext): string {
    const icons = {
      thread: "🧵",
      agent: "🤖",
      subgraph: "🔀",
    };
    
    const statusIcons = {
      idle: "⏸️",
      running: "▶️",
      paused: "⏸️",
      waiting: "⏳",
      completed: "✅",
      error: "❌",
    };
    
    return `${icons[ctx.type]} ${ctx.instanceId} ${statusIcons[ctx.status]}`;
  }
  
  private formatInstanceDescription(ctx: InstanceContext): string {
    const parts: string[] = [];
    
    if (ctx.parallelGroup) {
      parts.push(`Branch ${ctx.parallelGroup.branchIndex + 1}/${ctx.parallelGroup.totalBranches}`);
    }
    
    if (ctx.startedAt) {
      const duration = Date.now() - ctx.startedAt;
      parts.push(`${Math.round(duration / 1000)}s`);
    }
    
    return parts.join(' | ');
  }
  
  private setupLayout() {
    this.container = new Container();
    
    // Main thread view
    this.mainThreadPanel = new Box({ border: true, title: "Main Thread" });
    
    // Sub-instances panel
    this.subInstancesPanel = new SelectList({
      onSelect: (item) => this.showInstanceDetail(item.id),
    });
    
    const subPanel = new Box({ 
      border: true, 
      title: "Sub-Instances (Fork/Subgraph/Agent)" 
    });
    subPanel.addChild(this.subInstancesPanel);
    
    // Quick actions
    const actionsBox = new Box();
    actionsBox.addChild(new Text("[O]pen output.md  [R]efresh  [B]ack", 1, 0));
    
    this.container.addChild(actionsBox);
    this.container.addChild(this.mainThreadPanel);
    this.container.addChild(subPanel);
  }
  
  private showInstanceDetail(instanceId: string) {
    const ctx = this.instanceManager.get(instanceId);
    if (!ctx) return;
    
    const overlay = new Box({
      border: true,
      title: `Instance Details: ${ctx.instanceId}`,
      width: "70%",
      maxHeight: "70%",
    });
    
    overlay.addChild(new Text(`Type: ${ctx.type.toUpperCase()}`, { style: "bold" }));
    overlay.addChild(new Text(`Status: ${ctx.status.toUpperCase()}`));
    overlay.addChild(new Text(`Depth: ${ctx.depth}`));
    overlay.addChild(new Text(`Parent: ${ctx.parentId || 'None (root)'}`));
    overlay.addChild(new Text(`Root: ${ctx.rootId}`));
    
    if (ctx.parallelGroup) {
      overlay.addChild(new Spacer());
      overlay.addChild(new Text("Parallel Execution:", { style: "bold" }));
      overlay.addChild(new Text(
        `Group: ${ctx.parallelGroup.groupId}\n` +
        `Branch: ${ctx.parallelGroup.branchIndex + 1} of ${ctx.parallelGroup.totalBranches}`
      ));
    }
    
    if (ctx.startedAt) {
      overlay.addChild(new Spacer());
      overlay.addChild(new Text("Timing:", { style: "bold" }));
      overlay.addChild(new Text(`Started: ${new Date(ctx.startedAt).toLocaleString()}`));
      if (ctx.completedAt) {
        const duration = ctx.completedAt - ctx.startedAt;
        overlay.addChild(new Text(`Duration: ${Math.round(duration / 1000)}s`));
      }
    }
    
    // Link to output.md
    const paths = this.fileIO.getPaths(ctx.instanceId);
    overlay.addChild(new Spacer());
    overlay.addChild(new Text("Files:", { style: "bold" }));
    overlay.addChild(new Text(`Output: ${paths.display.output}`, { style: "dim underline" }));
    
    this.tui.showOverlay(overlay, { anchor: "center" });
  }
  
  private openOutputFile() {
    const paths = this.fileIO.getPaths(this.currentThreadId);
    
    // Show notification
    this.tui.showNotification({
      type: 'info',
      message: `Opening: ${paths.display.output}`,
    });
    
    // Open in external editor
    const { exec } = require('child_process');
    const editor = process.env.EDITOR || 'nano';
    exec(`${editor} "${paths.display.output}"`);
  }
  
  render(): Container {
    return this.container;
  }
  
  handleInput(data: string): boolean {
    if (data === "o" || data === "O") {
      this.openOutputFile();
      return true;
    }
    
    if (data === "r" || data === "R") {
      this.loadSubInstances();
      return true;
    }
    
    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }
    
    return this.subInstancesPanel.handleInput?.(data) || false;
  }
}
```

#### 9.3 Fork/Join Visualization

Add visual indicators for parallel execution:

```typescript
// src/tui/components/fork-join-indicator.ts

export class ForkJoinIndicator extends Component {
  private forkGroups: Map<string, ForkGroupInfo> = new Map();
  
  handleForkStart(groupId: string, totalBranches: number) {
    this.forkGroups.set(groupId, {
      groupId,
      totalBranches,
      completedBranches: 0,
      status: 'running',
      branches: [],
    });
    this.invalidate();
  }
  
  handleBranchComplete(groupId: string, branchIndex: number) {
    const group = this.forkGroups.get(groupId);
    if (group) {
      group.completedBranches++;
      group.branches.push(branchIndex);
      
      if (group.completedBranches >= group.totalBranches) {
        group.status = 'completed';
      }
      
      this.invalidate();
    }
  }
  
  render(width: number): string[] {
    const lines: string[] = [];
    
    for (const group of this.forkGroups.values()) {
      const progress = Math.round(
        (group.completedBranches / group.totalBranches) * 100
      );
      
      const bar = this.createProgressBar(progress, 20);
      
      lines.push(`Fork Group: ${group.groupId}`);
      lines.push(`[${bar}] ${progress}% (${group.completedBranches}/${group.totalBranches})`);
      lines.push('');
    }
    
    return lines;
  }
  
  private createProgressBar(progress: number, width: number): string {
    const filled = Math.round((progress / 100) * width);
    const empty = width - filled;
    
    return '█'.repeat(filled) + '░'.repeat(empty);
  }
}
```

### Deliverables

- ✅ InstanceManager for tracking execution hierarchy
- ✅ Hierarchical thread screen with sub-instance navigation
- ✅ Fork/Join visualization components
- ✅ Quick access to output.md files
- ✅ Real-time status synchronization across instances

---

## Phase 10: Other Modules (1 week)

### Goals

Complete remaining TUI modules: checkpoint management, settings, and log viewing.

### Tasks

#### 10.1 Checkpoint Management Screen

```typescript
// src/tui/screens/checkpoint-screen.ts

export class CheckpointScreen implements Screen {
  private container: Container;
  private checkpointList: SelectList;
  private detailPanel: Box;
  
  constructor(onBack?: () => void) {
    this.setupLayout();
    this.loadCheckpoints();
  }
  
  private async loadCheckpoints() {
    const checkpoints = await this.checkpointService.list();
    
    this.checkpointList.setItems(
      checkpoints.map(cp => ({
        id: cp.id,
        label: cp.name,
        description: `${cp.createdAt} | ${cp.size} KB`,
      }))
    );
  }
  
  private setupLayout() {
    // Similar to workflow screen pattern
  }
}
```

#### 10.2 Settings Configuration Interface

```typescript
// src/tui/screens/settings-screen.ts

export class SettingsScreen implements Screen {
  private container: Container;
  private settingsList: SettingsList;
  
  constructor(onBack?: () => void) {
    this.setupLayout();
  }
  
  private setupLayout() {
    this.settingsList = new SettingsList({
      items: [
        {
          key: 'theme',
          label: 'Theme',
          type: 'select',
          options: ['dark', 'light', 'auto'],
          value: 'dark',
        },
        {
          key: 'logLevel',
          label: 'Log Level',
          type: 'select',
          options: ['debug', 'info', 'warn', 'error'],
          value: 'info',
        },
        {
          key: 'autoCleanup',
          label: 'Auto Cleanup Sessions',
          type: 'boolean',
          value: true,
        },
        {
          key: 'retentionDays',
          label: 'Session Retention (days)',
          type: 'number',
          value: 7,
          min: 1,
          max: 30,
        },
      ],
      onSave: (settings) => this.saveSettings(settings),
    });
    
    this.container.addChild(this.settingsList);
  }
}
```

#### 10.3 Log Viewer with Filtering

```typescript
// src/tui/screens/log-viewer-screen.ts

export class LogViewerScreen implements Screen {
  private container: Container;
  private logPanel: Box;
  private filterInput: Input;
  private logs: LogEntry[] = [];
  private filteredLogs: LogEntry[] = [];
  
  constructor(onBack?: () => void) {
    this.setupLayout();
    this.loadLogs();
  }
  
  private setupLayout() {
    // Filter bar
    const filterBox = new Box();
    filterBox.addChild(new Text("Filter:", 1, 0));
    this.filterInput = new Input("Enter filter...");
    this.filterInput.onChange = (text) => this.applyFilter(text);
    filterBox.addChild(this.filterInput);
    
    // Log display
    this.logPanel = new Box({ scrollable: true });
    
    this.container.addChild(filterBox);
    this.container.addChild(this.logPanel);
  }
  
  private applyFilter(text: string) {
    if (!text) {
      this.filteredLogs = [...this.logs];
    } else {
      const lower = text.toLowerCase();
      this.filteredLogs = this.logs.filter(log =>
        log.message.toLowerCase().includes(lower) ||
        log.type.includes(lower)
      );
    }
    this.refreshLogPanel();
  }
}
```

### Deliverables

- ✅ Checkpoint management screen
- ✅ Settings configuration interface
- ✅ Log viewer with filtering
- ✅ All screens integrated with message bus

---

## Phase 11: Optimization & Testing (1 week)

### Goals

Optimize performance, ensure accessibility, and validate multi-instance scenarios.

### Tasks

#### 11.1 Performance Optimization

**Virtual Scrolling for Large Lists:**

```typescript
// Implement virtual scrolling for log panels with 1000+ entries
class VirtualScrollPanel extends Component {
  private viewportHeight: number = 20;
  private scrollTop: number = 0;
  
  render(width: number): string[] {
    const start = this.scrollTop;
    const end = Math.min(start + this.viewportHeight, this.items.length);
    
    return this.items.slice(start, end).map(item => item.render(width));
  }
}
```

**Debounced Updates:**

```typescript
// Debounce frequent status updates
private debouncedUpdate = debounce((status: string) => {
  this.updateStatus(status);
}, 100);
```

**Message Batching:**

```typescript
// Batch display file writes
class DisplayFileHandler {
  private buffer: Map<string, BaseMessage[]> = new Map();
  private flushTimer: NodeJS.Timeout | null = null;
  
  async handle(message: BaseMessage): Promise<void> {
    const sessionMessages = this.buffer.get(message.sessionId) || [];
    sessionMessages.push(message);
    this.buffer.set(message.sessionId, sessionMessages);
    
    // Flush after 1 second or 50 messages
    if (sessionMessages.length >= 50) {
      await this.flushSession(message.sessionId);
    } else if (!this.flushTimer) {
      this.flushTimer = setTimeout(() => {
        this.flushAll();
      }, 1000);
    }
  }
}
```

#### 11.2 Accessibility Improvements

**Keyboard Navigation:**

```typescript
// Ensure all interactive elements are keyboard accessible
interface AccessibleComponent extends Component {
  getAccessibleName(): string;
  getAccessibleRole(): string;
  isFocusable(): boolean;
}
```

**Screen Reader Support:**

```typescript
// Add ARIA-like attributes for terminal screen readers
class AccessibleText extends Text {
  render(width: number): string[] {
    const lines = super.render(width);
    
    // Add semantic markers
    if (this.role === 'alert') {
      return lines.map(line => `[ALERT] ${line}`);
    }
    
    return lines;
  }
}
```

#### 11.3 Multi-Instance Scenario Testing

**Test Cases:**

1. **Fork/Join Parallel Execution**
   - Create workflow with Fork node (3 branches)
   - Verify each branch appears in sub-instances panel
   - Test Join node waits for all branches
   - Validate aggregated output.md

2. **Nested Subgraph Calls**
   - Parent thread → Subgraph → Agent Loop
   - Verify depth tracking (0 → 1 → 2)
   - Test navigation between levels
   - Check message routing to correct parents

3. **Multiple Concurrent Agents**
   - Start 3 agents simultaneously
   - Verify isolation (messages don't mix)
   - Test individual pause/resume
   - Validate separate output files

4. **Human Relay Under Load**
   - Trigger 5 concurrent Human Relay requests
   - Verify file isolation (different sessions)
   - Test timeout handling
   - Check response routing to correct agents

#### 11.4 Cross-Platform Testing

**Platforms:**
- Linux (Ubuntu, Fedora)
- macOS (Intel, Apple Silicon)
- Windows (PowerShell, CMD, WSL)
- Termux (Android)

**Terminal Emulators:**
- iTerm2 (macOS)
- Windows Terminal
- GNOME Terminal
- Kitty (test Kitty protocol support)

### Deliverables

- ✅ Performance benchmarks (target: <50ms render time)
- ✅ Accessibility audit report
- ✅ Multi-instance test suite
- ✅ Cross-platform compatibility matrix
- ✅ Production-ready TUI application

---

## Implementation Timeline

| Phase | Duration | Start | End | Dependencies |
|-------|----------|-------|-----|--------------|
| Phase 7 | 1-2 weeks | Week 1 | Week 2 | Output refactoring complete |
| Phase 8 | 1 week | Week 3 | Week 3 | Phase 7 complete |
| Phase 9 | 1-2 weeks | Week 4 | Week 5 | Phase 8 complete |
| Phase 10 | 1 week | Week 6 | Week 6 | Phase 9 complete |
| Phase 11 | 1 week | Week 7 | Week 7 | All phases complete |

**Total Estimated Time**: 5-7 weeks

---

## Success Criteria

### Functional Requirements

- [ ] All screens use message bus instead of direct adapter calls
- [ ] Human Relay uses file-based workflow (no inline editor)
- [ ] Multi-instance hierarchy properly tracked and displayed
- [ ] Fork/Join parallel execution visualized correctly
- [ ] Nested Subgraph navigation works seamlessly
- [ ] Output routing follows specification documents
- [ ] File IO separation (functional vs display) enforced

### Performance Requirements

- [ ] TUI render time < 50ms for typical screens
- [ ] Streaming LLM output smooth (no visible lag)
- [ ] File watcher responds within 1 second of save
- [ ] Memory usage < 200MB for 100 concurrent instances
- [ ] Log panel handles 10,000+ entries via virtual scrolling

### Quality Requirements

- [ ] 90%+ code coverage for new components
- [ ] All critical paths have integration tests
- [ ] Cross-platform testing passed on all target platforms
- [ ] Accessibility audit score > 90/100
- [ ] Zero breaking changes for existing CLI commands

---

## References

- `tui-output-refactoring-guide.md`: Prerequisites for these phases
- `file-io-prd.md`: File-based Human Relay workflow
- `graph-agent-message-classification.md`: Entity hierarchy model
- `message-output-prd.md`: Output routing decisions
- `message-types-migration-spec.md`: SDK message type definitions

---

## Notes

1. **Incremental Development**: Each phase builds on the previous one. Don't skip phases.

2. **Testing Early**: Write tests alongside implementation, not after.

3. **User Feedback**: Get feedback from actual users after Phase 8 (Human Relay) to validate the file-based workflow.

4. **Performance Monitoring**: Add performance metrics early (Phase 7) to catch regressions.

5. **Documentation**: Update user-facing documentation as each phase completes.
