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
 * - Verify LOOP_START/LOOP_END pairing and cross-node references.
 *
 * Differences from WorkflowValidator:
 * - GraphValidator performs validation during the graph preprocessing phase; the input is GraphData.
 * - WorkflowValidator performs validation during the workflow registration phase; the input is WorkflowTemplate.
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

import type { WorkflowGraphAnalysis } from "../../types/graph/analysis.js";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { WorkflowGraphStructure } from "../../entities/workflow-graph-structure.js";
import { analyzeWorkflowGraph } from "../../builder/utils/workflow-graph-analyzer.js";
import { detectCycles } from "../../builder/utils/workflow-cycle-detector.js";
import { analyzeReachability } from "../../builder/utils/workflow-reachability-analyzer.js";

// Import specialized validators
import { validateStartEndNodes, validateTriggeredSubgraphNodes } from "./start-end-validator.js";
import { validateIsolatedNodes } from "./isolated-node-validator.js";
import { validateForkJoinPairs } from "./fork-join-validator.js";
import { validateSubgraphExistence, validateSubgraphCompatibility } from "./subgraph-validator.js";
import {
  validateEmbedGraphExistence,
  validateEmbedGraphConstraints,
} from "./embed-graph-validator.js";
import { validateSyncNodes } from "./sync-node-validator.js";
import {
  isTriggeredSubgraph,
  validateTriggeredSubgraphConnectivity,
} from "./triggered-subgraph-validator.js";
import { validateLoopPairs } from "./loop-pair-validator.js";

/**
 * Graph Validator Class
 */
export class GraphValidator {
  /**
   * Verify the graph structure.
   * All validation rules are mandatory and always enabled.
   */
  static validate(
    graph: WorkflowGraphStructure,
  ): Result<WorkflowGraphStructure, ConfigurationValidationError[]> {
    const errorList: ConfigurationValidationError[] = [];

    // Check if it is a trigger sub-workflow.
    const _isTriggeredSubgraph = isTriggeredSubgraph(graph);

    // Check the START/END nodes.
    const startEndErrors = _isTriggeredSubgraph
      ? validateTriggeredSubgraphNodes(graph)
      : validateStartEndNodes(graph);
    errorList.push(...startEndErrors);

    // Check isolated nodes.
    const isolatedErrors = validateIsolatedNodes(graph);
    errorList.push(...isolatedErrors);

    // Detect loop
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

    // Reachability Analysis
    if (_isTriggeredSubgraph) {
      // The trigger workflow only verifies internal connectivity.
      const connectivityErrors = validateTriggeredSubgraphConnectivity(graph);
      errorList.push(...connectivityErrors);
    } else {
      // Verify the reachability from START to END in the normal workflow.
      const reachabilityResult = analyzeReachability(graph);

      // Unreachable node
      for (const nodeId of reachabilityResult.unreachableNodes) {
        errorList.push(
          new ConfigurationValidationError(`Node (${nodeId}) is not reachable from START node`, {
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
          new ConfigurationValidationError(`Node (${nodeId}) cannot reach END node`, {
            configType: "workflow",
            context: {
              code: "DEAD_END_NODE",
              nodeId,
            },
          }),
        );
      }
    }

    // FORK/JOIN pairing verification
    const forkJoinErrors = validateForkJoinPairs(graph);
    errorList.push(...forkJoinErrors);

    // LOOP_START/LOOP_END pairing verification
    const loopPairErrors = validateLoopPairs(graph);
    errorList.push(...loopPairErrors);

    // Check the existence of the sub-workflow.
    const subgraphErrors = validateSubgraphExistence(graph);
    errorList.push(...subgraphErrors);

    // Validate EMBED_GRAPH nodes
    const embedGraphErrors = validateEmbedGraphExistence(graph);
    errorList.push(...embedGraphErrors);

    // Validate EMBED_GRAPH constraints
    const embedGraphConstraintErrors = validateEmbedGraphConstraints(graph);
    errorList.push(...embedGraphConstraintErrors);

    // Validate SYNC nodes
    const syncErrors = validateSyncNodes(graph);
    errorList.push(...syncErrors);

    // Check the compatibility of sub-workflow interfaces.
    const compatibilityErrors = validateSubgraphCompatibility(graph);
    errorList.push(...compatibilityErrors);

    if (errorList.length === 0) {
      return ok(graph);
    }
    return err(errorList);
  }

  /**
   * Complete graph analysis
   */
  static analyze(graph: WorkflowGraphStructure): WorkflowGraphAnalysis {
    return analyzeWorkflowGraph(graph);
  }
}
