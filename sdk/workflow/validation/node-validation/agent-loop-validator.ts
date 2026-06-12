/**
 * AgentLoop Node Validation Function
 * Provides static validation logic for AGENT_LOOP nodes, using Zod for validation.
 */

import type { StaticNode } from "@wf-agent/types";
import { AgentLoopNodeConfigSchema, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok } from "@wf-agent/common-utils";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

/**
 * Verify AGENT_LOOP node configuration
 * @param node Node definition
 * @returns Verification result
 */
export function validateAgentLoopNode(
  node: StaticNode,
): Result<StaticNode, ConfigurationValidationError[]> {
  const typeResult = validateNodeType(node, "AGENT_LOOP");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(node.config, AgentLoopNodeConfigSchema, node.id);
  if (configResult.isErr()) {
    return configResult;
  }

  return ok(node);
}
