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
import { isResourceDisabled } from "../utils.js";

/**
 * Create a list of predefined trigger templates
 */
export function createPredefinedTriggers(options?: PredefinedTriggersOptions): TriggerTemplate[] {
  const triggers: TriggerTemplate[] = [];
  const config = options?.config;

  // context_compression_trigger
  if (!isResourceDisabled(CONTEXT_COMPRESSION_TRIGGER_NAME, options)) {
    const template = config?.llmSummary
      ? createCustomContextCompressionTrigger(config.llmSummary)
      : createContextCompressionTriggerTemplate();
    triggers.push(template);
  }

  return triggers;
}
