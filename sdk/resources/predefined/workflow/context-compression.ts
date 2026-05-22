/**
 * Context Compression Workflow Module
 *
 * Provides workflow definition functionality
 */

import type { WorkflowTemplate, StaticNode, Edge } from "@wf-agent/types";
import { now, generateId } from "@wf-agent/common-utils";

/**
 * Context compression workflow ID
 */
export const CONTEXT_COMPRESSION_WORKFLOW_ID = "context_compression_workflow";

/**
 * Default context compression prompt words
 */
export const DEFAULT_COMPRESSION_PROMPT = `Please provide a compressed summary of the following history of the conversation.

Requirements:
1. retain all significant facts, decisions, and action items
2. retain requirements or constraints explicitly specified by the user
3. remove redundant greetings, transition statements and repetitive information
4. if code snippets exist, retain the description of their function and purpose, and may omit implementation details
5. limit the length of the summary to 20% of the original length

Please output the summary directly without any prefixes or explanations.;`;

/**
 * Context compression configuration options
 */
export interface ContextCompressionConfig {
  /** Custom compression hint words */
  compressionPrompt?: string;
  /** Timeout period (in milliseconds), default is 60000 */
  timeout?: number;
  /** Maximum number of triggers (0 indicates no limit); the default value is 0. */
  maxTriggers?: number;
}

/**
 * Create a predefined context compression workflow
 */
export function createContextCompressionWorkflow(
  compressionPrompt?: string,
  config: ContextCompressionConfig = {},
): WorkflowTemplate {
  const currentTime = now();

  // Use semantic variable names for better code readability
  const compressionStartId = generateId();
  const compressionLlmId = generateId();
  const compressionEndId = generateId();

  const nodes: StaticNode[] = [
    {
      id: compressionStartId,
      type: "START_FROM_TRIGGER",
      name: "Start Compression",
      description: "Receive the full context passed in by the main workflow execution",
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
      id: compressionLlmId,
      type: "LLM",
      name: "Compress Context",
      description: "Using LLM to compress dialog history with custom prompt",
      config: {
        profileId: "DEFAULT",
        contextId: "current",
        outputContext: "compressed",
        parameters: {
          systemPrompt: compressionPrompt || DEFAULT_COMPRESSION_PROMPT,
        },
      },
    },
    {
      id: compressionEndId,
      type: "CONTINUE_FROM_TRIGGER",
      name: "Complete Compression",
      description: "Passes compression results back to the main workflow execution",
      config: {
        messageOutputs: [
          {
            internalName: "compressed",
            externalName: "current",
            description: "Compressed conversation summary",
          },
        ],
      },
    },
  ];

  const edges: Edge[] = [
    {
      id: generateId(),
      sourceNodeId: compressionStartId,
      targetNodeId: compressionLlmId,
      type: "DEFAULT",
    },
    {
      id: generateId(),
      sourceNodeId: compressionLlmId,
      targetNodeId: compressionEndId,
      type: "DEFAULT",
    },
  ];

  // Edge IDs are now managed at runtime during preprocessing
  // No need to manually assign them in static node definitions

  return {
    id: CONTEXT_COMPRESSION_WORKFLOW_ID,
    name: "Context Compression Workflow",
    type: "TRIGGERED_SUBWORKFLOW" as const,
    description: "LLM-based context compression workflow to compress dialog history into summaries",
    nodes,
    edges,
    triggeredSubworkflowConfig: {
      enableCheckpoints: false,
      timeout: config.timeout ?? 60000,
      maxRetries: config.maxTriggers ?? 0,
    },
    metadata: {
      category: "system",
      tags: ["context", "compression", "token", "memory", "predefined"],
      author: "system",
    },
    version: "1.0.0",
    createdAt: currentTime,
    updatedAt: currentTime,
  };
}

/**
 * Create a custom configuration for context-compressed workflow
 */
export function createCustomContextCompressionWorkflow(
  config: ContextCompressionConfig = {},
): WorkflowTemplate {
  return createContextCompressionWorkflow(config.compressionPrompt, config);
}


