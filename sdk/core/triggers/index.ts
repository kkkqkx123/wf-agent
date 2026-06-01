/**
 * Universal Trigger Module
 *
 * Provides a Trigger execution framework that can be reused by both the Graph and Agent modules.
 */

// Type definitions
export type {
  BaseTriggerCondition,
  BaseTriggerAction,
  BaseTriggerDefinition,
  TriggerExecutionResult,
  TriggerStatus,
  BaseEventData,
  TriggerHandler,
  TriggerMatcher,
  MatchResult,
} from "./types.js";

// Matcher
export {
  defaultTriggerMatcher,
  matchTriggers,
  createTriggerMatcher,
} from "./matcher.js";

// limiter
export {
  canTrigger,
  getTriggerStatus,
  incrementTriggerCount,
  resetTriggerCount,
  isTriggerExpired,
  getRemainingTriggers,
} from "./limiter.js";

// Executor
export { executeTriggers } from "./executor.js";
export type { TriggerExecutorConfig } from "./executor.js";