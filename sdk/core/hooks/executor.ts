/**
 * Universal Hook Executor
 *
 * Provides stateless hook execution logic that can be reused by Graph and Agent modules.
 * Includes:
 * - Hook filtering and sorting
 * - Conditional expression evaluation
 * - Parallel/serial execution
 * - Error handling
 */

import type {
  BaseHookDefinition,
  BaseHookContext,
  HookExecutionResult,
  HookExecutorConfig,
  HookHandler,
  EventEmitter,
  ContextBuilder,
} from "./types.js";
import type { EvaluationContext } from "@wf-agent/types";
import { conditionEvaluator } from "@wf-agent/common-utils";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import { buildHookExecutedEvent } from "../utils/event/builders/index.js";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * Default Executor Configuration
 */
const DEFAULT_CONFIG: Required<HookExecutorConfig> = {
  parallel: true,
  continueOnError: true,
  warnOnConditionFailure: true,
};

/**
 * Filter and Sort Hooks
 *
 * @param hooks List of Hook definitions
 * @param hookType Type of the target Hook
 * @returns List of filtered and sorted Hooks
 */
export function filterAndSortHooks<T extends BaseHookDefinition>(
  hooks: T[],
  hookType: string,
): T[] {
  return hooks
    .filter(hook => hook.hookType === hookType && hook.enabled !== false)
    .sort((a, b) => (b.weight || 0) - (a.weight || 0));
}

/**
 * Evaluate Hook Conditions
 *
 * @param hook The Hook definition
 * @param evalContext The evaluation context
 * @param warnOnFailure Whether to log a warning in case of failure
 * @returns Whether the conditions are met (returns true if no conditions are specified)
 */
export function evaluateHookCondition(
  hook: BaseHookDefinition,
  evalContext: Record<string, unknown>,
  warnOnFailure: boolean = true,
): boolean {
  if (!hook.condition) {
    return true;
  }

  try {
    // "Convert the general context into EvaluationContext."
    const evaluationContext: EvaluationContext = {
      variables: (evalContext["variables"] || {}) as Record<string, unknown>,
      input: (evalContext["input"] || {}) as Record<string, unknown>,
      output: (evalContext["output"] || {}) as Record<string, unknown>,
    };
    return conditionEvaluator.evaluate(hook.condition, evaluationContext);
  } catch (error) {
    if (warnOnFailure) {
      logger.warn(
        `Hook condition evaluation failed for "${hook.eventName}": ${getErrorMessage(error)}`,
        { eventName: hook.eventName, error: getErrorMessage(error) },
      );
    }
    return false;
  }
}

/**
 * Execute a single Hook
 *
 * @param hook The Hook definition
 * @param context The execution context
 * @param buildEvalContext The context builder
 * @param handlers The list of handlers
 * @param emitEvent The event emission function
 * @param config The executor configuration
 * @returns The execution result
 */
export async function executeSingleHook<TContext extends BaseHookContext>(
  hook: BaseHookDefinition,
  context: TContext,
  buildEvalContext: ContextBuilder<TContext>,
  handlers: HookHandler<TContext>[],
  emitEvent: EventEmitter,
  config: Required<HookExecutorConfig> = DEFAULT_CONFIG,
): Promise<HookExecutionResult> {
  const startTime = now();

  try {
    // Constructing an evaluation context
    const evalContext = buildEvalContext(context);

    // Evaluation criteria
    if (!evaluateHookCondition(hook, evalContext, config.warnOnConditionFailure)) {
      return {
        success: true,
        eventName: hook.eventName,
        executionTime: now() - startTime,
        data: { skipped: true, reason: "condition_not_met" },
      };
    }

    // Generate event data
    const eventData = resolvePayloadTemplate(hook.eventPayload || {}, evalContext);

    // Execute all processors.
    for (const handler of handlers) {
      await handler(context, hook, eventData);
    }

    // Send an event
    if (emitEvent) {
      await emitEvent(
        buildHookExecutedEvent({
          eventName: hook.eventName,
          data: eventData,
        }),
      );
    }

    return {
      success: true,
      eventName: hook.eventName,
      executionTime: now() - startTime,
      data: eventData,
    };
  } catch (error) {
    return {
      success: false,
      eventName: hook.eventName,
      executionTime: now() - startTime,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}

/**
 * Execute multiple Hooks
 *
 * @param hooks: A list of Hook definitions
 * @param context: The execution context
 * @param buildEvalContext: The context builder
 * @param handlers: A list of handlers
 * @param emitEvent: The event emission function
 * @param config: The executor configuration
 * @returns: The execution results of all Hooks
 */
export async function executeHooks<TContext extends BaseHookContext>(
  hooks: BaseHookDefinition[],
  context: TContext,
  buildEvalContext: ContextBuilder<TContext>,
  handlers: HookHandler<TContext>[],
  emitEvent: EventEmitter,
  config: HookExecutorConfig = {},
): Promise<HookExecutionResult[]> {
  const resolvedConfig = { ...DEFAULT_CONFIG, ...config };

  if (resolvedConfig.parallel) {
    // Parallel execution
    const promises = hooks.map(hook =>
      executeSingleHook(hook, context, buildEvalContext, handlers, emitEvent, resolvedConfig),
    );
    const results = await Promise.allSettled(promises);
    return results.map(r =>
      r.status === "fulfilled"
        ? r.value
        : {
            success: false,
            eventName: "unknown",
            executionTime: 0,
            error: r.reason instanceof Error ? r.reason : new Error(String(r.reason)),
          },
    );
  } else {
    // Serial execution
    const results: HookExecutionResult[] = [];
    for (const hook of hooks) {
      const result = await executeSingleHook(
        hook,
        context,
        buildEvalContext,
        handlers,
        emitEvent,
        resolvedConfig,
      );
      results.push(result);

      // If the execution is not continued and fails, then it should be interrupted.
      if (!resolvedConfig.continueOnError && !result.success) {
        break;
      }
    }
    return results;
  }
}

/**
 * Parsing Load Templates
 *
 * Support variable substitution, e.g. {{output.result}} -> actual value
 *
 * @param payload Load template
 * @param context Evaluation context
 * @returns The parsed payload
 */
export function resolvePayloadTemplate(
  payload: Record<string, unknown>,
  context: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(payload)) {
    if (key === "handler") {
      continue;
    }

    if (typeof value === "string") {
      result[key] = resolveTemplateVariable(value, context);
    } else if (typeof value === "object" && value !== null) {
      result[key] = resolvePayloadTemplate(value as Record<string, unknown>, context);
    } else {
      result[key] = value;
    }
  }

  return result;
}

/**
 * Parse template variables
 *
 * @param template The template string
 * @param context The evaluation context
 * @returns The parsed value
 */
function resolveTemplateVariable(template: string, context: Record<string, unknown>): unknown {
  // Match the {{variable}} pattern
  const match = template.match(/^\s*\{\{([^}]+)\}\}\s*$/);

  if (match) {
    // The entire string is a variable reference.
    const path = match[1]?.trim();
    if (!path) {
      return "";
    }
    const value = getNestedValue(context, path);

    // If the value is undefined, return an empty string.
    return value !== undefined ? value : "";
  }

  // Replace variable references in the string.
  return template.replace(/\{\{([^}]+)\}\}/g, (_, path) => {
    const trimmedPath = path?.trim();
    if (!trimmedPath) {
      return "";
    }
    const value = getNestedValue(context, trimmedPath);
    return value !== undefined ? String(value) : "";
  });

  // Replace variable references in the string.
  return template.replace(/\{\{([^}]+)\}\}/g, (_, path) => {
    const value = getNestedValue(context, path.trim());
    return value !== undefined ? String(value) : "";
  });
}

/**
 * Getting the value of a nested object
 *
 * @param obj object
 * @param path path (e.g. "output.result")
 * @returns value
 */
function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;

  for (const part of parts) {
    if (current === null || current === undefined) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }

  return current;
}
