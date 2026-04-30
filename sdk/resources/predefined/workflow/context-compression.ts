/**
 * Context Compression Workflow Module
 *
 * Provides workflow definition and registration functionality
 * Combines the definition layer with the execution layer, simplifying the architecture
 */

import type { WorkflowDefinition, Node, Edge, TruncateMessageOperation } from "@wf-agent/types";
import { now, generateId } from "@wf-agent/common-utils";
import type { WorkflowRegistry } from "../../../workflow/stores/workflow-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ContextCompressionWorkflow" });

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
5. limit the length of the summary to 30% of the original length

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
export function createContextCompressionWorkflow(compressionPrompt?: string): WorkflowDefinition {
  const currentTime = now();

  const startNodeId = generateId();
  const llmNodeId = generateId();
  const processorNodeId = generateId();
  const endNodeId = generateId();

  const nodes: Node[] = [
    {
      id: startNodeId,
      type: "START_FROM_TRIGGER",
      name: "Start Compression",
      description: "Receive the full context passed in by the main workflow execution",
      config: {},
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    },
    {
      id: llmNodeId,
      type: "LLM",
      name: "Compress Context",
      description: "Using LLM to compress dialog history",
      config: {
        profileId: "DEFAULT",
        prompt: compressionPrompt || DEFAULT_COMPRESSION_PROMPT,
      },
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    },
    {
      id: processorNodeId,
      type: "CONTEXT_PROCESSOR",
      name: "Extract Result",
      description: "Preserve LLM compression results, truncate the original context",
      config: {
        operationConfig: {
          operation: "TRUNCATE",
          strategy: { type: "KEEP_LAST", count: 1 },
        } as TruncateMessageOperation,
      },
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    },
    {
      id: endNodeId,
      type: "CONTINUE_FROM_TRIGGER",
      name: "Complete Compression",
      description: "Passes compression results back to the main workflow execution",
      config: {
        conversationHistoryCallback: {
          operation: "TRUNCATE",
          truncate: {
            operation: "TRUNCATE",
            strategy: { type: "KEEP_LAST", count: 1 },
          },
        },
      },
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    },
  ];

  const edges: Edge[] = [
    {
      id: generateId(),
      sourceNodeId: startNodeId,
      targetNodeId: llmNodeId,
      type: "DEFAULT",
    },
    {
      id: generateId(),
      sourceNodeId: llmNodeId,
      targetNodeId: processorNodeId,
      type: "DEFAULT",
    },
    {
      id: generateId(),
      sourceNodeId: processorNodeId,
      targetNodeId: endNodeId,
      type: "DEFAULT",
    },
  ];

  const nodeMap = new Map(nodes.map(n => [n.id, n]));
  for (const edge of edges) {
    const sourceNode = nodeMap.get(edge.sourceNodeId);
    const targetNode = nodeMap.get(edge.targetNodeId);
    if (sourceNode) {
      sourceNode.outgoingEdgeIds.push(edge.id);
    }
    if (targetNode) {
      targetNode.incomingEdgeIds.push(edge.id);
    }
  }

  return {
    id: CONTEXT_COMPRESSION_WORKFLOW_ID,
    name: "Context Compression Workflow",
    type: "TRIGGERED_SUBWORKFLOW",
    description: "LLM-based context compression workflow to compress dialog history into summaries",
    nodes,
    edges,
    triggeredSubworkflowConfig: {
      enableCheckpoints: false,
      timeout: 60000,
      maxRetries: 0,
    },
    metadata: {
      category: "system",
      tags: ["context", "compression", "token", "memory", "predefined"],
      author: "system",
    },
    version: "1.0.0.0",
    createdAt: currentTime,
    updatedAt: currentTime,
  };
}

/**
 * Create a custom configuration for context-compressed workflow
 */
export function createCustomContextCompressionWorkflow(
  config: ContextCompressionConfig = {},
): WorkflowDefinition {
  return createContextCompressionWorkflow(config.compressionPrompt);
}

/**
 * Registering the context compression workflow
 */
export function registerContextCompressionWorkflow(
  registry: WorkflowRegistry,
  config?: ContextCompressionConfig,
  skipIfExists: boolean = true,
): boolean {
  try {
    const workflow = config?.compressionPrompt
      ? createCustomContextCompressionWorkflow(config)
      : createContextCompressionWorkflow();

    // Check if the workflow already exists
    if (registry.has(workflow.id)) {
      if (skipIfExists) {
        logger.info("Context compression workflow already exists, skipping registration");
        return false;
      }
      // If skipIfExists is false, let the registry.register throw the error
    }

    registry.register(workflow, { skipIfExists });
    logger.info("Registered context compression workflow");
    return true;
  } catch (error) {
    logger.error("Failed to register context compression workflow", { error });
    return false;
  }
}

/**
 * Cancel the context compression workflow.
 */
export function unregisterContextCompressionWorkflow(registry: WorkflowRegistry): boolean {
  try {
    if (registry.has(CONTEXT_COMPRESSION_WORKFLOW_ID)) {
      registry.unregister(CONTEXT_COMPRESSION_WORKFLOW_ID, { force: true });
      logger.info("Unregistered context compression workflow");
      return true;
    }
    return false;
  } catch (error) {
    logger.error("Failed to unregister context compression workflow", { error });
    return false;
  }
}

/**
 * Check whether the context compression workflow has been registered.
 */
export function isContextCompressionWorkflowRegistered(registry: WorkflowRegistry): boolean {
  return registry.has(CONTEXT_COMPRESSION_WORKFLOW_ID);
}
