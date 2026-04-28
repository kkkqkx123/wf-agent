/**
 * Graph Structure Validator
 *
 * Responsibilities:
 * - Verify the topological structure and logical correctness of the graph.
 * - Check the in-degree and out-degree constraints of the START/END nodes (the quantity and existence of which have already been verified by WorkflowValidator).
 * - Detect circular dependencies (cycle detection).
 * - Analyze node reachability (paths from START to END).
 * - Verify the pairing relationships and business logic of FORK/JOIN nodes.
 * - Verify the special constraints of triggering sub-workflows (the combination of nodes has already been verified by WorkflowValidator).
 * - Verify the existence and interface compatibility of sub-workflows.
 *
 * Differences from WorkflowValidator:
 * - GraphValidator performs validation during the graph preprocessing phase; the input is GraphData.
 * - WorkflowValidator performs validation during the workflow registration phase; the input is WorkflowDefinition.
 * - GraphValidator verifies rules that depend on the graph structure (preprocessing phase).
 * - WorkflowValidator verifies all rules that can be determined during the definition phase (validation before registration).
 *
 * Validation Timing:
 * - Called after the GraphBuilder constructs the graph.
 * - Called during the recursive processing of sub-workflows.
 * - Called within the workflow preprocessing process.
 *
 * Prerequisites:
 * - The input graph data has passed the basic validation by WorkflowValidator.
 * - The basic data integrity of nodes and edges has been ensured.
 * - The existence of nodes referenced by edges has been verified.
 * - The quantity and existence of START/END nodes have been verified.
 * - The combination of nodes triggering sub-workflows has been verified.
 *
 * Does Not Include:
 * - Basic data integrity validation (handled by WorkflowValidator).
 * - Schema validation of node configurations (handled by WorkflowValidator).
 * - ID uniqueness validation (handled by WorkflowValidator).
 * - Verification of the existence of nodes referenced by edges (handled by WorkflowValidator).
 * - Verification of the quantity and existence of START/END nodes (handled by WorkflowValidator).
 * - Verification of the combination of nodes triggering sub-workflows (handled by WorkflowValidator).
 */

import type { ID, NodeType, GraphValidationOptions, WorkflowGraphAnalysis } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { WorkflowGraphData } from "../entities/workflow-graph-data.js";
import { SUBGRAPH_METADATA_KEYS } from "@wf-agent/types";
import { analyzeWorkflowGraph } from "../builder/utils/workflow-graph-analyzer.js";
import { detectCycles } from "../builder/utils/workflow-cycle-detector.js";
import { analyzeReachability } from "../builder/utils/workflow-reachability-analyzer.js";
import { getReachableNodes } from "../builder/utils/workflow-traversal.js";

/**
 * Image Validator Class
 */
export class GraphValidator {
  /**
   * Verify the graph structure.
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
      checkSubgraphExistence: false,
      checkSubgraphCompatibility: false,
      ...options,
    };

    // Check if it is a trigger sub-workflow.
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
        // The trigger workflow only verifies internal connectivity.
        const connectivityErrors = this.validateTriggeredSubgraphConnectivity(graph);
        errorList.push(...connectivityErrors);
      } else {
        // Verify the reachability from START to END in the normal workflow.
        const reachabilityResult = analyzeReachability(graph);

        //  unreachable node
        for (const nodeId of reachabilityResult.unreachableNodes) {
          errorList.push(
            new ConfigurationValidationError(`节点(${nodeId})从START节点不可达`, {
              configType: "workflow",
              context: {
                code: "UNREACHABLE_NODE",
                nodeId,
              },
            }),
          );
        }

        // Dead node
        for (const nodeId of reachabilityResult.deadEndNodes) {
          errorList.push(
            new ConfigurationValidationError(`节点(${nodeId})无法到达END节点`, {
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

    // Check the existence of the sub-workflow.
    if (opts.checkSubgraphExistence) {
      const subgraphErrors = this.validateSubgraphExistence(graph);
      errorList.push(...subgraphErrors);
    }

    // Check the compatibility of sub-workflow interfaces.
    if (opts.checkSubgraphCompatibility) {
      const compatibilityErrors = this.validateSubgraphCompatibility(graph);
      errorList.push(...compatibilityErrors);
    }

    if (errorList.length === 0) {
      return ok(graph);
    }
    return err(errorList);
  }

  /**
   * Verify the in-degree and out-degree constraints of the START and END nodes.
   *
   * Note: The number and presence of START/END nodes have already been verified in WorkflowValidator.
   * This method only verifies the topological constraints:
   * - A START node cannot have any incoming edges.
   * - An END node cannot have any outgoing edges.
   * - The uniqueness of START nodes (excluding boundary nodes of sub-workflows).
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
            new ConfigurationValidationError(`END节点(${endNodeId})不能有出边`, {
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
    // Eliminate sub-workflow boundary nodes (START nodes marked as entry)
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
          new ConfigurationValidationError(`节点(${node.id})是孤立节点，既没有入边也没有出边`, {
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
   *
   * The validation includes:
   * - The validity of the forkPaths configuration for the FORK node
   * - The validity of the forkPathIds and mainPathId configurations for the JOIN node
   * - The pairing relationship between the FORK and JOIN nodes
   * - The global uniqueness of the forkPathIds
   * - The reachability from FORK to JOIN
   *
   * Note: The schema validation of the node configurations has already been completed in WorkflowValidator.
   * This method focuses on verifying the business logic and pairing relationship of FORK/JOIN.
   *
   * @param graph Graph data
   * @returns List of validation errors
   */
  private static validateForkJoinPairs(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];
    // Use the first element of the forkPathIds array as the pairing identifier.
    const forkNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[] }>(); // forkPathIds[0] -> {nodeId, forkPathIds}
    const joinNodes = new Map<ID, { nodeId: ID; forkPathIds: ID[]; mainPathId?: ID }>(); // forkPathIds[0] -> {nodeId, forkPathIds, mainPathId}
    const pairs = new Map<ID, ID>();
    const allForkPathIds = new Set<ID>(); // Used to check the global uniqueness of forkPathId

    // Collect all FORK and JOIN nodes.
    for (const node of graph.nodes.values()) {
      if (node.type === ("FORK" as NodeType)) {
        const config = node.originalNode?.config as
          | { forkPaths?: Array<{ pathId: ID; childNodeId: ID }> }
          | undefined;
        const forkPaths = config?.forkPaths;

        // Verify the configuration of the Fork node.
        if (!forkPaths || !Array.isArray(forkPaths) || forkPaths.length === 0) {
          errors.push(
            new ConfigurationValidationError(`FORK节点(${node.id})的forkPaths必须是非空数组`, {
              configType: "workflow",
              context: {
                code: "INVALID_FORK_PATHS",
                nodeId: node.id,
              },
            }),
          );
          continue;
        }

        // Extract pathId and childNodeId from forkPaths.
        const forkPathIds: ID[] = [];
        const childNodeIds: string[] = [];
        for (const forkPath of forkPaths) {
          if (!forkPath.pathId || !forkPath.childNodeId) {
            errors.push(
              new ConfigurationValidationError(
                `FORK节点(${node.id})的forkPaths中的每个元素必须包含pathId和childNodeId`,
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
          childNodeIds.push(forkPath.childNodeId);
        }

        // Check if pathId is unique within the workflow definition.
        for (const forkPathId of forkPathIds) {
          if (allForkPathIds.has(forkPathId)) {
            errors.push(
              new ConfigurationValidationError(
                `FORK节点(${node.id})的pathId(${forkPathId})在工作流定义内部不唯一`,
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

        // Use the pathId of the first element from forkPaths as the pairing identifier.
        if (forkPathIds.length === 0) {
          continue;
        }
        const pairId = forkPathIds[0]!;
        if (forkNodes.has(pairId)) {
          errors.push(
            new ConfigurationValidationError(
              `FORK节点(${node.id})的forkPaths第一个元素的pathId(${pairId})已被其他FORK节点使用`,
              {
                configType: "workflow",
                context: {
                  code: "DUPLICATE_FORK_PAIR_ID",
                  nodeId: node.id,
                  pairId,
                },
              },
            ),
          );
        } else {
          forkNodes.set(pairId, { nodeId: node.id, forkPathIds });
        }
      } else if (node.type === ("JOIN" as NodeType)) {
        const config = node.originalNode?.config as
          | { forkPathIds?: ID[]; mainPathId?: ID }
          | undefined;
        const forkPathIds = config?.forkPathIds;
        const mainPathId = config?.mainPathId;

        // Verify the Join node configuration.
        if (!forkPathIds || !Array.isArray(forkPathIds) || forkPathIds.length === 0) {
          errors.push(
            new ConfigurationValidationError(`JOIN节点(${node.id})的forkPathIds必须是非空数组`, {
              configType: "workflow",
              context: {
                code: "INVALID_FORK_PATH_IDS",
                nodeId: node.id,
              },
            }),
          );
          continue;
        }

        // Verify mainPathId
        if (mainPathId && !forkPathIds.includes(mainPathId)) {
          errors.push(
            new ConfigurationValidationError(
              `JOIN节点(${node.id})的mainPathId(${mainPathId})必须在forkPathIds中`,
              {
                configType: "workflow",
                context: {
                  code: "MAIN_PATH_ID_NOT_FOUND",
                  nodeId: node.id,
                  mainPathId,
                },
              },
            ),
          );
          continue;
        }

        // Use the first element of forkPathIds as the pairing identifier.
        const pairId = forkPathIds[0];
        if (pairId && joinNodes.has(pairId)) {
          errors.push(
            new ConfigurationValidationError(
              `The first element (${pairId}) of the forkPathIds of a JOIN node (${node.id}) is already used by another JOIN node.`,
              {
                configType: "workflow",
                context: {
                  code: "DUPLICATE_JOIN_PAIR_ID",
                  nodeId: node.id,
                  pairId,
                },
              },
            ),
          );
        } else if (pairId) {
          joinNodes.set(pairId, { nodeId: node.id, forkPathIds, mainPathId });
        }
      }
    }

    // Check the pairing.
    const unpairedForks: ID[] = [];
    const unpairedJoins: ID[] = [];

    for (const [pairId, forkInfo] of forkNodes) {
      if (joinNodes.has(pairId)) {
        const joinInfo = joinNodes.get(pairId)!;
        // Verify that the forkPathIds arrays of Fork and Join are identical (including the order).
        if (JSON.stringify(forkInfo.forkPathIds) !== JSON.stringify(joinInfo.forkPathIds)) {
          errors.push(
            new ConfigurationValidationError(
              `FORK节点(${forkInfo.nodeId})和JOIN节点(${joinInfo.nodeId})的forkPathIds不一致`,
              {
                configType: "workflow",
                context: {
                  code: "FORK_JOIN_MISMATCH",
                  forkNodeId: forkInfo.nodeId,
                  joinNodeId: joinInfo.nodeId,
                },
              },
            ),
          );
        } else {
          pairs.set(forkInfo.nodeId, joinInfo.nodeId);
        }
      } else {
        unpairedForks.push(forkInfo.nodeId);
      }
    }

    for (const [pairId, joinInfo] of joinNodes) {
      if (!forkNodes.has(pairId)) {
        unpairedJoins.push(joinInfo.nodeId);
      }
    }

    // Report on unmatched FORK nodes.
    for (const forkNodeId of unpairedForks) {
      errors.push(
        new ConfigurationValidationError(`FORK节点(${forkNodeId})没有配对的JOIN节点`, {
          configType: "workflow",
          context: {
            code: "UNPAIRED_FORK",
            nodeId: forkNodeId,
          },
        }),
      );
    }

    // Report of unmatched JOIN nodes
    for (const joinNodeId of unpairedJoins) {
      errors.push(
        new ConfigurationValidationError(`JOIN节点(${joinNodeId})没有配对的FORK节点`, {
          configType: "workflow",
          context: {
            code: "UNPAIRED_JOIN",
            nodeId: joinNodeId,
          },
        }),
      );
    }

    // Check the reachability from FORK to JOIN.
    for (const [forkNodeId, joinNodeId] of pairs) {
      const reachableNodes = getReachableNodes(graph, forkNodeId);
      if (!reachableNodes.has(joinNodeId)) {
        errors.push(
          new ConfigurationValidationError(
            `FORK节点(${forkNodeId})无法到达配对的JOIN节点(${joinNodeId})`,
            {
              configType: "workflow",
              context: {
                code: "FORK_JOIN_NOT_REACHABLE",
                nodeId: forkNodeId,
                relatedNodeId: joinNodeId,
              },
            },
          ),
        );
      }
    }

    return errors;
  }

  /**
   * Complete graph analysis
   */
  static analyze(graph: WorkflowGraphData): WorkflowGraphAnalysis {
    return analyzeWorkflowGraph(graph);
  }

  /**
   * Verify the existence of sub-workflows
   * @param graph Graph data
   * @returns List of verification errors
   */
  private static validateSubgraphExistence(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    for (const node of graph.nodes.values()) {
      if (node.type === ("SUBGRAPH" as NodeType)) {
        const _subgraphConfig = node.originalNode?.config as { subgraphId?: string } | undefined;
        if (!_subgraphConfig || !_subgraphConfig.subgraphId) {
          errors.push(
            new ConfigurationValidationError(`SUBGRAPH节点(${node.id})缺少subgraphId配置`, {
              configType: "workflow",
              context: {
                code: "MISSING_SUBGRAPH_ID",
                nodeId: node.id,
              },
            }),
          );
        }
      }
    }

    return errors;
  }

  /**
   * Verify sub-workflow interface compatibility
   * @param graph Graph data
   * @returns List of verification errors
   */
  private static validateSubgraphCompatibility(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    for (const node of graph.nodes.values()) {
      if (node.type === ("SUBGRAPH" as NodeType)) {
        const _subgraphConfig = node.originalNode?.config as { subgraphId?: string } | undefined;
      }
    }

    return errors;
  }

  /**
   * Check if it is a trigger workflow
   * @param graph Graph data
   * @returns Whether it is a trigger workflow
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
   * Verify the topological constraints of the trigger workflow
   *
   * Note: The combination of nodes (in terms of quantity and presence) has already been verified in WorkflowValidator.
   * This method only verifies the following topological constraints:
   * - The START_FROM_TRIGGER node cannot have any incoming edges.
   * - The CONTINUE_FROM_TRIGGER node cannot have any outgoing edges.
   * - It cannot contain a regular START node.
   * - It cannot contain a regular END node.
   *
   * @param graph Graph data
   * @returns List of verification errors
   */
  private static validateTriggeredSubgraphNodes(graph: WorkflowGraphData): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Check the START_FROM_TRIGGER node.
    const startFromTriggerNodes: ID[] = [];
    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        startFromTriggerNodes.push(node.id);
      }
    }

    if (startFromTriggerNodes.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          "The trigger subworkflow must contain a START_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MISSING_START_FROM_TRIGGER_NODE",
            },
          },
        ),
      );
    } else if (startFromTriggerNodes.length > 1) {
      errors.push(
        new ConfigurationValidationError(
          "The trigger subworkflow can contain only one START_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MULTIPLE_START_FROM_TRIGGER_NODES",
            },
          },
        ),
      );
    } else {
      // Check the in-degree of the START_FROM_TRIGGER node.
      const startNodeId = startFromTriggerNodes[0]!;
      const incomingEdges = graph.getIncomingEdges(startNodeId);
      if (incomingEdges.length > 0) {
        errors.push(
          new ConfigurationValidationError("START_FROM_TRIGGER node cannot have incoming edges", {
            configType: "workflow",
            context: {
              code: "START_FROM_TRIGGER_NODE_HAS_INCOMING_EDGES",
              nodeId: startNodeId,
            },
          }),
        );
      }
    }

    // Check the CONTINUE_FROM_TRIGGER node.
    const continueFromTriggerNodes: ID[] = [];
    for (const node of graph.nodes.values()) {
      if (node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)) {
        continueFromTriggerNodes.push(node.id);
      }
    }

    if (continueFromTriggerNodes.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          "The trigger child workflow must contain a CONTINUE_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MISSING_CONTINUE_FROM_TRIGGER_NODE",
            },
          },
        ),
      );
    } else if (continueFromTriggerNodes.length > 1) {
      errors.push(
        new ConfigurationValidationError(
          "The trigger subworkflow can only contain one CONTINUE_FROM_TRIGGER node",
          {
            configType: "workflow",
            context: {
              code: "MULTIPLE_CONTINUE_FROM_TRIGGER_NODES",
            },
          },
        ),
      );
    } else {
      // Check the outdegree of the CONTINUE_FROM_TRIGGER node.
      const endNodeId = continueFromTriggerNodes[0]!;
      const outgoingEdges = graph.getOutgoingEdges(endNodeId);
      if (outgoingEdges.length > 0) {
        errors.push(
          new ConfigurationValidationError(
            "CONTINUE_FROM_TRIGGER node cannot have outgoing edges",
            {
              configType: "workflow",
              context: {
                code: "CONTINUE_FROM_TRIGGER_NODE_HAS_OUTGOING_EDGES",
                nodeId: endNodeId,
              },
            },
          ),
        );
      }
    }

    // Check if it contains regular START or END nodes.
    for (const node of graph.nodes.values()) {
      if (node.type === ("START" as NodeType)) {
        errors.push(
          new ConfigurationValidationError("The trigger subworkflow cannot contain a START node", {
            configType: "workflow",
            context: {
              code: "TRIGGERED_SUBGRAPH_CONTAINS_START_NODE",
              nodeId: node.id,
            },
          }),
        );
      }
      if (node.type === ("END" as NodeType)) {
        errors.push(
          new ConfigurationValidationError("Trigger subworkflows cannot contain END nodes", {
            configType: "workflow",
            context: {
              code: "TRIGGERED_SUBGRAPH_CONTAINS_END_NODE",
              nodeId: node.id,
            },
          }),
        );
      }
    }

    return errors;
  }

  /**
   * Verify the internal connectivity of the trigger workflow
   * Ensure that all nodes can be reached from START_FROM_TRIGGER and also from CONTINUE_FROM_TRIGGER
   * @param graph Graph data
   * @returns List of verification errors
   */
  private static validateTriggeredSubgraphConnectivity(
    graph: WorkflowGraphData,
  ): ConfigurationValidationError[] {
    const errors: ConfigurationValidationError[] = [];

    // Find the START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER nodes.
    let startNodeId: ID | null = null;
    let endNodeId: ID | null = null;

    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        startNodeId = node.id;
      } else if (node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)) {
        endNodeId = node.id;
      }
    }

    if (!startNodeId || !endNodeId) {
      // If the required nodes are missing, the error has already been reported in validateTriggeredSubgraphNodes.
      return errors;
    }

    // Check the reachability from START_FROM_TRIGGER to all nodes.
    const reachableFromStart = getReachableNodes(graph, startNodeId);
    for (const node of graph.nodes.values()) {
      if (node.type === ("START_FROM_TRIGGER" as NodeType)) {
        continue; // Skip the starting node
      }
      if (!reachableFromStart.has(node.id)) {
        errors.push(
          new ConfigurationValidationError(`节点(${node.id})从START_FROM_TRIGGER节点不可达`, {
            configType: "workflow",
            context: {
              code: "UNREACHABLE_FROM_START_FROM_TRIGGER",
              nodeId: node.id,
            },
          }),
        );
      }
    }

    // Check the reachability from all nodes to CONTINUE_FROM_TRIGGER.
    for (const node of graph.nodes.values()) {
      if (node.type === ("CONTINUE_FROM_TRIGGER" as NodeType)) {
        continue; // Skip the end node.
      }
      const reachableFromNode = getReachableNodes(graph, node.id);
      if (!reachableFromNode.has(endNodeId)) {
        errors.push(
          new ConfigurationValidationError(`节点(${node.id})无法到达CONTINUE_FROM_TRIGGER节点`, {
            configType: "workflow",
            context: {
              code: "CANNOT_REACH_CONTINUE_FROM_TRIGGER",
              nodeId: node.id,
            },
          }),
        );
      }
    }

    return errors;
  }
}
