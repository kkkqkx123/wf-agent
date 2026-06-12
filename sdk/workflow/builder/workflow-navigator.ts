/**
 * Workflow Navigator
 * Used to navigate through the nodes of a workflow graph during execution, determining the next node to be processed.
 *
 * Design Principles:
 * - Stateless Execution: Does not cache the execution flow state (such as currentNodeId); all methods pass the required data through parameters.
 * - Stateful Definition: Holds an immutable workflow definition (WorkflowGraphData), which is managed by the parent container (WorkflowExecutionContext) throughout its lifecycle.
 * - Pure Functions: All methods are pure functions and do not have any side effects.
 *
 * Creation Method: Instances of WorkflowNavigator are created and cached uniformly by WorkflowExecutionContext, initialized using the graph object within the WorkflowExecution.
 */

import type { ID, StaticNodeType, Condition, Edge, WorkflowGraphStructure } from "@wf-agent/types";
import { conditionEvaluator } from "../evaluation/index.js";
import { getReachableNodes } from "./utils/workflow-traversal.js";

/**
 * Navigation results
 */
export interface NavigationResult {
  /** Next node ID */
  nextNodeId?: ID;
  /** Has the end node been reached? */
  isEnd: boolean;
  /** Are there multiple possible next nodes (requiring routing decisions)? */
  hasMultiplePaths: boolean;
  /** All possible next node IDs */
  possibleNextNodeIds: ID[];
}

/**
 * Route decision result
 */
export interface RoutingDecision {
  /** Selected node ID */
  selectedNodeId: ID;
  /** Used edge ID */
  edgeId: ID;
  /** Reasons for the decision */
  reason: string;
}

/**
 * Path enumeration options for safety limits
 */
export interface PathEnumerationOptions {
  /** Maximum number of paths to return (default: 100) */
  maxPaths?: number;

  /** Maximum depth per path (default: 50) */
  maxDepth?: number;

  /** Stop after finding first path to END node (default: false) */
  stopAtFirstEnd?: boolean;

  /** Timeout in milliseconds (default: 5000ms) */
  timeoutMs?: number;

  /** Callback for progress reporting - return false to stop */
  onPathFound?: (path: ID[], totalCount: number) => boolean;
}

/**
 * Result of path enumeration with metadata
 */
export interface PathEnumerationResult {
  /** Paths found (up to maxPaths limit) */
  paths: ID[][];

  /** Whether enumeration was truncated */
  truncated: boolean;

  /** Total number of paths found (may exceed paths.length) */
  totalCount: number;

  /** Reason for truncation, if any */
  reason?: "MAX_PATHS" | "MAX_DEPTH" | "TIMEOUT" | "USER_CANCELLED" | "COMPLETE";
}

/**
 * Workflow Navigator class
 */
export class WorkflowNavigator {
  private graph: WorkflowGraphStructure;

  constructor(graph: WorkflowGraphStructure) {
    this.graph = graph;
  }

  /**
   * Get the next node (simple navigation, without considering conditions)
   * @param currentNodeId: The ID of the current node; if undefined, return the START node
   * @returns: The navigation result
   */
  getNextNode(currentNodeId?: ID): NavigationResult {
    if (!currentNodeId) {
      // If the current node does not exist, return the START node.
      if (this.graph.startNodeId) {
        return {
          nextNodeId: this.graph.startNodeId,
          isEnd: false,
          hasMultiplePaths: false,
          possibleNextNodeIds: [this.graph.startNodeId],
        };
      }
      return {
        isEnd: true,
        hasMultiplePaths: false,
        possibleNextNodeIds: [],
      };
    }

    const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);

    if (outgoingEdges.length === 0) {
      // No external connections were found, indicating that the end node has been reached.
      return {
        isEnd: true,
        hasMultiplePaths: false,
        possibleNextNodeIds: [],
      };
    }

    const possibleNextNodeIds = outgoingEdges.map(
      (edge: { targetNodeId: ID }) => edge.targetNodeId,
    );

    if (outgoingEdges.length === 1) {
      // There is only one exit; return it directly.
      const edge = outgoingEdges[0]!;
      return {
        nextNodeId: edge.targetNodeId,
        isEnd: this.graph.endNodeIds.has(edge.targetNodeId),
        hasMultiplePaths: false,
        possibleNextNodeIds,
      };
    }

    // Multiple external links require routing decisions.
    return {
      isEnd: false,
      hasMultiplePaths: true,
      possibleNextNodeIds,
    };
  }

  /**
   * Evaluate conditions to select the next node
   * @param currentNodeId: The ID of the current node
   * @param conditionEvaluator: The function used to evaluate the conditions
   * @returns: The routing decision result; returns null if no edge meets the conditions
   */
  routeNextNode(
    currentNodeId: ID,
    conditionEvaluator: (condition: Condition) => boolean,
  ): RoutingDecision | null {
    const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);

    if (outgoingEdges.length === 0) {
      return null;
    }

    // Sort by weight (with higher weights taking precedence)
    const sortedEdges = [...outgoingEdges].sort((a, b) => {
      return (b.weight || 0) - (a.weight || 0);
    });

    // First pass: Try to find a CONDITIONAL edge that matches (respecting weight priority)
    for (const edge of sortedEdges) {
      if (edge.type === "CONDITIONAL") {
        const condition = edge.originalEdge?.condition;
        if (condition && conditionEvaluator(condition)) {
          return {
            selectedNodeId: edge.targetNodeId,
            edgeId: edge.id,
            reason: "CONDITION_MATCHED",
          };
        }
      }
    }

    // Second pass: If no CONDITIONAL edge matched, use DEFAULT edge (if exists)
    for (const edge of sortedEdges) {
      if (edge.type === "DEFAULT") {
        return {
          selectedNodeId: edge.targetNodeId,
          edgeId: edge.id,
          reason: "DEFAULT_EDGE",
        };
      }
    }

    // No edge meets the conditions.
    return null;
  }

  /**
   * Select the next node based on the WorkflowExecution context
   * @param currentNodeId: The ID of the current node
   * @param workflowExecution: A WorkflowExecution instance that provides context information such as variables, inputs, and outputs
   * @param currentNodeType: The type of the current node, used for handling special logic in ROUTE nodes
   * @param lastNodeResult: The execution result of the previous node, used for decision-making in ROUTE nodes
   * @returns: The ID of the next node; returns null if no available route is found
   */
  async selectNextNodeWithContext(
    currentNodeId: ID,
    workflowExecution: {
      variableScopes: { workflowExecution: Record<string, unknown> };
      input: unknown;
      output: unknown;
    },
    currentNodeType: StaticNodeType,
    lastNodeResult?: { nodeId?: string; selectedNode?: string },
  ): Promise<string | null> {
    // Handling special logic for the ROUTE node
    if (currentNodeType === ("ROUTE" as StaticNodeType)) {
      // The ROUTE node uses its own routing decisions to get selectedNode from the output returned by the processor
      // Route processor returns: { status: 'COMPLETED', selectedNode: nodeId }
      if (
        lastNodeResult &&
        lastNodeResult.nodeId === currentNodeId &&
        typeof lastNodeResult.selectedNode === "string"
      ) {
        return lastNodeResult.selectedNode;
      }
      return null;
    }

    const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);

    if (outgoingEdges.length === 0) {
      return null;
    }

    // Filter edges that meet the criteria.
    const satisfiedEdges = await this.filterEdgesWithContext(outgoingEdges, workflowExecution);

    // If no edge meets the conditions, select the default edge.
    if (satisfiedEdges.length === 0) {
      const defaultEdge = outgoingEdges.find(e => e.type === "DEFAULT");
      return defaultEdge ? defaultEdge.targetNodeId : null;
    }

    // Sort edges by weight.
    const sortedEdges = this.sortEdges(satisfiedEdges);

    // Select the first edge.
    const nextEdge = sortedEdges[0];
    return nextEdge ? nextEdge.targetNodeId : null;
  }

  /**
   * Filter edges that meet the conditions based on the WorkflowExecution context
   * @param edges: Array of edges
   * @param workflowExecution: WorkflowExecution instance
   * @returns: Array of edges that meet the conditions
   */
  private async filterEdgesWithContext(
    edges: Edge[],
    workflowExecution: {
      variableScopes: { workflowExecution: Record<string, unknown> };
      input: unknown;
      output: unknown;
    },
  ): Promise<Edge[]> {
    const results = await Promise.all(
      edges.map(async edge => ({
        edge,
        satisfied: await this.evaluateEdgeConditionWithContext(edge, workflowExecution),
      })),
    );
    return results.filter(r => r.satisfied).map(r => r.edge);
  }

  /**
   * Evaluate the condition of an edge based on the WorkflowExecution context
   * @param edge: The edge in question
   * @param workflowExecution: The WorkflowExecution instance
   * @returns: Whether the condition is met or not
   */
  private async evaluateEdgeConditionWithContext(
    edge: Edge,
    workflowExecution: {
      variableScopes: { workflowExecution: Record<string, unknown> };
      input: unknown;
      output: unknown;
    },
  ): Promise<boolean> {
    // The default edges always satisfy the conditions.
    if (edge.type === "DEFAULT") {
      return true;
    }

    // Conditional edges must have conditions.
    if (!edge.condition) {
      return false;
    }

    // Build an evaluation context
    const context = {
      variables: workflowExecution.variableScopes.workflowExecution,
      input: workflowExecution.input as Record<string, unknown>,
      output: workflowExecution.output as Record<string, unknown>,
    };

    return conditionEvaluator.evaluate(edge.condition, context);
  }

  /**
   * Sort edges
   * @param edges: An array of edges
   * @returns: A sorted array of edges
   */
  private sortEdges(edges: Edge[]): Edge[] {
    return [...edges].sort((a, b) => {
      // Sort in descending order of weight (higher weights take precedence).
      const weightA = a.weight || 0;
      const weightB = b.weight || 0;

      if (weightA !== weightB) {
        return weightB - weightA;
      }

      // If the weights are the same, sort in ascending order by id (to ensure certainty).
      return a.id.localeCompare(b.id);
    });
  }

  /**
   * Get the path from the specified source node to the target node
   * @param fromNodeId: Source node ID
   * @param targetNodeId: Target node ID
   * @returns: Array of paths; returns null if the path is unreachable
   */
  getPathTo(fromNodeId: ID, targetNodeId: ID): ID[] | null {
    if (fromNodeId === targetNodeId) {
      return [fromNodeId];
    }

    // Use BFS to find the shortest path.
    const visited = new Set<ID>();
    const queue: { nodeId: ID; path: ID[] }[] = [{ nodeId: fromNodeId, path: [fromNodeId] }];
    visited.add(fromNodeId);

    while (queue.length > 0) {
      const { nodeId, path } = queue.shift()!;

      const neighbors = this.graph.getOutgoingNeighbors(nodeId);
      for (const neighborId of neighbors) {
        if (neighborId === targetNodeId) {
          return [...path, neighborId];
        }

        if (!visited.has(neighborId)) {
          visited.add(neighborId);
          queue.push({
            nodeId: neighborId,
            path: [...path, neighborId],
          });
        }
      }
    }

    return null;
  }

  /**
   * Check if it is possible to reach the target node from the starting node
   * @param fromNodeId ID of the starting node
   * @param targetNodeId ID of the target node
   * @returns Whether reachability is established
   */
  canReach(fromNodeId: ID, targetNodeId: ID): boolean {
    const reachableNodes = getReachableNodes(this.graph, fromNodeId);
    return reachableNodes.has(targetNodeId);
  }

  /**
   * Get all possible execution paths (from the specified node to all END nodes)
   * @param fromNodeId: The ID of the starting node
   * @param options: Path enumeration options for safety limits
   * @returns: Path enumeration result with metadata
   */
  getAllExecutionPaths(
    fromNodeId: ID,
    options: PathEnumerationOptions = {},
  ): PathEnumerationResult {
    const {
      maxPaths = 100,
      maxDepth = 50,
      stopAtFirstEnd = false,
      timeoutMs = 5000,
      onPathFound,
    } = options;

    const paths: ID[][] = [];
    const visited = new Set<ID>();
    let totalCount = 0;
    let truncated = false;
    let reason: PathEnumerationResult["reason"] = "COMPLETE";

    const startTime = Date.now();

    const dfs = (nodeId: ID, path: ID[]): boolean => {
      // Check timeout
      if (Date.now() - startTime > timeoutMs) {
        truncated = true;
        reason = "TIMEOUT";
        return false;
      }

      // Check depth limit
      if (path.length >= maxDepth) {
        truncated = true;
        reason = "MAX_DEPTH";
        return false;
      }

      const newPath = [...path, nodeId];

      // Check if END node
      if (this.graph.endNodeIds.has(nodeId)) {
        totalCount++;

        // Add to results if under limit
        if (paths.length < maxPaths) {
          paths.push(newPath);
        } else {
          truncated = true;
          reason = "MAX_PATHS";
        }

        // Notify callback
        if (onPathFound && !onPathFound(newPath, totalCount)) {
          truncated = true;
          reason = "USER_CANCELLED";
          return false;
        }

        // Stop at first end if requested
        if (stopAtFirstEnd) {
          return false;
        }

        return true; // Continue searching for more paths
      }

      // Avoid cycles
      if (visited.has(nodeId)) {
        return true;
      }

      visited.add(nodeId);

      // Explore neighbors
      const neighbors = this.graph.getOutgoingNeighbors(nodeId);
      for (const neighborId of neighbors) {
        if (!dfs(neighborId, newPath)) {
          visited.delete(nodeId);
          return false; // Early termination
        }
      }

      visited.delete(nodeId);
      return true;
    };

    dfs(fromNodeId, []);

    return {
      paths,
      truncated,
      totalCount,
      reason: truncated ? reason : "COMPLETE",
    };
  }

  /**
   * Get all predecessor nodes of the specified node
   * @param nodeId Node ID
   * @returns Array of predecessor node IDs
   */
  getPredecessors(nodeId: ID): ID[] {
    return Array.from(this.graph.getIncomingNeighbors(nodeId));
  }

  /**
   * Get all the successor nodes of the specified node
   * @param nodeId Node ID
   * @returns Array of successor node IDs
   */
  getSuccessors(nodeId: ID): ID[] {
    return Array.from(this.graph.getOutgoingNeighbors(nodeId));
  }

  /**
   * Check if the specified node is a FORK node.
   * @param nodeId Node ID
   * @returns Whether it is a FORK node
   */
  isForkNode(nodeId: ID): boolean {
    const node = this.graph.getNode(nodeId);
    return node?.type === ("FORK" as StaticNodeType);
  }

  /**
   * Check if the specified node is a JOIN node.
   * @param nodeId Node ID
   * @returns Whether it is a JOIN node
   */
  isJoinNode(nodeId: ID): boolean {
    const node = this.graph.getNode(nodeId);
    return node?.type === ("JOIN" as StaticNodeType);
  }

  /**
   * Check if the specified node is a ROUTE node.
   * @param nodeId Node ID
   * @returns Whether it is a ROUTE node
   */
  isRouteNode(nodeId: ID): boolean {
    const node = this.graph.getNode(nodeId);
    return node?.type === ("ROUTE" as StaticNodeType);
  }

  /**
   * Check if the specified node is an END node.
   * @param nodeId Node ID
   * @returns Whether it is an END node
   */
  isEndNode(nodeId: ID): boolean {
    return this.graph.endNodeIds.has(nodeId);
  }

  /**
   * Check if the specified node is a START node.
   * @param nodeId Node ID
   * @returns Whether it is a START node
   */
  isStartNode(nodeId: ID): boolean {
    return this.graph.startNodeId === nodeId;
  }

  /**
   * Get the reference of the graph.
   */
  getGraph(): WorkflowGraphStructure {
    return this.graph;
  }
}
