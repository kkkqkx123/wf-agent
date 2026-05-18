/**
 * Triggered Subgraph Validator
 *
 * Validates triggered subgraph connectivity:
 * - Check if graph is a triggered subgraph (contains START_FROM_TRIGGER node)
 * - Verify internal connectivity from START_FROM_TRIGGER to all nodes
 * - Verify reachability from all nodes to CONTINUE_FROM_TRIGGER
 */

import type { ID, StaticNodeType } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";
import { getReachableNodes } from "../../builder/utils/workflow-traversal.js";

/**
 * Check if the graph is a triggered subgraph
 * @param graph Graph data
 * @returns Whether it is a triggered subgraph
 */
export function isTriggeredSubgraph(graph: WorkflowGraphData): boolean {
  for (const node of graph.nodes.values()) {
    if (node.type === ("START_FROM_TRIGGER" as StaticNodeType)) {
      return true;
    }
  }
  return false;
}

/**
 * Validate internal connectivity of triggered subgraph
 * Ensures that all nodes can be reached from START_FROM_TRIGGER and can reach CONTINUE_FROM_TRIGGER
 * @param graph Graph data
 * @returns List of validation errors
 */
export function validateTriggeredSubgraphConnectivity(
  graph: WorkflowGraphData,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Find START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER nodes
  let startNodeId: ID | null = null;
  let endNodeId: ID | null = null;

  for (const node of graph.nodes.values()) {
    if (node.type === ("START_FROM_TRIGGER" as StaticNodeType)) {
      startNodeId = node.id;
    } else if (node.type === ("CONTINUE_FROM_TRIGGER" as StaticNodeType)) {
      endNodeId = node.id;
    }
  }

  if (!startNodeId || !endNodeId) {
    // If required nodes are missing, errors have already been reported in validateTriggeredSubgraphNodes
    return errors;
  }

  // Check reachability from START_FROM_TRIGGER to all nodes
  const reachableFromStart = getReachableNodes(graph, startNodeId);
  const unreachableNodeIds: Set<ID> = new Set();
  
  for (const node of graph.nodes.values()) {
    if (node.type === ("START_FROM_TRIGGER" as StaticNodeType)) {
      continue; // Skip the starting node
    }
    if (!reachableFromStart.has(node.id)) {
      unreachableNodeIds.add(node.id);
      errors.push(
        new ConfigurationValidationError(
          `Node '${node.id}' is not reachable from START_FROM_TRIGGER. ` +
          `This node appears to be disconnected from the workflow. ` +
          `Note: This may also cause 'cannot reach CONTINUE_FROM_TRIGGER' errors. Fix connectivity first.`,
          {
            configType: "workflow",
            context: {
              code: "UNREACHABLE_FROM_START_FROM_TRIGGER",
              nodeId: node.id,
            },
          }
        )
      );
    }
  }

  // Check reachability from all nodes to CONTINUE_FROM_TRIGGER
  // Skip nodes that are already unreachable (they have their own error)
  for (const node of graph.nodes.values()) {
    if (node.type === ("CONTINUE_FROM_TRIGGER" as StaticNodeType)) {
      continue; // Skip the end node
    }
    
    // Skip nodes already reported as unreachable to avoid redundant errors
    if (unreachableNodeIds.has(node.id)) {
      continue;
    }
    
    const reachableFromNode = getReachableNodes(graph, node.id);
    if (!reachableFromNode.has(endNodeId)) {
      errors.push(
        new ConfigurationValidationError(
          `Node '${node.id}' cannot reach CONTINUE_FROM_TRIGGER. ` +
          `Check if this node has proper connections to the workflow end or subsequent nodes.`,
          {
            configType: "workflow",
            context: {
              code: "CANNOT_REACH_CONTINUE_FROM_TRIGGER",
              nodeId: node.id,
            },
          }
        )
      );
    }
  }

  return errors;
}
