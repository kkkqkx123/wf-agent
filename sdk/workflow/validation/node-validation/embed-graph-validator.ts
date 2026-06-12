/**
 * EmbedGraph Node Validation Function
 * Provides static validation logic for EmbedGraph nodes, using Zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { EmbedGraphNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify EmbedGraph node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateEmbedGraphNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "EMBED_GRAPH");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, EmbedGraphNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
