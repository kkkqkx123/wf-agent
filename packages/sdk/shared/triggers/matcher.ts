/**
 * Universal Trigger Matcher
 *
 * Provides the logic for matching events with trigger conditions.
 * Supports simple field matching (eventType, eventName) and
 * expression-based matching via the ConditionEvaluator.
 */

import type { BaseTriggerCondition, BaseEventData, TriggerMatcher } from "./types.js";
import type { EvaluationContext } from "@wf-agent/types";
import { DependencyManager, conditionEvaluator } from "../../services/evaluation/index.js";
import { canTrigger } from "./limiter.js";
import type { BaseTriggerDefinition } from "./types.js";
import { getGlobalLogger } from "@wf-agent/common-utils";

const logger = getGlobalLogger().child("TriggerMatcher", { module: "core/triggers" });

// Module-level dependency manager for caching compiled trigger conditions.
// Conditions are static per trigger definition, so caching across events yields performance improvements.
const depManager = new DependencyManager();

/**
 * Build an EvaluationContext from a BaseEventData and optional execution context.
 *
 * Maps:
 *   - event fields (type, eventName, timestamp, sourceId) → variables
 *   - event.data → input
 *   - executionContext → variables (merged, takes precedence)
 *   - output → empty (not applicable at match time)
 */
function buildEvalContext(event: BaseEventData, executionContext?: Record<string, unknown>): EvaluationContext {
  return {
    variables: {
      type: event.type,
      eventName: event.eventName,
      timestamp: event.timestamp,
      sourceId: event.sourceId,
      ...(executionContext || {}),
    },
    input: (event.data as Record<string, unknown>) ?? {},
    output: {},
  };
}

/**
 * Default Trigger Matcher
 *
 * Matching Rules:
 * 1. The event type must match.
 * 2. If the condition specifies an eventName, the event must also match.
 * 3. If the condition includes an expression condition (condition.condition),
 *    it is evaluated using the ConditionEvaluator from common-utils for
 *    richer matching (e.g., "data.status == 'completed'").
 * 4. Execution context is available for condition evaluation (e.g., "iteration >= 5").
 *
 * @param condition - Trigger condition
 * @param event - Event data
 * @param executionContext - Optional execution context for condition evaluation
 * @returns Whether a match was found
 */
export const defaultTriggerMatcher: TriggerMatcher = (
  condition: BaseTriggerCondition,
  event: BaseEventData,
  executionContext?: Record<string, unknown>,
): boolean => {
  // Step 1: Check the event type.
  if (condition.eventType !== event.type) {
    logger.debug("Match failed: eventType mismatch", {
      expected: condition.eventType,
      actual: event.type,
    });
    return false;
  }

  // Step 2: If the condition specifies eventName, check whether there is a match.
  if (condition.eventName && condition.eventName !== event.eventName) {
    logger.debug("Match failed: eventName mismatch", {
      expected: condition.eventName,
      actual: event.eventName,
    });
    return false;
  }

  // Step 3: If the condition includes an expression condition, evaluate it.
  if (condition.condition) {
    const ctx = buildEvalContext(event, executionContext);
    try {
      // Handle discriminated union Condition type
      const conditionRecord = (condition.condition as unknown) as Record<string, unknown>;
      const conditionType = (conditionRecord['type'] as string) ?? "expression";

      let passed: boolean;

      if (conditionType === "expression") {
        // Use DependencyManager for expression conditions (backward compatibility)
        const exprKey = conditionRecord['expression'] as string;
        const tracked = depManager.getTrackedExpression(exprKey);
        if (tracked) {
          const result = depManager.evaluateIfChanged(exprKey, ctx);
          passed = Boolean(result);
        } else {
          depManager.register(exprKey, conditionRecord['expression'] as string, ctx);
          const tracked = depManager.getTrackedExpression(exprKey);
          passed = Boolean(tracked?.lastResult);
        }
      } else {
        // Use unified conditionEvaluator for other condition types
        passed = conditionEvaluator.evaluate(condition.condition, ctx);
      }

      if (!passed) {
        logger.debug("Match failed: condition evaluated to false", {
          type: conditionType,
          eventType: event.type,
        });
        return false;
      }
    } catch (err) {
      const conditionRecord = (condition.condition as unknown) as Record<string, unknown>;
      const conditionInfo =
        (conditionRecord['type'] as string) === "expression" || !conditionRecord['type']
          ? { expression: conditionRecord['expression'] }
          : { type: conditionRecord['type'] };
      logger.warn("Match failed: condition evaluation threw", {
        ...conditionInfo,
        error: err instanceof Error ? err.message : String(err),
      });
      return false;
    }
  }

  return true;
};

/**
 * Clear the dependency manager cache.
 * Useful for testing or when trigger definitions change at runtime.
 */
export function clearConditionCache(): void {
  depManager.clear();
}

/**
 * Match event against all triggers and return those that match.
 *
 * Integrates with the limiter (canTrigger) to skip expired or disabled triggers.
 *
 * @param triggers - List of triggers
 * @param event - Event data
 * @param matcher - Matcher (optional, default is defaultTriggerMatcher)
 * @param executionContext - Optional execution context for condition evaluation
 * @returns List of matched triggers
 */
export function matchTriggers<T extends BaseTriggerDefinition>(
  triggers: T[],
  event: BaseEventData,
  matcher: TriggerMatcher = defaultTriggerMatcher,
  executionContext?: Record<string, unknown>,
): T[] {
  return triggers.filter(trigger => {
    // Skip disabled / expired triggers (delegates to limiter).
    if (!canTrigger(trigger)) {
      return false;
    }

    // For backward compatibility, if matcher only takes 2 params, use old signature
    if (matcher.length === 2) {
      return (matcher as (condition: BaseTriggerCondition, event: BaseEventData) => boolean)(
        trigger.condition,
        event,
      );
    }

    // New signature with execution context
    return (matcher as (condition: BaseTriggerCondition, event: BaseEventData, ctx?: Record<string, unknown>) => boolean)(
      trigger.condition,
      event,
      executionContext,
    );
  });
}

/**
 * Composition strategy for createTriggerMatcher.
 */
export interface CreateTriggerMatcherOptions {
  /**
   * When to run the custom matcher relative to the default checks.
   * - "default-first" (default): run default checks first; custom runs only if default passes.
   * - "custom-first": run custom first; default runs only if custom passes.
   * - "custom-only": skip default checks entirely; use only the custom matcher.
   */
  order?: "default-first" | "custom-first" | "custom-only";
}

/**
 * Create a custom matcher.
 *
 * Factory function for composing a custom matching function with the default matcher.
 * The composition order is configurable via options.
 *
 * @param customMatcher - The custom matching function
 * @param options - Composition options
 * @returns The composed matcher
 */
export function createTriggerMatcher(
  customMatcher: (condition: BaseTriggerCondition, event: BaseEventData) => boolean,
  options: CreateTriggerMatcherOptions = {},
): TriggerMatcher {
  const { order = "default-first" } = options;

  return (condition, event) => {
    switch (order) {
      case "custom-only":
        return customMatcher(condition, event);

      case "custom-first":
        if (!customMatcher(condition, event)) {
          return false;
        }
        return defaultTriggerMatcher(condition, event);

      case "default-first":
      default:
        if (!defaultTriggerMatcher(condition, event)) {
          return false;
        }
        return customMatcher(condition, event);
    }
  };
}
