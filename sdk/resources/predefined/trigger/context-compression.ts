/**
 * Context Compression Trigger Module
 *
 * Provides trigger definition functionality
 */

import type { TriggerTemplate, TriggerAction } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { LLM_SUMMARY_WORKFLOW_ID } from "../workflow/llm-summary.js";

/**
 * Context Compression Trigger Template Name
 */
export const CONTEXT_COMPRESSION_TRIGGER_NAME = "context_compression_trigger";

/**
 * Context compression configuration options
 */
export interface ContextCompressionConfig {
  /** Custom compression hint words */
  compressionPrompt?: string;
  /** Which triggered workflow to execute (defaults to llm_summary_workflow) */
  triggeredWorkflowId?: string;
  /** Timeout duration (in milliseconds), default is 60000 */
  timeout?: number;
  /** Maximum number of triggers (0 indicates no limit); the default value is 0. */
  maxTriggers?: number;
}

/**
 * Create a predefined context compression trigger template
 */
export function createContextCompressionTriggerTemplate(
  triggeredWorkflowIdOverride?: string,
): TriggerTemplate {
  return {
    name: CONTEXT_COMPRESSION_TRIGGER_NAME,
    description:
      "Automatically triggers a context compression sub-workflow when Token usage exceeds the limit",
    condition: {
      eventType: "CONTEXT_COMPRESSION_REQUESTED",
    },
    action: {
      type: "execute_triggered_subworkflow",
      parameters: {
        triggeredWorkflowId: triggeredWorkflowIdOverride ?? LLM_SUMMARY_WORKFLOW_ID,
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
  const template = createContextCompressionTriggerTemplate(config.triggeredWorkflowId);

  if (config.timeout !== undefined) {
    (template.action as Extract<TriggerAction, { type: "execute_triggered_subworkflow" }>).parameters.timeout = config.timeout;
  }

  if (config.maxTriggers !== undefined) {
    template.maxTriggers = config.maxTriggers;
  }

  template.updatedAt = now();
  return template;
}


