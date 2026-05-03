/**
 * Unified export of Agent type definitions
 *
 * Agent is an execution engine that operates independently of Graph.
 *
 * ## Architecture Overview
 *
 * This module exports both static definitions and runtime execution types.
 * Runtime types are re-exported from agent-execution/ for backward compatibility.
 *
 * | Category | Location | Purpose |
 * |----------|----------|---------|
 * | Static | agent/ | File-based configuration templates |
 * | Runtime | agent-execution/ | Execution instance types |
 *
 * ## Recommended Import Paths
 *
 * For new code, import directly from the specific package:
 * - Static types: `from "@wf-agent/types/agent"`
 * - Runtime types: `from "@wf-agent/types/agent-execution"`
 */

// =============================================================================
// Static Definition Types (from agent/)
// =============================================================================

// Static Definition Type (for file-based configuration)
export type { AgentLoopDefinition } from "./definition.js";

// Static Configuration Components
export type {
  AgentHookStatic,
  AgentTriggerStatic,
  AgentTriggerAction,
  AgentCheckpointConfig,
  AgentLoopMetadata,
} from "./static-config.js";

// =============================================================================
// Runtime Execution Types (re-exported from agent-execution/)
// =============================================================================

// Basic Types (Status, Records, Result)
export { AgentLoopStatus } from "../agent-execution/types.js";
export type {
  ToolCallRecord,
  IterationRecord,
  AgentLoopResult,
} from "../agent-execution/types.js";

// Execution Context (RuntimeConfig, Options, Context)
export type {
  AgentLoopRuntimeConfig,
  TransformContextFn,
  ConvertToLlmFn,
  AgentLoopExecutionOptions,
  AgentLoopExecutionContext,
  AgentLoopRunOptions,
  AgentLoopExecutionResult,
} from "../agent-execution/context.js";

// Event Types
export { AgentStreamEventType } from "../agent-execution/event.js";
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
  AgentHookTriggeredEvent,
} from "../agent-execution/event.js";

// Hook Type (runtime)
export type { AgentHookType, AgentHook } from "../agent-execution/hooks.js";

// Execution Definition Types
export type {
  AgentLoopExecution,
  AgentLoopExecutionSnapshot,
} from "../agent-execution/definition.js";

// =============================================================================
// Checkpoint Types (from checkpoint/agent/)
// =============================================================================

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
