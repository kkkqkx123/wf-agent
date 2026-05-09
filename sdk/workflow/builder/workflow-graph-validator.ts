/**
 * Workflow Graph Structure Validator
 *
 * Responsibilities:
 * - Verify the topological structure and logical correctness of the workflow graph.
 * - Check the in-degree and out-degree constraints of the START/END nodes.
 * - Detect circular dependencies (cycle detection).
 * - Analyze node reachability (paths from START to END).
 * - Verify the pairing relationships and business logic of FORK/JOIN nodes.
 */

import type { ID, NodeType, GraphValidationOptions } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { WorkflowGraphData } from "../entities/workflow-graph-data.js";
import { SUBGRAPH_METADATA_KEYS } from "@wf-agent/types";
import { detectCycles } from "./utils/workflow-cycle-detector.js";
import { analyzeReachability } from "./utils/workflow-reachability-analyzer.js";

/**
 * Workflow Graph Validator Class
 */
export class WorkflowGraphValidator {
  /**
   * Verify the workflow graph structure.
   */
  static validate(
    graph: WorkflowGraphData,
    options: GraphValidationOptions = {},
  ): Result<WorkflowGraphData, ConfigurationValidationError[]> {
    const errorList: ConfigurationValidationError[] = [];

    const opts = {
      checkCycles: true,
      checkReachability: true,
      checkForkJoin: true,
      checkStartEnd: true,
      checkIsolatedNodes: true,
      ...options,
    };

    // Check if it is a triggered subworkflow.
    const isTriggeredSubgraph = this.isTriggeredSubgraph(graph);

    // Check the START/END nodes.
    if (opts.checkStartEnd) {
      const startEndErrors = isTriggeredSubgraph
        ? this.validateTriggeredSubgraphNodes(graph)
        : this.validateStartEndNodes(graph);
      errorList.push(...startEndErrors);
    }

    // Check isolated nodes.
    if (opts.checkIsolatedNodes) {
      const isolatedErrors = this.validateIsolatedNodes(graph);
      errorList.push(...isolatedErrors);
    }

    // Detect loop
    if (opts.checkCycles) {
      const cycleResult = detectCycles(graph);
      if (cycleResult.hasCycle) {
        errorList.push(
          new ConfigurationValidationError("Circular dependencies exist in the workflow", {
            configType: "workflow",
            context: {
              code: "CYCLE_DETECTED",
              cycleNodes: cycleResult.cycleNodes,
              cycleEdges: cycleResult.cycleEdges,
            },
          }),
        );
      }
    }

    // Reachability Analysis
    if (opts.checkReachability) {
      if (isTriggeredSubgraph) {
        // Triggered subworkflow only verifies internal connectivity.
        const connectivityErrors = this.validateTriggeredSubgraphConnectivity(graph);
        errorList.push(...connectivityErrors);
      } else {
        // Verify the reachability from START to END in normal workflows.
        const reachabilityResult = analyzeReachability(graph);

        // Unreachable nodes
        for (const nodeId of reachabilityResult.unreachableNodes) {
          errorList.push(
            new ConfigurationValidationError(`Node(${nodeId}) is unreachable from START node`, {
              configType: "workflow",
              context: {
                code: "UNREACHABLE_NODE",
                nodeId,
              },
            }),
          );
        }

        // Dead nodes
        for (const nodeId of reachabilityResult.deadEndNodes) {
          errorList.push(
            new ConfigurationValidationError(`Node(${nodeId}) cannot reach END node`, {
              configType: "workflow",
              context: {
                code: "DEAD_END_NODE",
                nodeId,
              },
            }),
          );
        }
      }
    }

    // FORK/JOIN pairing verification
    if (opts.checkForkJoin) {
      const forkJoinErrors = this.validateForkJoinPairs(graph);
      errorList.push(...forkJoinErrors);
    }

    if (errorList.length === 0) {
      return ok(graph);
    }
    return err(errorList);
  }

  /**
   * Verify the in-degree and out-degree constraints of the START and END nodes.
   */
  private static validateStartEndNodes(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Check the START node.
    if (!graph.startNodeId) {
      errors.push(
        new ConfigurationValidationError("The workflow must contain a START node", {
          configType: "workflow",
          context: {
            code: "MISSING_START_NODE",
          },
        }),
      );
    } else {
      // Check the in-degree of the START node.
      const incomingEdges = graph.getIncomingEdges(graph.startNodeId);
      if (incomingEdges.length > 0) {
        errors.push(
          new ConfigurationValidationError("START nodes cannot have incoming edges", {
            configType: "workflow",
            context: {
              code: "START_NODE_HAS_INCOMING_EDGES",
              nodeId: graph.startNodeId,
            },
          }),
        );
      }
    }

    // Check the END node.
    if (graph.endNodeIds.size === 0) {
      errors.push(
        new ConfigurationValidationError("The workflow must contain at least one END node", {
          configType: "workflow",
          context: {
            code: "MISSING_END_NODE",
          },
        }),
      );
    } else {
      // Check the outdegree of the END node.
      for (const endNodeId of graph.endNodeIds) {
        const outgoingEdges = graph.getOutgoingEdges(endNodeId);
        if (outgoingEdges.length > 0) {
          errors.push(
            new ConfigurationValidationError(`END node(${endNodeId}) cannot have outgoing edges`, {
              configType: "workflow",
              context: {
                code: "END_NODE_HAS_OUTGOING_EDGES",
                nodeId: endNodeId,
              },
            }),
          );
        }
      }
    }

    // Check whether the START node is unique
    let startNodeCount = 0;
    for (const node of graph.nodes.values()) {
      if (node.type === ("START" as NodeType)) {
        // Check if it is a sub-workflow boundary node.
        const isSubgraphBoundary =
          node.internalMetadata?.[SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE] === "entry";
        if (!isSubgraphBoundary) {
          startNodeCount++;
        }
      }
    }
    if (startNodeCount > 1) {
      errors.push(
        new ConfigurationValidationError("A workflow can only contain one START node", {
          configType: "workflow",
          context: {
            code: "MULTIPLE_START_NODES",
          },
        }),
      );
    }

    return errors;
  }

  /**
   * Verify isolated nodes
   */
  private static validateIsolatedNodes(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    for (const node of graph.nodes.values()) {
      const incomingEdges = graph.getIncomingEdges(node.id);
      const outgoingEdges = graph.getOutgoingEdges(node.id);

      // The START, END, START_FROM_TRIGGER, and CONTINUE_FROM_TRIGGER nodes are not considered isolated nodes.
      if (
        node.type === ("START" as NodeType) ||
        node.type === ("END" as NodeType) ||
        node.type === ("START_FROM_TRIGGER" as NodeType) ||
        node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)
      ) {
        continue;
      }

      if (incomingEdges.length === 0 && outgoingEdges.length === 0) {
        errors.push(
          new ConfigurationValidationError(`Node(${node.id}) is isolated, has no incoming or outgoing edges`, {
            configType: "workflow",
            context: {
              code: "ISOLATED_NODE",
              nodeId: node.id,
            },
          }),
        );
      }
    }

    return errors;
  }

  /**
   * FORK/JOIN Pair Verification and Business Logic Validation
   */
  private static validateForkJoinPairs(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];
    const forkNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[] }>();
    const joinNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[]; mainPathId?: ID }>();
    const allForkPathIds = new Set<ID>();

    // Collect all FORK and JOIN nodes.
    for (const node of graph.nodes.values()) {
      if (node.type === ("FORK" as NodeType)) {
        const config = node.originalNode?.config as
          | { forkPaths?: Array<{ pathId: ID; childNodeId: ID }> }
          | undefined;
        const forkPaths = config?.forkPaths;

        if (!forkPaths || !Array.isArray(forkPaths) || forkPaths.length === 0) {
          errors.push(
            new ConfigurationValidationError(`FORK node(${node.id}) forkPaths must be non-empty array`, {
              configType: "workflow",
              context: {
                code: "INVALID_FORK_PATHS",
                nodeId: node.id,
              },
            }),
          );
          continue;
        }

        const forkPathIds: ID[] = [];
        for (const forkPath of forkPaths) {
          if (!forkPath.pathId || !forkPath.childNodeId) {
            errors.push(
              new ConfigurationValidationError(
                `FORK node(${node.id}) forkPaths each element must contain pathId and childNodeId`,
                {
                  configType: "workflow",
                  context: {
                    code: "INVALID_FORK_PATH_ITEM",
                    nodeId: node.id,
                  },
                },
              ),
            );
            continue;
          }
          forkPathIds.push(forkPath.pathId);
        }

        // Check if pathId is unique within the workflow definition.
        for (const forkPathId of forkPathIds) {
          if (allForkPathIds.has(forkPathId)) {
            errors.push(
              new ConfigurationValidationError(
                `FORK node(${node.id}) pathId(${forkPathId}) is not unique within workflow definition`,
                {
                  configType: "workflow",
                  context: {
                    code: "DUPLICATE_FORK_PATH_ID",
                    nodeId: node.id,
                    forkPathId,
                  },
                },
              ),
            );
          } else {
            allForkPathIds.add(forkPathId);
          }
        }

        if (forkPathIds.length === 0) {
          continue;
        }
        const pairId = forkPathIds[0]!;
        forkNodes.set(pairId, { nodeId: node.id, forkPathIds });
      } else if (node.type === ("JOIN" as NodeType)) {
        const config = node.originalNode?.config as
          | { forkPathIds?: ID[]; mainPathId?: ID }
          | undefined;
        const forkPathIds = config?.forkPathIds;
        const mainPathId = config?.mainPathId;

        if (!forkPathIds || !Array.isArray(forkPathIds) || forkPathIds.length === 0) {
          errors.push(
            new ConfigurationValidationError(`JOIN node(${node.id}) forkPathIds must be non-empty array`, {
              configType: "workflow",
              context: {
                code: "INVALID_JOIN_PATH_IDS",
                nodeId: node.id,
              },
            }),
          );
          continue;
        }

        // Check if mainPathId is in forkPathIds
        if (mainPathId && !forkPathIds.includes(mainPathId)) {
          errors.push(
            new ConfigurationValidationError(
              `JOIN node(${node.id}) mainPathId(${mainPathId}) must be one of forkPathIds`,
              {
                configType: "workflow",
                context: {
                  code: "INVALID_MAIN_PATH_ID",
                  nodeId: node.id,
                  mainPathId,
                  forkPathIds,
                },
              },
            ),
          );
        }

        const pairId = forkPathIds[0]!;
        joinNodes.set(pairId, { nodeId: node.id, forkPathIds, mainPathId });
      }
    }

    // Check pairing
    for (const [pairId, forkNode] of forkNodes) {
      if (!joinNodes.has(pairId)) {
        errors.push(
          new ConfigurationValidationError(
            `FORK node(${forkNode.nodeId}) has no matching JOIN node (pairId: ${pairId})`,
            {
              configType: "workflow",
              context: {
                code: "UNPAIRED_FORK",
                nodeId: forkNode.nodeId,
                pairId,
              },
            },
          ),
        );
      } else {
        const joinNode = joinNodes.get(pairId)!;
        // Check if forkPathIds match
        const forkPathIdsSet = new Set(forkNode.forkPathIds);
        const joinPathIdsSet = new Set(joinNode.forkPathIds);

        if (forkPathIdsSet.size !== joinPathIdsSet.size) {
          errors.push(
            new ConfigurationValidationError(
              `FORK node(${forkNode.nodeId}) and JOIN node(${joinNode.nodeId}) forkPathIds count mismatch`,
              {
                configType: "workflow",
                context: {
                  code: "FORK_JOIN_PATH_COUNT_MISMATCH",
                  forkNodeId: forkNode.nodeId,
                  joinNodeId: joinNode.nodeId,
                },
              },
            ),
          );
        }
      }
    }

    for (const [pairId, joinNode] of joinNodes) {
      if (!forkNodes.has(pairId)) {
        errors.push(
          new ConfigurationValidationError(
            `JOIN node(${joinNode.nodeId}) has no matching FORK node (pairId: ${pairId})`,
            {
              configType: "workflow",
              context: {
                code: "UNPAIRED_JOIN",
                nodeId: joinNode.nodeId,
                pairId,
              },
            },
          ),
        );
      }
    }

    return errors;
  }

  /**
   * Check if the graph is a triggered subworkflow.
   */
  private static isTriggeredSubgraph(graph: WorkflowGraphData): boolean {
    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        return true;
      }
    }
    return false;
  }

  /**
   * Verify the START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER nodes for triggered subworkflows.
   */
  private static validateTriggeredSubgraphNodes(
    graph: WorkflowGraphData,
  ): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Check START_FROM_TRIGGER node.
    let startFromTriggerCount = 0;
    let startFromTriggerNodeId: ID | undefined;

    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        startFromTriggerCount++;
        startFromTriggerNodeId = node.id;
      }
    }

    if (startFromTriggerCount === 0) {
      errors.push(
        new ConfigurationValidationError("The workflow must contain a START_FROM_TRIGGER node", {
          configType: "workflow",
          context: {
            code: "MISSING_START_FROM_TRIGGER_NODE",
          },
        }),
      );
    } else if (startFromTriggerCount > 1) {
      errors.push(
        new ConfigurationValidationError(
          "A triggered subworkflow can only contain one START_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MULTIPLE_START_FROM_TRIGGER_NODES",
            },
          },
        ),
      );
    } else if (startFromTriggerNodeId) {
      // Check the in-degree of START_FROM_TRIGGER node.
      const incomingEdges = graph.getIncomingEdges(startFromTriggerNodeId);
      if (incomingEdges.length > 0) {
        errors.push(
          new ConfigurationValidationError(
            "START_FROM_TRIGGER node cannot have incoming edges",
            {
              configType: "workflow",
              context: {
                code: "START_FROM_TRIGGER_HAS_INCOMING_EDGES",
                nodeId: startFromTriggerNodeId,
              },
            },
          ),
        );
      }
    }

    // Check CONTINUE_FROM_TRIGGER nodes.
    let continueFromTriggerCount = 0;
    for (const node of graph.nodes.values()) {
      if (node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)) {
        continueFromTriggerCount++;
      }
    }

    if (continueFromTriggerCount === 0) {
      errors.push(
        new ConfigurationValidationError(
          "The workflow must contain at least one CONTINUE_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MISSING_CONTINUE_FROM_TRIGGER_NODE",
            },
          },
        ),
      );
    }

    // Triggered subworkflows should not contain regular START or END nodes.
    for (const node of graph.nodes.values()) {
      if (node.type === ("START" as NodeType)) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subworkflow cannot contain a regular START node",
            {
              configType: "workflow",
              context: {
                code: "TRIGGERED_SUBGRAPH_CONTAINS_START_NODE",
                nodeId: node.id,
              },
            },
          ),
        );
      }
      if (node.type === ("END" as NodeType)) {
        errors.push(
          new ConfigurationValidationError(
            "Triggered subworkflow cannot contain a regular END node",
            {
              configType: "workflow",
              context: {
                code: "TRIGGERED_SUBGRAPH_CONTAINS_END_NODE",
                nodeId: node.id,
              },
            },
          ),
        );
      }
    }

    return errors;
  }

  /**
   * Verify connectivity for triggered subworkflows.
   * Checks that all nodes are reachable from START_FROM_TRIGGER and can reach CONTINUE_FROM_TRIGGER.
   */
  private static validateTriggeredSubgraphConnectivity(
    graph: WorkflowGraphData,
  ): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Find START_FROM_TRIGGER node.
    let startFromTriggerNodeId: ID | undefined;
    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        startFromTriggerNodeId = node.id;
        break;
      }
    }

    if (!startFromTriggerNodeId) {
      return errors; // Already reported in validateTriggeredSubgraphNodes
    }

    // Find all CONTINUE_FROM_TRIGGER nodes.
    const continueFromTriggerNodeIds: ID[] = [];
    for (const node of graph.nodes.values()) {
      if (node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)) {
        continueFromTriggerNodeIds.push(node.id);
      }
    }

    // Check reachability from START_FROM_TRIGGER.
    const visited = new Set<ID>();
    const queue: ID[] = [startFromTriggerNodeId];
    visited.add(startFromTriggerNodeId);

    while (queue.length > 0) {
      const currentId = queue.shift()!;
      const neighbors = graph.getOutgoingNeighbors(currentId);
      for (const neighborId of neighbors) {
        if (!visited.has(neighborId)) {
          visited.add(neighborId);
          queue.push(neighborId);
        }
      }
    }

    // Check if all nodes are reachable.
    for (const node of graph.nodes.values()) {
      if (!visited.has(node.id)) {
        errors.push(
          new ConfigurationValidationError(
            `Node(${node.id}) is unreachable from START_FROM_TRIGGER node`,
            {
              configType: "workflow",
              context: {
                code: "UNREACHABLE_NODE_IN_TRIGGERED_SUBGRAPH",
                nodeId: node.id,
              },
            },
          ),
        );
      }
    }

    // Check if all nodes can reach at least one CONTINUE_FROM_TRIGGER.
    // For simplicity, we'll check reverse reachability from CONTINUE_FROM_TRIGGER nodes.
    const canReachEnd = new Set<ID>();
    const endQueue: ID[] = [...continueFromTriggerNodeIds];
    for (const id of continueFromTriggerNodeIds) {
      canReachEnd.add(id);
    }

    while (endQueue.length > 0) {
      const currentId = endQueue.shift()!;
      const predecessors = graph.getIncomingNeighbors(currentId);
      for (const predId of predecessors) {
        if (!canReachEnd.has(predId)) {
          canReachEnd.add(predId);
          endQueue.push(predId);
        }
      }
    }

    for (const node of graph.nodes.values()) {
      if (!canReachEnd.has(node.id)) {
        errors.push(
          new ConfigurationValidationError(
            `Node(${node.id}) cannot reach any CONTINUE_FROM_TRIGGER node`,
            {
              configType: "workflow",
              context: {
                code: "DEAD_END_NODE_IN_TRIGGERED_SUBGRAPH",
                nodeId: node.id,
              },
            },
          ),
        );
      }
    }

    return errors;
  }
}
