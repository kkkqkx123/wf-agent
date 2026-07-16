/**
 * Workflow Tool Call Protocol Consistency Validator
 *
 * Validates that all nodes in a workflow definition that reference LLM profiles
 * have consistent tool call format configurations.
 *
 * Validation checks:
 * 1. Collect all nodes with profileId references (AGENT_LOOP, LLM nodes)
 * 2. Resolve each node's effective toolCallFormat (node-level -> profile-level -> default)
 * 3. Check that all nodes use the same protocol
 * 4. If node-level toolCallFormat is set, also check compatibility with the referenced profile
 * 5. Report all inconsistencies at load time
 */

import type { StaticNode, LLMProfile, ToolCallFormat } from "@wf-agent/types";
import { validateToolFormatCompatibility } from "../../services/llm/formatters/tool-format-selector.js";

/**
 * Protocol consistency result for a single node
 */
export interface NodeProtocolResult {
  /** Node ID */
  nodeId: string;
  /** Node name (for logging) */
  nodeName?: string;
  /** Effective tool call format after resolution */
  effectiveFormat: string;
  /** Whether this node's config is valid */
  valid: boolean;
  /** Error message if invalid */
  error?: string;
  /** Warning message if applicable */
  warning?: string;
}

/**
 * Overall workflow protocol consistency result
 */
export interface WorkflowProtocolConsistencyResult {
  /** Whether the workflow has consistent protocols */
  consistent: boolean;
  /** Per-node protocol details */
  nodeResults: NodeProtocolResult[];
  /** Error messages (critical issues like incompatible formats) */
  errors: string[];
  /** Warning messages (non-critical issues like missing profile config) */
  warnings: string[];
}

/**
 * Type guard for nodes that have a profileId reference.
 */
function hasProfileId(node: StaticNode): boolean {
  return (
    node.type === "LLM" ||
    node.type === "AGENT_LOOP"
  );
}

/**
 * Get the effective tool call format for a node.
 * Resolution order: node.config.toolCallFormat -> profile.toolCallFormat -> "native"
 */
function getEffectiveToolCallFormat(
  node: StaticNode,
  profileResolver: (profileId: string) => LLMProfile | undefined,
): { format: string; profile?: LLMProfile } {
  // Get profileId from node config
  let profileId: string | undefined;
  let nodeFormat: unknown;

  if (node.type === "LLM") {
    const config = node.config as { profileId?: string; toolCallFormat?: { format: string } };
    profileId = config.profileId;
    nodeFormat = config.toolCallFormat?.format;
  } else if (node.type === "AGENT_LOOP") {
    const config = node.config as {
      inlineConfig?: { profileId?: string };
      profileId?: string;
      toolCallFormat?: { format: string };
    };
    profileId = config.profileId || config.inlineConfig?.profileId;
    nodeFormat = (node.config as { toolCallFormat?: { format: string } }).toolCallFormat?.format;
  }

  // If no profileId, can't resolve profile
  if (!profileId) {
    return { format: (nodeFormat as string) || "native" };
  }

  const profile = profileResolver(profileId);
  const profileFormat = profile?.toolCallFormat?.format;

  return {
    format: (nodeFormat as string) || profileFormat || "native",
    profile,
  };
}

/**
 * Validate tool call protocol consistency across all nodes in a workflow definition.
 *
 * @param nodes All static nodes in the workflow definition
 * @param profileResolver Function to resolve a profile by ID
 * @returns Consistency result with errors and warnings
 */
export function validateWorkflowToolCallProtocolConsistency(
  nodes: StaticNode[],
  profileResolver: (profileId: string) => LLMProfile | undefined,
): WorkflowProtocolConsistencyResult {
  const errors: string[] = [];
  const warnings: string[] = [];
  const nodeResults: NodeProtocolResult[] = [];
  const protocols = new Set<string>();

  // Only check nodes that reference LLM profiles
  const relevantNodes = nodes.filter(hasProfileId);

  // If no relevant nodes, skip validation
  if (relevantNodes.length === 0) {
    return { consistent: true, nodeResults: [], errors, warnings };
  }

  for (const node of relevantNodes) {
    const { format, profile } = getEffectiveToolCallFormat(node, profileResolver);
    protocols.add(format);

    let nodeFormat: unknown;
    let profileId: string | undefined;

    if (node.type === "LLM") {
      const config = node.config as { profileId?: string; toolCallFormat?: { format: string } };
      profileId = config.profileId;
      nodeFormat = config.toolCallFormat?.format;
    } else if (node.type === "AGENT_LOOP") {
      const config = node.config as {
        inlineConfig?: { profileId?: string };
        profileId?: string;
      };
      profileId = config.profileId || config.inlineConfig?.profileId;
      nodeFormat = (node.config as { toolCallFormat?: { format: string } }).toolCallFormat?.format;
    }

    const result: NodeProtocolResult = {
      nodeId: node.id,
      nodeName: node.name,
      effectiveFormat: format,
      valid: true,
    };

    // Check node-profile compatibility if both have explicit config
    if (nodeFormat && profile?.toolCallFormat) {
      const compatibility = validateToolFormatCompatibility(
        profile.toolCallFormat.format,
        nodeFormat as ToolCallFormat,
      );
      if (!compatibility.compatible) {
        result.valid = false;
        result.error = `Node "${node.id}": tool call format "${nodeFormat}" is incompatible with profile "${profileId}" format "${profile.toolCallFormat.format}". ${compatibility.reason || ""}`;
        errors.push(result.error);
      } else if (compatibility.reason) {
        result.warning = compatibility.reason;
        warnings.push(compatibility.reason);
      }
    }

    nodeResults.push(result);
  }

  // Check for cross-node protocol inconsistency
  if (protocols.size > 1) {
    const protocolList = [...protocols].join(", ");
    const errorMsg = `Inconsistent tool call protocols across workflow nodes: ${protocolList}. All nodes that reference LLM profiles should use the same protocol.`;
    errors.push(errorMsg);
  }

  return {
    consistent: errors.length === 0,
    nodeResults,
    errors,
    warnings,
  };
}