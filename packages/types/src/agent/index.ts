/**
 * Unified export of Agent type definitions
 * Agent is an execution engine that operates independently of Graph.
 */

// State Enumeration
export { AgentLoopStatus } from "./status.js";

// Configuration Type
export type { AgentLoopConfig, TransformContextFn, ConvertToLlmFn } from "./config.js";

// Result Type
export type { AgentLoopResult } from "./result.js";

// Event Type
export { AgentStreamEventType } from "./event.js";
export type {
  AgentStreamEvent,
  AgentStartEvent,
  AgentEndEvent,
  TurnStartEvent,
  TurnEndEvent,
  MessageStartEvent,
  MessageUpdateEvent,
  MessageEndEvent,
  ToolExecutionStartEvent,
  ToolExecutionUpdateEvent,
  ToolExecutionEndEvent,
  IterationCompleteEvent,
  AgentErrorEvent,
  SteeringInjectedEvent,
  FollowupQueuedEvent,
} from "./event.js";

// Execution record type
export type { ToolCallRecord, IterationRecord } from "./records.js";

// Hook Type
export type { AgentHookType, AgentHook } from "./hooks.js";

// Checkpoint type (imported from the checkpoint/agent module)
export type {
  AgentLoopDelta,
  AgentLoopStateSnapshot,
  AgentLoopCheckpoint,
  AgentLoopCheckpointConfigContext,
  AgentLoopCheckpointConfig,
  AgentLoopCheckpointConfigLayer,
} from "../checkpoint/agent/index.js";

// Reexport the common checkpoint types (for backward compatibility)
export type {
  CheckpointType as TCheckpointType,
  CheckpointMetadata,
  CheckpointConfigResult,
} from "../checkpoint/base.js";
