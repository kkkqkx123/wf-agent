/**
 * Universal Trigger Executor
 *
 * Provides stateless trigger execution logic that can be reused by Graph and Agent modules.
 * Encapsulates the complete triggering pipeline:
 *   1. Match triggers against events (via matcher.ts)
 *   2. Check trigger limits (via limiter.ts)
 *   3. Execute matched triggers via handler
 *   4. Collect results with configurable error handling
 *
 * Design Principles:
 * - Stateless: Does not retain any mutable state
 * - Generic: Works with any type extending BaseTriggerDefinition
 * - Separated from business logic: Does not know about workflow/agent-specific concerns
 *
 * @see sdk/core/hooks/executor.ts for the analogous Hook executor pattern
 */

import type {
  BaseTriggerDefinition,
  BaseEventData,
  TriggerHandler,
  TriggerExecutionResult,
} from "./types.js";
import { matchTriggers } from "./matcher.js";
import { getGlobalLogger } from "@wf-agent/common-utils";

const logger = getGlobalLogger().child("TriggerExecutor", { module: "core/triggers" });

/**
 * Trigger Executor Configuration
 */
export interface TriggerExecutorConfig {
  /**
   * Error handling strategy when a trigger handler throws:
   * - "silent": Ignore errors silently, continue with remaining triggers
   * - "log" (default): Log the error, continue with remaining triggers
   * - "throw": Re-throw immediately, stop processing remaining triggers
   */
  errorHandling?: "silent" | "log" | "throw";
}

/**
 * Default executor configuration
 */
const DEFAULT_CONFIG: TriggerExecutorConfig = {
  errorHandling: "log",
};

/**
 * Execute triggers against an event.
 *
 * For each trigger:
 *   1. Checks canTrigger() from limiter (enabled + maxTriggers)
 *   2. Matches condition against event via defaultTriggerMatcher (eventType, eventName, expression)
 *   3. If matched, calls the provided handler
 *   4. Collects results with error handling per config
 *
 * @param triggers - List of triggers to evaluate and execute
 * @param event - Event data for matching
 * @param handler - Handler function called for each matched trigger
 * @param config - Execution configuration
 * @returns Array of execution results
 */
export async function executeTriggers<T extends BaseTriggerDefinition>(
  triggers: T[],
  event: BaseEventData,
  handler: TriggerHandler<T>,
  config: TriggerExecutorConfig = {},
): Promise<TriggerExecutionResult[]> {
  const resolvedConfig = {
    errorHandling: config.errorHandling ?? DEFAULT_CONFIG.errorHandling,
  } as Required<TriggerExecutorConfig>;
  const results: TriggerExecutionResult[] = [];

  // Step 1: Match triggers using matcher + limiter
  const matchedTriggers = matchTriggers(triggers, event);

  // Step 2: Execute matched triggers in serial (keeping predictable ordering)
  for (const trigger of matchedTriggers) {
    try {
      const result = await handler(trigger, event);
      results.push(result);
    } catch (error) {
      const errorResult: TriggerExecutionResult = {
        triggerId: trigger.id,
        success: false,
        action: trigger.action,
        executionTime: 0,
        error: error instanceof Error ? error : new Error(String(error)),
      };
      results.push(errorResult);

      switch (resolvedConfig.errorHandling) {
        case "silent":
          break;
        case "log":
          logger.warn("Trigger execution failed", {
            triggerId: trigger.id,
            actionType: trigger.action.type,
            error: error instanceof Error ? error.message : String(error),
          });
          break;
        case "throw":
          throw error;
      }
    }
  }

  return results;
}
