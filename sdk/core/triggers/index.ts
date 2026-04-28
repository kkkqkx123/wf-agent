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
} from "./types.js";

// Matcher
export {
  defaultTriggerMatcher,
  matchTriggerCondition,
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

// Processor
export { executeCustomAction } from "./handlers/index.js";

export type { CustomActionParameters } from "./handlers/index.js";

// Custom Handler Registry (re-export from registry)
export {
  CustomHandlerRegistry,
  getCustomHandlerRegistry,
  registerCustomHandler,
  getCustomHandler,
  resetCustomHandlerRegistry,
} from "../registry/custom-handler-registry.js";

export type { CustomTriggerHandler } from "../registry/custom-handler-registry.js";
