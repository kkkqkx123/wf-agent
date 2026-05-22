/**
 * Static node Validation Function Module
 * Provides simple static validation functions for all types of nodes
 * Only use zod schema to verify config
 *
 * Subgraph nodes do not exist during the execution phase, therefore no processing is required.
 */

import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { err } from "@wf-agent/common-utils";

import { validateStartNode } from "./start-validator.js";
import { validateEndNode } from "./end-validator.js";
import { validateForkNode } from "./fork-validator.js";
import { validateJoinNode } from "./join-validator.js";
import { validateLoopStartNode } from "./loop-start-validator.js";
import { validateLoopEndNode } from "./loop-end-validator.js";
import { validateScriptNode } from "./script-validator.js";
import { validateContextProcessorNode } from "./context-processor-validator.js";
import { validateRouteNode } from "./route-validator.js";
import { validateVariableNode } from "./variable-validator.js";
import { validateLLMNode } from "./llm-validator.js";
import { validateUserInteractionNode } from "./user-interaction-validator.js";
import { validateSubgraphNode } from "./subgraph-validator.js";
import { validateEmbedGraphNode } from "./embed-graph-validator.js";
import { validateStartFromTriggerNode } from "./start-from-trigger-validator.js";
import { validateContinueFromTriggerNode } from "./continue-from-trigger-validator.js";
import { validateSyncNode } from "./sync-validator.js";

export { validateForkNode } from "./fork-validator.js";
export { validateJoinNode } from "./join-validator.js";
export { validateLoopStartNode } from "./loop-start-validator.js";
export { validateLoopEndNode } from "./loop-end-validator.js";
export { validateStartNode } from "./start-validator.js";
export { validateEndNode } from "./end-validator.js";
export { validateScriptNode } from "./script-validator.js";
export { validateContextProcessorNode } from "./context-processor-validator.js";
export { validateRouteNode } from "./route-validator.js";
export { validateVariableNode } from "./variable-validator.js";
export { validateLLMNode } from "./llm-validator.js";
export { validateUserInteractionNode } from "./user-interaction-validator.js";
export { validateSubgraphNode } from "./subgraph-validator.js";
export { validateEmbedGraphNode } from "./embed-graph-validator.js";
export { validateStartFromTriggerNode } from "./start-from-trigger-validator.js";
export { validateContinueFromTriggerNode } from "./continue-from-trigger-validator.js";
export { validateSyncNode } from "./sync-validator.js";

/**
 * Verify node configuration based on the node type
 * @param node Static node definition
 * @returns Verification result
 */
export function validateNodeByType(node: StaticNode): Result<StaticNode, ConfigurationValidationError[]> {
  switch (node.type) {
    case "START":
      return validateStartNode(node);
    case "END":
      return validateEndNode(node);
    case "FORK":
      return validateForkNode(node);
    case "JOIN":
      return validateJoinNode(node);
    case "LOOP_START":
      return validateLoopStartNode(node);
    case "LOOP_END":
      return validateLoopEndNode(node);
    case "SCRIPT":
      return validateScriptNode(node);
    case "CONTEXT_PROCESSOR":
      return validateContextProcessorNode(node);
    case "ROUTE":
      return validateRouteNode(node);
    case "VARIABLE":
      return validateVariableNode(node);
    case "LLM":
      return validateLLMNode(node);
    case "USER_INTERACTION":
      return validateUserInteractionNode(node);
    case "SUBGRAPH":
      return validateSubgraphNode(node);
    case "EMBED_GRAPH":
      return validateEmbedGraphNode(node);
    case "START_FROM_TRIGGER":
      return validateStartFromTriggerNode(node);
    case "CONTINUE_FROM_TRIGGER":
      return validateContinueFromTriggerNode(node);
    case "SYNC":
      return validateSyncNode(node);
    default:
      return err([
        new ConfigurationValidationError(`Unknown node type: ${node.type}`, {
          configType: "node",
          configPath: `node.${node.id}`,
        }),
      ]);
  }
}
