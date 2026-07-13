/**
 * Agent Execution Type Tests
 * 
 * Tests for agent execution types including:
 * - AgentLoopExecution structure
 * - AgentLoopExecutionSnapshot
 * - IterationRecord and ToolCallRecord
 * - AgentLoopStatus and AgentLoopResult
 * - AgentHook configuration
 * 
 * Priority: MEDIUM (Phase 2)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  AgentLoopExecution,
  AgentLoopExecutionSnapshot,
  AgentLoopStatus,
  IterationRecord,
  ToolCallRecord,
  AgentLoopResult,
  AgentHook,
  AgentHookType,
} from "../../../src/index.js";

// =============================================================================
// Test 1: AgentLoopStatus Type
// =============================================================================

// Test status literal types
const createdStatus: AgentLoopStatus = "CREATED";
const runningStatus: AgentLoopStatus = "RUNNING";
const pausedStatus: AgentLoopStatus = "PAUSED";
const completedStatus: AgentLoopStatus = "COMPLETED";
const failedStatus: AgentLoopStatus = "FAILED";
const cancelledStatus: AgentLoopStatus = "CANCELLED";

expectAssignable<AgentLoopStatus>(createdStatus);
expectAssignable<AgentLoopStatus>(runningStatus);
expectAssignable<AgentLoopStatus>(pausedStatus);
expectAssignable<AgentLoopStatus>(completedStatus);
expectAssignable<AgentLoopStatus>(failedStatus);
expectAssignable<AgentLoopStatus>(cancelledStatus);

// =============================================================================
// Test 2: ToolCallRecord Structure
// =============================================================================

const toolCallRecord: ToolCallRecord = {
  id: "tool-call-123",
  name: "search",
  arguments: { query: "test" },
  result: { data: "result" },
  startTime: Date.now(),
  endTime: Date.now() + 1000,
};

expectType<ToolCallRecord>(toolCallRecord);
expectType<string>(toolCallRecord.id);
expectType<string>(toolCallRecord.name);
expectType<unknown>(toolCallRecord.arguments);
expectType<unknown | undefined>(toolCallRecord.result);
expectType<string | undefined>(toolCallRecord.error);
expectType<number>(toolCallRecord.startTime);
expectType<number | undefined>(toolCallRecord.endTime);

// Tool call without optional fields
const minimalToolCall: ToolCallRecord = {
  id: "tool-call-456",
  name: "read_file",
  arguments: { path: "/test.txt" },
  startTime: Date.now(),
};

expectType<ToolCallRecord>(minimalToolCall);

// =============================================================================
// Test 3: IterationRecord Structure
// =============================================================================

const iterationRecord: IterationRecord = {
  iteration: 1,
  startTime: Date.now(),
  endTime: Date.now() + 5000,
  toolCalls: [toolCallRecord, minimalToolCall],
  responseContent: "This is the LLM response",
};

expectType<IterationRecord>(iterationRecord);
expectType<number>(iterationRecord.iteration);
expectType<number>(iterationRecord.startTime);
expectType<number | undefined>(iterationRecord.endTime);
expectType<ToolCallRecord[]>(iterationRecord.toolCalls);
expectType<string | undefined>(iterationRecord.responseContent);

// Iteration without optional fields
const minimalIteration: IterationRecord = {
  iteration: 2,
  startTime: Date.now(),
  toolCalls: [],
};

expectType<IterationRecord>(minimalIteration);

// =============================================================================
// Test 4: AgentLoopExecution Structure
// =============================================================================

const agentExecution: AgentLoopExecution = {
  id: "exec-123",
  definitionId: "def-456",
  status: "RUNNING",
  currentIteration: 3,
  toolCallCount: 5,
  iterationHistory: [iterationRecord, minimalIteration],
  messages: [],
  startTime: Date.now(),
  endTime: Date.now() + 10000,
};

expectType<AgentLoopExecution>(agentExecution);
expectType<string>(agentExecution.id);
expectType<string>(agentExecution.definitionId);
expectType<AgentLoopStatus>(agentExecution.status);
expectType<number>(agentExecution.currentIteration);
expectType<number>(agentExecution.toolCallCount);
expectType<IterationRecord[]>(agentExecution.iterationHistory);
expectType<number>(agentExecution.startTime);
expectType<number | undefined>(agentExecution.endTime);
expectType<unknown | undefined>(agentExecution.error);

// Execution with hierarchy metadata
const executionWithHierarchy: AgentLoopExecution = {
  id: "exec-789",
  definitionId: "def-012",
  status: "COMPLETED",
  currentIteration: 5,
  toolCallCount: 10,
  iterationHistory: [],
  messages: [],
  startTime: Date.now(),
  hierarchy: {
    parent: {
      parentType: "WORKFLOW",
      parentId: "workflow-123",
      nodeId: "agent-node-1",
    },
    children: [],
    depth: 1,
    rootExecutionId: "root-workflow-id",
    rootExecutionType: "WORKFLOW",
  },
};

expectType<AgentLoopExecution>(executionWithHierarchy);

// Execution with Agent parent (delegation scenario)
const delegatedExecution: AgentLoopExecution = {
  id: "exec-delegate",
  definitionId: "def-agent",
  status: "RUNNING",
  currentIteration: 0,
  toolCallCount: 0,
  iterationHistory: [],
  messages: [],
  startTime: Date.now(),
  hierarchy: {
    parent: {
      parentType: "AGENT_LOOP",
      parentId: "parent-agent-456",
      delegationPurpose: "Code review task delegation",
    },
    children: [],
    depth: 2,
    rootExecutionId: "root-workflow-id",
    rootExecutionType: "WORKFLOW",
  },
};

expectType<AgentLoopExecution>(delegatedExecution);

// Minimal execution
const minimalExecution: AgentLoopExecution = {
  id: "exec-minimal",
  definitionId: "def-minimal",
  status: "CREATED",
  currentIteration: 0,
  toolCallCount: 0,
  iterationHistory: [],
  messages: [],
  startTime: Date.now(),
};

expectType<AgentLoopExecution>(minimalExecution);

// =============================================================================
// Test 5: AgentLoopExecutionSnapshot Structure
// =============================================================================

const snapshot: AgentLoopExecutionSnapshot = {
  id: "exec-123",
  definitionId: "def-456",
  status: "PAUSED",
  currentIteration: 2,
  toolCallCount: 3,
  iterationHistory: [iterationRecord],
  messages: [],
  startTime: Date.now(),
  isStreaming: true,
  pendingToolCalls: ["tool-1", "tool-2"],
};

expectType<AgentLoopExecutionSnapshot>(snapshot);
expectType<string>(snapshot.id);
expectType<string>(snapshot.definitionId);
expectType<AgentLoopStatus>(snapshot.status);
expectType<number>(snapshot.currentIteration);
expectType<number>(snapshot.toolCallCount);
expectType<IterationRecord[]>(snapshot.iterationHistory);
expectType<boolean | undefined>(snapshot.isStreaming);
expectType<string[] | undefined>(snapshot.pendingToolCalls);

// Snapshot without optional fields
const minimalSnapshot: AgentLoopExecutionSnapshot = {
  id: "snap-1",
  definitionId: "def-1",
  status: "COMPLETED",
  currentIteration: 1,
  toolCallCount: 0,
  iterationHistory: [],
  messages: [],
  startTime: Date.now(),
};

expectType<AgentLoopExecutionSnapshot>(minimalSnapshot);

// =============================================================================
// Test 6: AgentLoopResult Structure
// =============================================================================

const successResult: AgentLoopResult = {
  success: true,
  content: "Task completed successfully",
  iterations: 5,
  toolCallCount: 8,
  agentLoopId: "agent-123",
};

expectType<AgentLoopResult>(successResult);
expectType<boolean>(successResult.success);
expectType<string | undefined>(successResult.content);
expectType<number>(successResult.iterations);
expectType<number>(successResult.toolCallCount);
expectType<unknown | undefined>(successResult.error);
expectType<string | undefined>(successResult.agentLoopId);

// Failed result
const failedResult: AgentLoopResult = {
  success: false,
  iterations: 2,
  toolCallCount: 1,
  error: new Error("Maximum iterations exceeded"),
  agentLoopId: "agent-456",
};

expectType<AgentLoopResult>(failedResult);

// Result without optional fields
const minimalResult: AgentLoopResult = {
  success: true,
  iterations: 1,
  toolCallCount: 0,
};

expectType<AgentLoopResult>(minimalResult);

// =============================================================================
// Test 7: AgentHookType Literal Types
// =============================================================================

const beforeIteration: AgentHookType = "BEFORE_ITERATION";
const afterIteration: AgentHookType = "AFTER_ITERATION";
const beforeToolCall: AgentHookType = "BEFORE_TOOL_CALL";
const afterToolCall: AgentHookType = "AFTER_TOOL_CALL";
const beforeLLMCall: AgentHookType = "BEFORE_LLM_CALL";
const afterLLMCall: AgentHookType = "AFTER_LLM_CALL";

expectAssignable<AgentHookType>(beforeIteration);
expectAssignable<AgentHookType>(afterIteration);
expectAssignable<AgentHookType>(beforeToolCall);
expectAssignable<AgentHookType>(afterToolCall);
expectAssignable<AgentHookType>(beforeLLMCall);
expectAssignable<AgentHookType>(afterLLMCall);

// =============================================================================
// Test 8: AgentHook Configuration
// =============================================================================

const hookConfig: AgentHook = {
  hookType: "BEFORE_ITERATION",
  eventName: "iteration.start",
  enabled: true,
  weight: 10,
  createCheckpoint: true,
  checkpointDescription: "Before iteration checkpoint",
};

expectType<AgentHook>(hookConfig);
expectType<AgentHookType>(hookConfig.hookType);
expectType<string>(hookConfig.eventName);
expectType<boolean | undefined>(hookConfig.enabled);
expectType<number | undefined>(hookConfig.weight);
expectType<boolean | undefined>(hookConfig.createCheckpoint);
expectType<string | undefined>(hookConfig.checkpointDescription);

// Hook with condition
const conditionalHook: AgentHook = {
  hookType: "AFTER_TOOL_CALL",
  condition: {
    type: "expression",
    expression: "toolCallCount > 5",
  },
  eventName: "tool.heavy_usage",
  eventPayload: { threshold: 5 },
};

expectType<AgentHook>(conditionalHook);

// Minimal hook
const minimalHook: AgentHook = {
  hookType: "AFTER_LLM_CALL",
  eventName: "llm.completed",
};

expectType<AgentHook>(minimalHook);

// =============================================================================
// Test 9: Status Transition Patterns
// =============================================================================

// Simulate status transitions
function transitionStatus(current: AgentLoopStatus): AgentLoopStatus {
  switch (current) {
    case "CREATED":
      return "RUNNING";
    case "RUNNING":
      return "COMPLETED";
    case "PAUSED":
      return "RUNNING";
    default:
      return current;
  }
}

const newStatus = transitionStatus("CREATED");
expectType<AgentLoopStatus>(newStatus);

// =============================================================================
// Test 10: Integration Pattern - Execution with Hooks
// =============================================================================

interface ExecutionWithHooks {
  execution: AgentLoopExecution;
  hooks: AgentHook[];
}

const executionWithHooks: ExecutionWithHooks = {
  execution: minimalExecution,
  hooks: [
    {
      hookType: "BEFORE_ITERATION",
      eventName: "iteration.before",
      createCheckpoint: true,
    },
    {
      hookType: "AFTER_TOOL_CALL",
      eventName: "tool.after",
      weight: 5,
    },
  ],
};

expectType<ExecutionWithHooks>(executionWithHooks);
expectType<AgentLoopExecution>(executionWithHooks.execution);
expectType<AgentHook[]>(executionWithHooks.hooks);
