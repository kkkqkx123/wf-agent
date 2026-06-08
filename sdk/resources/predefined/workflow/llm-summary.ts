/**
 * LLM Summary Workflow Module
 *
 * Provides workflow definition for LLM-based conversation summarization.
 * This is a simple compression strategy: use LLM to summarize, then replace
 * the original conversation context with only the summary.
 *
 * Suitable for long conversation compression scenarios without extensive tool calls.
 * More specialized compression implementations (e.g., for coding scenarios) will
 * be provided separately.
 */

import type { WorkflowTemplate, StaticNode, Edge } from "@wf-agent/types";
import type { TruncateMessageOperation } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";

/**
 * LLM Summary workflow ID
 */
export const LLM_SUMMARY_WORKFLOW_ID = "llm_summary_workflow";

/**
 * Default LLM summary prompt words
 */
export const DEFAULT_LLM_SUMMARY_PROMPT = `Please provide a compressed summary of the following history of the conversation.

Requirements:
1. retain all significant facts, decisions, and action items
2. retain requirements or constraints explicitly specified by the user
3. remove redundant greetings, transition statements and repetitive information
4. if code snippets exist, retain the description of their function and purpose, and may omit implementation details
5. limit the length of the summary to 20% of the original length

Please output the summary directly without any prefixes or explanations.`;

/**
 * LLM Summary workflow configuration options
 */
export interface LlmSummaryConfig {
  /** Custom compression hint words */
  compressionPrompt?: string;
  /** Timeout period (in milliseconds), default is 60000 */
  timeout?: number;
  /** Maximum number of triggers (0 indicates no limit); the default value is 0. */
  maxTriggers?: number;
}

/**
 * Create a predefined LLM summary workflow
 *
 * Flow: START_FROM_TRIGGER → LLM (summarize) → CONTEXT_PROCESSOR (truncate to keep only summary) → CONTINUE_FROM_TRIGGER
 *
 * Node IDs use semantic names for readability; the preprocessing pipeline
 * handles conversion to internal runtime types.
 */
export function createLlmSummaryWorkflow(
  compressionPrompt?: string,
  config: LlmSummaryConfig = {},
): WorkflowTemplate {
  const currentTime = now();

  const nodes: StaticNode[] = [
    {
      id: "llm-summary-start",
      type: "START_FROM_TRIGGER",
      name: "Start LLM Summary",
      description: "Receive the full conversation history from the main workflow execution",
      config: {
        messageInputs: [
          {
            externalName: "conversationHistory",
            internalName: "current",
            required: true,
            description: "Full conversation history to be compressed",
          },
        ],
      },
    },
    {
      id: "llm-summary-llm",
      type: "LLM",
      name: "Summarize Context",
      description: "Use LLM to generate a compressed summary of the conversation history",
      config: {
        profileId: "DEFAULT",
        contextId: "current",
        outputContext: "compressed",
        parameters: {
          systemPrompt: compressionPrompt || DEFAULT_LLM_SUMMARY_PROMPT,
        },
      },
    },
    {
      id: "llm-summary-truncate",
      type: "CONTEXT_PROCESSOR",
      name: "Replace with Summary",
      description: "Replace the original conversation context with the compressed summary only",
      config: {
        sourceContext: "current",
        targetContext: "current",
        operationConfig: {
          operation: "TRUNCATE",
          strategy: { type: "KEEP_LAST", count: 1 },
          createNewBatch: true,
        } as TruncateMessageOperation,
        operationOptions: {
          visibleOnly: true,
          target: "self",
        },
      },
    },
    {
      id: "llm-summary-end",
      type: "CONTINUE_FROM_TRIGGER",
      name: "Complete LLM Summary",
      description: "Pass the compressed conversation summary back to the main workflow execution",
      config: {
        messageOutputs: [
          {
            internalName: "current",
            externalName: "current",
            description: "Compressed conversation summary (original context replaced)",
          },
        ],
      },
    },
  ];

  const edges: Edge[] = [
    {
      id: "e-llm-summary-start-to-llm",
      sourceNodeId: "llm-summary-start",
      targetNodeId: "llm-summary-llm",
      type: "DEFAULT",
    },
    {
      id: "e-llm-summary-llm-to-truncate",
      sourceNodeId: "llm-summary-llm",
      targetNodeId: "llm-summary-truncate",
      type: "DEFAULT",
    },
    {
      id: "e-llm-summary-truncate-to-end",
      sourceNodeId: "llm-summary-truncate",
      targetNodeId: "llm-summary-end",
      type: "DEFAULT",
    },
  ];

  return {
    id: LLM_SUMMARY_WORKFLOW_ID,
    name: "LLM Summary Workflow",
    type: "TRIGGERED_SUBWORKFLOW",
    description:
      "LLM-based conversation summarization workflow: summarize → replace original context with summary",
    nodes,
    edges,
    triggeredSubworkflowConfig: {
      enableCheckpoints: false,
      timeout: config.timeout ?? 60000,
      maxRetries: config.maxTriggers ?? 0,
    },
    metadata: {
      category: "system",
      tags: ["context", "compression", "summary", "token", "memory", "predefined"],
      author: "system",
    },
    version: "1.0.0",
    createdAt: currentTime,
    updatedAt: currentTime,
  };
}

/**
 * Create a custom LLM summary workflow with configuration overrides.
 */
export function createCustomLlmSummaryWorkflow(
  config: LlmSummaryConfig = {},
): WorkflowTemplate {
  return createLlmSummaryWorkflow(config.compressionPrompt, config);
}