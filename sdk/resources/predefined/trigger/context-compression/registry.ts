/**
 * Context Compression Trigger Registry
 *
 * Definition layer: Creates trigger templates (pure functions, no side effects)
 */

import type { TriggerTemplate } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { CONTEXT_COMPRESSION_WORKFLOW_ID } from "../../../workflow/context-compression/registry.js";

/**
 * Context Compression Trigger Template Name
 */
export const CONTEXT_COMPRESSION_TRIGGER_NAME = "context_compression_trigger";

/**
 * Context compression configuration options
 */
export interface ContextCompressionConfig {
  /** Custom compression prompt */
  compressionPrompt?: string;
  /** Timeout duration (in milliseconds), default is 60000 */
  timeout?: number;
  /** Maximum number of triggers (0 indicates no limit); the default value is 0. */
  maxTriggers?: number;
}

/**
 * Create a predefined context compression trigger template
 */
export function createContextCompressionTriggerTemplate(): TriggerTemplate {
  return {
    name: CONTEXT_COMPRESSION_TRIGGER_NAME,
    description:
      "Automatically triggers a context compression sub-workflow when Token usage exceeds the limit",
    condition: {
      eventType: "CONTEXT_COMPRESSION_REQUESTED",
    },
    action: {
      type: "execute_triggered_subgraph",
      parameters: {
        triggeredWorkflowId: CONTEXT_COMPRESSION_WORKFLOW_ID,
        waitForCompletion: true,
        timeout: 60000,
        recordHistory: false,
      },
    },
    enabled: true,
    maxTriggers: 0,
    metadata: {
      category: "system",
      tags: ["context", "compression", "token", "memory"],
    },
    createdAt: now(),
    updatedAt: now(),
  };
}

/**
 * Create a custom configuration for context compression trigger templates.
 */
export function createCustomContextCompressionTrigger(
  config: ContextCompressionConfig = {},
): TriggerTemplate {
  const template = createContextCompressionTriggerTemplate();

  if (config.timeout !== undefined) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (template.action.parameters as any)["timeout"] = config.timeout;
  }

  if (config.maxTriggers !== undefined) {
    template.maxTriggers = config.maxTriggers;
  }

  const metadata = template.metadata || {};
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (metadata as any)["customConfig"] = config;
  template.metadata = metadata;

  template.updatedAt = now();
  return template;
}
