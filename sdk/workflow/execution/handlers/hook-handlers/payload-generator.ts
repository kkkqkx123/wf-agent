/**
 * Hook Payload Generator
 *
 * Responsible for generating event payload data when Hook is triggered.
 */

import type { NodeHook } from "@wf-agent/types";
import type { HookEvaluationContext } from "./context-builder.js";
import { renderTemplate } from "@wf-agent/common-utils";

/**
 * Generate event payload
 *
 * @param hook Hook configuration
 * @param evalContext Evaluation context
 * @returns Event payload
 */
export function generateHookEventData(
  hook: NodeHook,
  evalContext: HookEvaluationContext,
): Record<string, unknown> {
  // If hook has eventPayload configured, use it
  if (hook.eventPayload) {
    return resolvePayloadTemplate(hook.eventPayload, evalContext);
  }

  // Otherwise use default event data
  return {
    output: evalContext.output,
    status: evalContext.status,
    executionTime: evalContext.executionTime,
    error: evalContext.error,
    variables: evalContext.variables,
    config: evalContext.config,
    metadata: evalContext.metadata,
  };
}

/**
 * Resolve payload template (supports variable substitution)
 *
 * @param payload Payload template
 * @param evalContext Evaluation context
 * @returns Resolved payload
 */
export function resolvePayloadTemplate(
  payload: Record<string, unknown>,
  evalContext: HookEvaluationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(payload)) {
    if (typeof value === "string") {
      // Handle template variables like {{output.result}}
      result[key] = resolveTemplateVariable(value, evalContext);
    } else if (typeof value === "object" && value !== null) {
      // Recursively process nested objects
      result[key] = resolvePayloadTemplate(value as Record<string, unknown>, evalContext);
    } else {
      result[key] = value;
    }
  }

  return result;
}

/**
 * Resolve template variable and attempt type conversion
 *
 * @param template Template string
 * @param evalContext Evaluation context
 * @returns Resolved value (may be converted to number or boolean)
 */
function resolveTemplateVariable(template: string, evalContext: HookEvaluationContext): unknown {
  // Use unified template renderer
  const result = renderTemplate(template, evalContext as unknown as Record<string, unknown>);

  // Try to convert result to number or boolean
  if (result === "true") return true;
  if (result === "false") return false;
  if (/^-?\d+\.?\d*$/.test(result)) return parseFloat(result);

  return result;
}
