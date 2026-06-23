/**
 * General Hook Module
 *
 * Provides a Hook execution framework that can be reused by Graph and Agent modules.
 */

// Type definitions
export type {
  BaseHookDefinition,
  BaseHookContext,
  HookExecutionResult,
  HookExecutorConfig,
  HookHandler,
  EventEmitter,
  ContextBuilder,
} from "./types.js";

// Executor function
export {
  filterAndSortHooks,
  evaluateHookCondition,
  executeSingleHook,
  executeHooks,
  resolvePayloadTemplate,
} from "./executor.js";
