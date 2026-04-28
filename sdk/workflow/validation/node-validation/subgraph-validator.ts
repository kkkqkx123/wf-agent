/**
 * Subgraph node validation function
 * Provides static validation logic for subgraph nodes, using Zod for validation.
 */

import { z } from "zod";
import type { Node } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Subgraph node configuration schema
 */
const subgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean().optional().default(false),
});

/**
 * Verify subgraph node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateSubgraphNode(node: Node): Result<Node, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "SUBGRAPH");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, subgraphNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
