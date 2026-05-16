/**
 * Predefined Trigger Registry
 *
 * Responsible for creating predefined trigger templates.
 */

import type { TriggerTemplate } from "@wf-agent/types";
import type { PredefinedTriggersOptions } from "./types.js";
import {
  createContextCompressionTriggerTemplate,
  createCustomContextCompressionTrigger,
  CONTEXT_COMPRESSION_TRIGGER_NAME,
} from "./context-compression.js";

/**
 * Check if the trigger is disabled.
 */
function isDisabled(triggerName: string, options?: PredefinedTriggersOptions): boolean {
  if (!options) return false;

  // If a whitelist is set, only the triggers listed in the whitelist will be enabled.
  if (options.allowList && options.allowList.length > 0) {
    return !options.allowList.includes(triggerName);
  }

  // If a blacklist is set, the triggers listed in the blacklist will be disabled.
  if (options.blockList && options.blockList.length > 0) {
    return options.blockList.includes(triggerName);
  }

  return false;
}

/**
 * Create a list of predefined trigger templates
 */
export function createPredefinedTriggers(
  options?: PredefinedTriggersOptions,
): TriggerTemplate[] {
  const triggers: TriggerTemplate[] = [];
  const config = options?.config;

  // context_compression_trigger
  if (!isDisabled(CONTEXT_COMPRESSION_TRIGGER_NAME, options)) {
    const template = config?.contextCompression
      ? createCustomContextCompressionTrigger(config.contextCompression)
      : createContextCompressionTriggerTemplate();
    triggers.push(template);
  }

  return triggers;
}
