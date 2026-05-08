# Phase 2 Quick Reference

## Screen Message Subscriptions

### AgentScreen

**Subscribed Events**:
- `agent.start` - Agent loop started
- `agent.end` - Agent loop ended
- `agent.pause` - Agent paused
- `agent.resume` - Agent resumed
- `agent.cancel` - Agent cancelled
- `agent.iteration.start` - Iteration started
- `agent.iteration.end` - Iteration completed
- `agent.llm.stream` - LLM response chunk
- `agent.tool.call_start` - Tool execution started
- `agent.tool.call_end` - Tool execution completed

**State Updates**:
- `currentAgentId` - Set from START event
- `isRunning` - Toggled by lifecycle events
- Status panel - Updated on all lifecycle changes
- Log entries - Added for all events

---

### WorkflowScreen

**Subscribed Events**:
- `workflow.execution.node.start` - Node execution started
- `workflow.execution.node.end` - Node execution completed
- `workflow.execution.node.error` - Node execution failed
- `workflow.execution.node.skip` - Node skipped
- `workflow.execution.start` - Workflow execution started
- `workflow.execution.end` - Workflow execution ended

**UI Updates**:
- Detail panel - Shows node execution logs
- Timestamps - Added to each log entry
- Icons - ℹ️ for system, ❌ for errors

---

### DashboardScreen

**Subscribed Events**:
- `agent.start` - Increment active agents counter
- `agent.end` - Decrement active agents counter
- `workflow.execution.start` - Increment running threads counter
- `workflow.execution.end` - Decrement running threads counter

**State Tracking**:
```typescript
private activeAgents: number = 0;
private runningThreads: number = 0;
```

**Display Updates**:
- Counters update immediately on events
- Timestamp shows last update time
- No polling required

---

## Usage Examples

### Creating Screens with MessageBus

```typescript
// In CLIAppTUI.initializeScreens()
const dashboardScreen = new DashboardScreen(
  this.messageBus,  // Pass message bus
  (screenId) => this.showScreen(screenId)
);

const workflowScreen = new WorkflowScreen(
  this.messageBus,  // Pass message bus
  () => this.showScreen("dashboard")
);

const agentScreen = new AgentScreen(
  this.messageBus,  // Pass message bus
  () => this.showScreen("dashboard")
);
```

### Adding New Subscriptions

```typescript
// In screen constructor or setup method
const subscription = this.messageBus.subscribe(
  {
    categories: [MessageCategory.AGENT],
    types: [AgentMessageType.START],
  },
  (message) => this.handleMessage(message)
);
this.subscriptions.push(subscription);
```

### Handling Messages

```typescript
private handleMessage(message: BaseComponentMessage) {
  // Type-safe data extraction
  const data = message.data as YourDataType;
  
  // Process based on message type
  switch (message.type) {
    case YourMessageType.SOMETHING:
      // Handle it...
      break;
  }
}
```

### Cleanup Pattern

```typescript
destroy(): void {
  // Always cleanup subscriptions
  this.subscriptions.forEach(sub => sub.unsubscribe());
  this.subscriptions = [];
}
```

---

## Message Categories Reference

### AGENT Category
- Lifecycle: START, END, PAUSE, RESUME, CANCEL
- Iterations: ITERATION_START, ITERATION_END
- LLM: LLM_REQUEST, LLM_STREAM, LLM_RESPONSE, LLM_ERROR
- Tools: TOOL_CALL_START, TOOL_CALL_END, TOOL_RESULT, TOOL_ERROR
- Human Relay: HUMAN_RELAY_REQUEST, HUMAN_RELAY_RESPONSE
- Checkpoints: CHECKPOINT_CREATE, CHECKPOINT_RESTORE

### WORKFLOW_EXECUTION Category
- Lifecycle: START, END, PAUSE, RESUME, CANCEL
- Nodes: NODE_START, NODE_END, NODE_ERROR, NODE_SKIP
- Variables: VARIABLE_SET, VARIABLE_GET
- Parallel: FORK_START, FORK_BRANCH_START, JOIN_COMPLETE
- Agent Calls: AGENT_CALL, AGENT_RETURN
- Subgraphs: SUBGRAPH_CALL, SUBGRAPH_RETURN

---

## Common Patterns

### Filter by Multiple Types

```typescript
this.messageBus.subscribe(
  {
    categories: [MessageCategory.AGENT],
    types: [
      AgentMessageType.START,
      AgentMessageType.END,
    ],
  },
  handler
);
```

### Subscribe to All Messages in Category

```typescript
this.messageBus.subscribe(
  {
    categories: [MessageCategory.AGENT],
    // No types specified = all types in category
  },
  handler
);
```

### Subscribe to Everything

```typescript
this.messageBus.subscribeAll(handler);
```

---

## Debugging Tips

### Check Active Subscriptions

```typescript
console.log('Active subscriptions:', this.subscriptions.length);
console.log('Subscription status:', this.subscriptions.map(s => s.active));
```

### Log Received Messages

```typescript
private handleMessage(message: BaseComponentMessage) {
  console.log('Received message:', {
    type: message.type,
    category: message.category,
    timestamp: message.timestamp,
    data: message.data,
  });
  // ... handle message
}
```

### Verify Message Bus Connection

```typescript
constructor(messageBus?: MessageBus) {
  if (!messageBus) {
    console.warn('No message bus provided - real-time updates disabled');
    return;
  }
  // Setup subscriptions...
}
```

---

## Performance Considerations

### High-Frequency Events

For events that fire rapidly (e.g., LLM streaming):

```typescript
// Option 1: Throttle UI updates
private lastUpdate = 0;
private handleStream(message: BaseComponentMessage) {
  const now = Date.now();
  if (now - this.lastUpdate < 100) return; // 100ms throttle
  this.lastUpdate = now;
  // Update UI...
}

// Option 2: Batch updates
private buffer: string[] = [];
private flushTimer?: NodeJS.Timeout;

private handleStream(message: BaseComponentMessage) {
  this.buffer.push(data.chunk);
  
  if (!this.flushTimer) {
    this.flushTimer = setTimeout(() => {
      this.updateUI(this.buffer.join(''));
      this.buffer = [];
      this.flushTimer = undefined;
    }, 50); // Flush every 50ms
  }
}
```

### Memory Management

- Always unsubscribe in `destroy()`
- Don't create closures that capture large objects
- Clear buffers and timers when unsubscribing

---

## Migration Guide

### From Polling to Events

**Before (Polling)**:
```typescript
private pollInterval?: NodeJS.Timeout;

startPolling() {
  this.pollInterval = setInterval(() => {
    this.fetchStatus().then(status => this.updateUI(status));
  }, 1000);
}

destroy() {
  clearInterval(this.pollInterval);
}
```

**After (Events)**:
```typescript
private subscriptions: MessageSubscription[] = [];

setupSubscriptions() {
  const sub = this.messageBus.subscribe(
    { categories: [MessageCategory.AGENT] },
    (msg) => this.handleStatusUpdate(msg)
  );
  this.subscriptions.push(sub);
}

destroy() {
  this.subscriptions.forEach(s => s.unsubscribe());
}
```

---

## Troubleshooting

### Issue: Not Receiving Messages

**Check**:
1. Is messageBus provided to constructor?
2. Are subscriptions set up correctly?
3. Is the filter matching the message?
4. Is the subscription still active?

**Debug**:
```typescript
console.log('MessageBus available:', !!this.messageBus);
console.log('Subscriptions count:', this.subscriptions.length);
console.log('Subscription active:', this.subscriptions[0]?.active);
```

### Issue: Memory Leak

**Symptoms**:
- Application slows down over time
- Subscription count keeps growing

**Solution**:
- Ensure `destroy()` is called when screen is removed
- Verify all subscriptions are tracked in array
- Check for duplicate subscriptions

### Issue: UI Not Updating

**Check**:
1. Is message handler being called? (add console.log)
2. Is state being updated correctly?
3. Is UI re-render triggered after state change?
4. Are there TypeScript errors preventing compilation?

---

## Best Practices

1. **Always cleanup**: Unsubscribe in destroy()
2. **Type safety**: Use proper TypeScript types for message data
3. **Error handling**: Wrap handlers in try-catch if needed
4. **Logging**: Add debug logs during development
5. **Performance**: Throttle high-frequency updates
6. **Consistency**: Follow the same pattern across all screens
7. **Documentation**: Comment complex message handling logic

---

## Related Documentation

- [Phase 1 Implementation Summary](./phase-1-implementation-summary.md)
- [Phase 2 Implementation Summary](./phase-2-implementation-summary.md)
- [TUI Output Refactoring Guide](./tui-output-refactoring-guide.md)
- [Message Output PRD](../spec/message-output-prd.md)
- [File IO PRD](../spec/file-io-prd.md)
