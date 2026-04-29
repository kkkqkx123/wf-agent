/**
 * ID Tool Functions
 * Provide ID generation and validation capabilities
 */

import { z } from "zod";
import type { ID } from "@wf-agent/types";

/**
 * Generate a new ID (using UUID v4).
 */
export function generateId(): ID {
  return crypto.randomUUID();
}

/**
 * Verify if the ID is valid.
 */
export function isValidId(id: ID): boolean {
  return typeof id === "string" && id.length > 0;
}

/**
 * ID Format Schemas
 */
const idSchemas = {
  workflow: z
    .string()
    .regex(
      /^wflow_[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i,
      "Invalid workflow ID format",
    ),
  thread: z
    .string()
    .regex(
      /^thrd_[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i,
      "Invalid execution ID format",
    ),
  node: z.string().regex(/^node_[a-z0-9_]+$/, "Invalid node ID format"),
  edge: z.string().regex(/^edge_[a-z0-9_]+$/, "Invalid edge ID format"),
  checkpoint: z
    .string()
    .regex(
      /^ckpt_[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i,
      "Invalid checkpoint ID format",
    ),
  toolCall: z.string().regex(/^call_\d+_[a-z0-9]+$/, "Invalid tool call ID format"),
  event: z.string().regex(/^evt_\d+_[a-z0-9]+$/, "Invalid event ID format"),
} as const;

/**
 * Verify if the ID format is compliant with the specifications.
 * @param id: The ID to be verified
 * @param entityType: Entity type: workflow, thread, node, edge, checkpoint, toolCall, event
 * @returns: Whether the format is in compliance with the specifications
 */
export function validateId(id: ID, entityType: string): boolean {
  const schema = idSchemas[entityType as keyof typeof idSchemas];
  if (!schema) {
    return false;
  }
  const result = schema.safeParse(id);
  return result.success;
}

/**
 * Generate a node ID with a namespace prefix
 * @param prefix: The namespace prefix
 * @param originalId: The original node ID
 * @returns: The new node ID with the prefix
 */
export function generateNamespacedNodeId(prefix: string, originalId: ID): ID {
  // Remove the `node_` prefix from the original ID (if any).
  const baseId = originalId.replace(/^node_/, "");
  return `node_${prefix}_${baseId}`;
}

/**
 * Generate a new edge ID with a namespace prefix
 * @param prefix: The namespace prefix
 * @param originalId: The original edge ID
 * @returns: The new edge ID with the prefix
 */
export function generateNamespacedEdgeId(prefix: string, originalId: ID): ID {
  // Remove the `edge_` prefix from the original ID (if it exists).
  const baseId = originalId.replace(/^edge_/, "");
  return `edge_${prefix}_${baseId}`;
}

/**
 * Extract the original ID from the namespace ID
 * @param namespacedId: The ID with the namespace
 * @returns: The original ID
 */
export function extractOriginalId(namespacedId: ID): ID {
  // Remove the namespace prefix and retain the original ID.
  const parts = namespacedId.split("_");
  if (parts.length >= 3) {
    // Format: node_prefix_originalId or edge_prefix_originalId
    return parts.slice(2).join("_");
  }
  return namespacedId;
}

/**
 * Generate a sub-workflow namespace prefix
 * @param subworkflowId Sub-workflow ID
 * @param subgraphNodeId SUBGRAPH node ID
 * @returns Namespace prefix
 */
export function generateSubgraphNamespace(subworkflowId: ID, subgraphNodeId: ID): string {
  // Generate a unique prefix using the hash values of the sub-workflow ID and the SUBGRAPH node ID.
  const combined = `${subworkflowId}_${subgraphNodeId}`;
  let hash = 0;
  for (let i = 0; i < combined.length; i++) {
    const char = combined.charCodeAt(i);
    hash = (hash << 5) - hash + char;
    hash = hash & hash; // Convert to a 32-bit integer
  }
  return `sg_${Math.abs(hash).toString(16)}`;
}

/**
 * Check if the ID is a namespace ID
 * @param id The ID to be checked
 * @returns Whether it is a namespace ID
 */
export function isNamespacedId(id: ID): boolean {
  return /^node_sg_[a-f0-9]+_/.test(id) || /^edge_sg_[a-f0-9]+_/.test(id);
}
