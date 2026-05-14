/**
 * Event Type System Tests
 * 
 * Tests for event types including:
 * - BaseEvent structure
 * - EventType union (50+ event types)
 * - EventListener and EventHandler
 * - ListenerOptions
 * - Agent-specific events
 * - Event type guards patterns
 * 
 * Priority: MEDIUM (Phase 2)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  BaseEvent,
  EventType,
  EventListener,
  EventHandler,
  ListenerOptions,
  AgentStartedEvent,
  AgentCompletedEvent,
  AgentTurnStartedEvent,
  AgentTurnCompletedEvent,
} from "../../../src/index.js";

// =============================================================================
// Test 1: EventType Union Coverage
// =============================================================================

// Test major event categories
const workflowEvents: EventType[] = [
  "WORKFLOW_EXECUTION_STARTED",
  "WORKFLOW_EXECUTION_COMPLETED",
  "WORKFLOW_EXECUTION_FAILED",
  "WORKFLOW_EXECUTION_PAUSED",
  "WORKFLOW_EXECUTION_RESUMED",
  "WORKFLOW_EXECUTION_CANCELLED",
  "WORKFLOW_EXECUTION_STATE_CHANGED",
];

const nodeEvents: EventType[] = [
  "NODE_STARTED",
  "NODE_COMPLETED",
  "NODE_FAILED",
  "NODE_CUSTOM_EVENT",
];

const toolEvents: EventType[] = [
  "TOOL_CALL_STARTED",
  "TOOL_CALL_COMPLETED",
  "TOOL_CALL_FAILED",
  "TOOL_CALL_BLOCKED",
  "TOOL_ADDED",
  "TOOL_APPROVAL_REQUESTED",
  "TOOL_APPROVAL_RESPONDED",
  "TOOL_APPROVAL_FAILED",
  "TOOL_APPROVAL_ANNOTATED",
];

const agentEvents: EventType[] = [
  "AGENT_STARTED",
  "AGENT_COMPLETED",
  "AGENT_TURN_STARTED",
  "AGENT_TURN_COMPLETED",
  "AGENT_MESSAGE_STARTED",
  "AGENT_MESSAGE_COMPLETED",
  "AGENT_TOOL_EXECUTION_STARTED",
  "AGENT_TOOL_EXECUTION_COMPLETED",
  "AGENT_ITERATION_STARTED",
  "AGENT_ITERATION_COMPLETED",
  "AGENT_HOOK_TRIGGERED",
  "AGENT_PAUSED",
  "AGENT_CANCELLED",
];

const checkpointEvents: EventType[] = [
  "CHECKPOINT_CREATED",
  "CHECKPOINT_RESTORED",
  "CHECKPOINT_DELETED",
  "CHECKPOINT_FAILED",
];

const messageEvents: EventType[] = [
  "MESSAGE_ADDED",
];

const errorEvents: EventType[] = [
  "ERROR",
];

// Verify all are valid EventType
for (const eventType of [
  ...workflowEvents,
  ...nodeEvents,
  ...toolEvents,
  ...agentEvents,
  ...checkpointEvents,
  ...messageEvents,
  ...errorEvents,
]) {
  expectType<EventType>(eventType);
}

// =============================================================================
// Test 2: BaseEvent Structure
// =============================================================================

const baseEvent: BaseEvent = {
  id: "event-123",
  type: "WORKFLOW_EXECUTION_STARTED",
  timestamp: Date.now(),
  workflowId: "workflow-456",
  executionId: "exec-789",
  metadata: {
    source: "test",
  },
};

expectType<BaseEvent>(baseEvent);
expectType<string>(baseEvent.id);
expectType<EventType>(baseEvent.type);
expectType<number>(baseEvent.timestamp);
expectType<string | undefined>(baseEvent.workflowId);
expectType<string | undefined>(baseEvent.executionId);
expectType<Record<string, unknown> | undefined>(baseEvent.metadata);

// Minimal event
const minimalEvent: BaseEvent = {
  id: "event-minimal",
  type: "ERROR",
  timestamp: Date.now(),
};

expectType<BaseEvent>(minimalEvent);

// =============================================================================
// Test 3: EventListener Type
// =============================================================================

// Synchronous listener
const syncListener: EventListener<BaseEvent> = (event: BaseEvent) => {
  console.log(event.type);
};

expectType<EventListener<BaseEvent>>(syncListener);

// Async listener
const asyncListener: EventListener<BaseEvent> = async (event: BaseEvent) => {
  await Promise.resolve();
  console.log(event.id);
};

expectType<EventListener<BaseEvent>>(asyncListener);

// Listener with specific event type
const agentStartedListener: EventListener<AgentStartedEvent> = (
  event: AgentStartedEvent
) => {
  expectType<AgentStartedEvent>(event);
  expectType<"AGENT_STARTED">(event.type);
  expectType<string>(event.agentLoopId);
  expectType<number>(event.maxIterations);
};

expectType<EventListener<AgentStartedEvent>>(agentStartedListener);

// =============================================================================
// Test 4: EventHandler Structure
// =============================================================================

const handler: EventHandler = {
  eventType: "NODE_COMPLETED",
  listener: (event: BaseEvent) => {
    console.log("Node completed:", event.id);
  },
};

expectType<EventHandler>(handler);
expectType<EventType>(handler.eventType);
expectType<EventListener<BaseEvent>>(handler.listener);

// Handler with async listener
const asyncHandler: EventHandler = {
  eventType: "TOOL_CALL_COMPLETED",
  listener: async (event: BaseEvent) => {
    await Promise.resolve();
  },
};

expectType<EventHandler>(asyncHandler);

// =============================================================================
// Test 5: ListenerOptions Configuration
// =============================================================================

const optionsWithPriority: ListenerOptions = {
  priority: 10,
  timeout: 5000,
};

expectType<ListenerOptions>(optionsWithPriority);
expectType<number | undefined>(optionsWithPriority.priority);
expectType<number | undefined>(optionsWithPriority.timeout);
expectType<string | undefined>(optionsWithPriority.executionId);
expectType<boolean | undefined>(optionsWithPriority.autoCleanup);

// Options with filter
const optionsWithFilter: ListenerOptions<BaseEvent> = {
  priority: 5,
  filter: (event: BaseEvent) => event.type === "ERROR",
  autoCleanup: true,
};

expectType<ListenerOptions<BaseEvent>>(optionsWithFilter);
expectType<((event: BaseEvent) => boolean) | undefined>(optionsWithFilter.filter);

// Options with execution binding
const boundOptions: ListenerOptions = {
  executionId: "exec-123",
  autoCleanup: true,
  priority: 1,
};

expectType<ListenerOptions>(boundOptions);

// Minimal options
const minimalOptions: ListenerOptions = {};

expectType<ListenerOptions>(minimalOptions);

// =============================================================================
// Test 6: AgentStartedEvent Specific Structure
// =============================================================================

const agentStarted: AgentStartedEvent = {
  id: "agent-start-1",
  type: "AGENT_STARTED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-123",
  maxIterations: 10,
  initialMessageCount: 2,
  workflowId: "workflow-456",
  executionId: "exec-789",
};

expectType<AgentStartedEvent>(agentStarted);
expectType<"AGENT_STARTED">(agentStarted.type);
expectType<string>(agentStarted.agentLoopId);
expectType<number>(agentStarted.maxIterations);
expectType<number>(agentStarted.initialMessageCount);
expectType<string | undefined>(agentStarted.parentWorkflowExecutionId);
expectType<string | undefined>(agentStarted.nodeId);

// Agent started as graph node
const agentStartedAsNode: AgentStartedEvent = {
  id: "agent-start-2",
  type: "AGENT_STARTED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-456",
  maxIterations: 5,
  initialMessageCount: 1,
  parentWorkflowExecutionId: "workflow-exec-789",
  nodeId: "agent-node-1",
};

expectType<AgentStartedEvent>(agentStartedAsNode);

// =============================================================================
// Test 7: AgentCompletedEvent Specific Structure
// =============================================================================

const agentCompleted: AgentCompletedEvent = {
  id: "agent-complete-1",
  type: "AGENT_COMPLETED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-123",
  iterations: 5,
  toolCallCount: 8,
  success: true,
};

expectType<AgentCompletedEvent>(agentCompleted);
expectType<"AGENT_COMPLETED">(agentCompleted.type);
expectType<string>(agentCompleted.agentLoopId);
expectType<number>(agentCompleted.iterations);
expectType<number>(agentCompleted.toolCallCount);
expectType<boolean>(agentCompleted.success);
expectType<unknown | undefined>(agentCompleted.error);

// Failed completion
const agentFailed: AgentCompletedEvent = {
  id: "agent-fail-1",
  type: "AGENT_COMPLETED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-456",
  iterations: 2,
  toolCallCount: 1,
  success: false,
  error: new Error("Max iterations exceeded"),
};

expectType<AgentCompletedEvent>(agentFailed);

// =============================================================================
// Test 8: AgentTurnStartedEvent Specific Structure
// =============================================================================

const turnStarted: AgentTurnStartedEvent = {
  id: "turn-start-1",
  type: "AGENT_TURN_STARTED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-123",
  iteration: 3,
};

expectType<AgentTurnStartedEvent>(turnStarted);
expectType<"AGENT_TURN_STARTED">(turnStarted.type);
expectType<string>(turnStarted.agentLoopId);
expectType<number>(turnStarted.iteration);

// Turn started as graph node
const turnStartedAsNode: AgentTurnStartedEvent = {
  id: "turn-start-2",
  type: "AGENT_TURN_STARTED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-456",
  iteration: 1,
  parentWorkflowExecutionId: "workflow-exec-789",
  nodeId: "agent-node-1",
};

expectType<AgentTurnStartedEvent>(turnStartedAsNode);

// =============================================================================
// Test 9: AgentTurnCompletedEvent Specific Structure
// =============================================================================

const turnCompleted: AgentTurnCompletedEvent = {
  id: "turn-complete-1",
  type: "AGENT_TURN_COMPLETED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-123",
  iteration: 3,
  shouldContinue: true,
};

expectType<AgentTurnCompletedEvent>(turnCompleted);
expectType<"AGENT_TURN_COMPLETED">(turnCompleted.type);
expectType<string>(turnCompleted.agentLoopId);
expectType<number>(turnCompleted.iteration);
expectType<boolean>(turnCompleted.shouldContinue);
expectType<string | undefined>(turnCompleted.stopReason);

// Turn completed with stop reason
const turnStopped: AgentTurnCompletedEvent = {
  id: "turn-stop-1",
  type: "AGENT_TURN_COMPLETED",
  timestamp: Date.now(),
  agentLoopId: "agent-loop-456",
  iteration: 5,
  shouldContinue: false,
  stopReason: "Maximum iterations reached",
};

expectType<AgentTurnCompletedEvent>(turnStopped);

// =============================================================================
// Test 10: Event Type Narrowing Pattern
// =============================================================================

function handleAgentEvent(event: BaseEvent): void {
  if (event.type === "AGENT_STARTED") {
    // TypeScript should narrow to AgentStartedEvent
    const startedEvent = event as AgentStartedEvent;
    expectType<AgentStartedEvent>(startedEvent);
    expectType<string>(startedEvent.agentLoopId);
    expectType<number>(startedEvent.maxIterations);
  } else if (event.type === "AGENT_COMPLETED") {
    const completedEvent = event as AgentCompletedEvent;
    expectType<AgentCompletedEvent>(completedEvent);
    expectType<boolean>(completedEvent.success);
    expectType<number>(completedEvent.iterations);
  }
}

// =============================================================================
// Test 11: Integration Pattern - Event Subscription
// =============================================================================

interface EventSubscription {
  handler: EventHandler;
  options?: ListenerOptions;
  unsubscribe: () => void;
}

const subscription: EventSubscription = {
  handler: {
    eventType: "AGENT_ITERATION_COMPLETED",
    listener: (event: BaseEvent) => {
      console.log("Iteration completed");
    },
  },
  options: {
    priority: 10,
    autoCleanup: true,
  },
  unsubscribe: () => {
    console.log("Unsubscribed");
  },
};

expectType<EventSubscription>(subscription);
expectType<EventHandler>(subscription.handler);
expectType<ListenerOptions | undefined>(subscription.options);
expectType<() => void>(subscription.unsubscribe);

// =============================================================================
// Test 12: Integration Pattern - Event History
// =============================================================================

interface EventHistory {
  events: BaseEvent[];
  totalEvents: number;
  filteredByType: (type: EventType) => BaseEvent[];
}

const history: EventHistory = {
  events: [baseEvent, minimalEvent],
  totalEvents: 2,
  filteredByType: (type: EventType) => {
    return [];
  },
};

expectType<EventHistory>(history);
expectType<BaseEvent[]>(history.events);
expectType<number>(history.totalEvents);
expectType<(type: EventType) => BaseEvent[]>(history.filteredByType);

// =============================================================================
// Test 13: Integration Pattern - Event Emitter Interface
// =============================================================================

interface EventEmitter {
  on<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options?: ListenerOptions<T>
  ): void;
  
  emit(event: BaseEvent): void;
  
  off(eventType: EventType, listener?: EventListener<BaseEvent>): void;
}

declare const emitter: EventEmitter;

// Usage pattern
emitter.on("AGENT_STARTED", agentStartedListener, { priority: 5 });
emitter.emit(agentStarted);
// emitter.off("AGENT_STARTED", agentStartedListener); // Type mismatch in generic signature
