/**
 * Unified export of Agent Execution type definitions
 *
 * This module contains runtime execution types for Agent Loop.
 * Separated from agent/ (static definitions) for clear separation of concerns.
 *
 * ## Architecture Overview
 *
 * | Package | Purpose | Serializable |
 * |---------|---------|--------------|
 * | agent/ | Static definitions (templates) | Yes |
 * | agent-execution/ | Runtime execution types | Mixed |
 *
 * ## Module Structure
 *
 * - `types.ts`: Basic types (Status, Records, Result)
 * - `context.ts`: Execution context (RuntimeConfig, Options, Context)
 * - `definition.ts`: Execution instance definition
 * - `event.ts`: Streaming events
 * - `hooks.ts`: Hook types
 *
 * ## Key Types
 *
 * ### Execution Data
 * - `AgentLoopExecution`: Pure data object for execution instance
 * - `AgentLoopExecutionSnapshot`: Snapshot for checkpoint serialization
 *
 * ### Configuration
 * - `AgentLoopRuntimeConfig`: Runtime config with callbacks (not serializable)
 * - `AgentHook`: Runtime hook with Condition objects
 *
 * ### Options & Results
 * - `AgentLoopExecutionOptions`: Per-execution options
 * - `AgentLoopResult`: Execution outcome
 * - `AgentLoopExecutionResult`: Extended result with metadata
 *
 * ### State & Records
 * - `AgentLoopStatus`: Execution status enum
 * - `IterationRecord`: Iteration execution log
 * - `ToolCallRecord`: Tool call execution log
 *
 * ### Events
 * - `AgentStreamEvent`: Streaming events during execution
 * - `AgentHookTriggeredEvent`: Hook-triggered events
 *
 * @see AgentLoopDefinition - Static definition in agent/ package
 */

// Basic Types (Status, Records, Result)
export { AgentLoopStatus } from "./types.js";
export type {
  ToolCallRecord,
  IterationRecord,
  AgentLoopResult,
} from "./types.js";

// Execution Context (RuntimeConfig, Options, Context)
export type {
  AgentLoopRuntimeConfig,
  TransformContextFn,
  ConvertToLlmFn,
  AgentLoopExecutionOptions,
  AgentLoopExecutionContext,
  AgentLoopRunOptions,
  AgentLoopExecutionResult,
} from "./context.js";

// Hooks
export type { AgentHook, AgentHookType } from "./hooks.js";

// Events
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
  AgentHookTriggeredEvent,
} from "./event.js";

// Execution Definition
export type {
  AgentLoopExecution,
  AgentLoopExecutionSnapshot,
} from "./definition.js";
