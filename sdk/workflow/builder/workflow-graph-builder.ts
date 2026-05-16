/**
 * Workflow Graph Builder
 * Responsible for constructing a WorkflowGraph from WorkflowTemplate
 * Supports recursive processing of sub-workflows
 */

import type {
  WorkflowTemplate,
  StaticNodeType,
  RuntimeNodeType,
  ID,
  WorkflowNode,
  WorkflowEdge,
  GraphBuildOptions,
  SubgraphMergeOptions,
  SubgraphMergeResult,
} from "@wf-agent/types";
import { WorkflowGraphData } from "../entities/workflow-graph-data.js";
import { WorkflowGraphValidator } from "./workflow-graph-validator.js";
import {
  generateNamespacedNodeId,
  generateNamespacedEdgeId,
  generateId,
} from "../../utils/index.js";
import type { GlobalContext } from "../../core/global-context.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "workflow-graph-builder" });

/**
 * Workflow Graph Builder Class
 * Does not copy metadata from nodes or edges
 */
export class WorkflowGraphBuilder {
  /**
   * Constructing a workflow graph from workflow template
   */
  static build(workflow: WorkflowTemplate): WorkflowGraphData {
    const graph = new WorkflowGraphData();

    // Build nodes
    for (const node of workflow.nodes) {
      const workflowNode = {
        id: node.id,
        type: node.type as RuntimeNodeType,
        config: node.config,
        hooks: node.hooks,
        checkpointBeforeExecute: node.checkpointBeforeExecute,
        checkpointAfterExecute: node.checkpointAfterExecute,
        
        // Runtime context (initialized, will be populated during preprocessing)
        internalMetadata: {},
        originalNode: node,
        workflowId: workflow.id,
        parentWorkflowId: undefined,
        outgoingEdgeIds: [],
        incomingEdgeIds: [],
        
        // Copy name for logging/debugging convenience (optional)
        name: node.name,
      } as WorkflowNode;
      graph.addNode(workflowNode);

      // Record the START and END nodes.
      if (node.type === "START") {
        graph.startNodeId = node.id;
      } else if (node.type === "END") {
        graph.endNodeIds.add(node.id);
      }
    }

    // Build edges
    for (const edge of workflow.edges) {
      const workflowEdge: WorkflowEdge = {
        id: edge.id,
        sourceNodeId: edge.sourceNodeId,
        targetNodeId: edge.targetNodeId,
        type: edge.type,
        label: edge.label,
        description: edge.description,
        weight: edge.weight,
        originalEdge: edge,
      };
      graph.addEdge(workflowEdge);
    }

    return graph;
  }

  /**
   * The complete build and verification process
   */
  static buildAndValidate(
    workflow: WorkflowTemplate,
    options: GraphBuildOptions = {},
  ): {
    graph: WorkflowGraphData;
    isValid: boolean;
    errors: string[];
  } {
    // Build a graph
    const graph = this.build(workflow);

    // Handling the global uniqueness of Fork/Join Path IDs
    this.processForkJoinPathIds(graph);

    // Use WorkflowGraphValidator for validation.
    const validationResult = WorkflowGraphValidator.validate(graph, {
      checkCycles: options.detectCycles,
      checkReachability: options.analyzeReachability,
      checkForkJoin: true,
      checkStartEnd: true,
      checkIsolatedNodes: true,
    });

    return {
      graph,
      isValid: validationResult.isOk(),
      errors: validationResult.isErr()
        ? validationResult.error.map((e: { message: string }) => e.message)
        : [],
    };
  }

  /**
   * Handle the global uniqueness of Fork/Join Path IDs
   * Generate a globally unique ID for each pathId in the forkPaths, ensuring that Fork and Join nodes use the same Path ID
   * @param graph Graph data
   */
  private static processForkJoinPathIds(graph: WorkflowGraphData): void {
    const pathIdMapping = new Map<ID, ID>(); // Original Path ID -> Globally Unique Path ID

    // Collect all Fork nodes and generate a globally unique Path ID.
    for (const node of graph.nodes.values()) {
      if (node.type === ("FORK" as StaticNodeType)) {
        const config = node.originalNode?.config as Record<string, unknown>;
        const forkPaths = config?.["forkPaths"] as Array<{ pathId: ID }>;
        if (forkPaths && Array.isArray(forkPaths)) {
          for (const forkPath of forkPaths) {
            const originalPathId = forkPath["pathId"];
            // If no global ID has been generated yet, then create a new one.
            if (!pathIdMapping.has(originalPathId)) {
              pathIdMapping.set(originalPathId, `path-${generateId()}`);
            }
            // Update the `pathId` in `forkPath` to a globally unique ID.
            forkPath["pathId"] = pathIdMapping.get(originalPathId)!;
          }
        }
      }
    }

    // Update the Path ID of all Join nodes.
    for (const node of graph.nodes.values()) {
      if (node.type === ("JOIN" as StaticNodeType)) {
        const config = node.originalNode?.config as Record<string, unknown>;
        const forkPathIds = config?.["forkPathIds"] as ID[];
        if (forkPathIds && Array.isArray(forkPathIds)) {
          const globalPathIds: ID[] = [];
          for (const originalPathId of forkPathIds) {
            // Use the globally generated ID generated earlier.
            if (pathIdMapping.has(originalPathId)) {
              globalPathIds.push(pathIdMapping.get(originalPathId)!);
            } else {
              // If the Fork node does not have this Path ID, generate a new one.
              pathIdMapping.set(originalPathId, `path-${generateId()}`);
              globalPathIds.push(pathIdMapping.get(originalPathId)!);
            }
          }
          // Update the Join node configuration.
          config["forkPathIds"] = globalPathIds;

          // Update mainPathId (if it exists).
          // Design purpose: mainPathId must point to one of the values in forkPathIds, when forkPathIds is updated to a globally unique ID (to avoid duplication), mainPathId must also be updated
          const mainPathId = config?.["mainPathId"] as ID;
          if (mainPathId && pathIdMapping.has(mainPathId)) {
            config["mainPathId"] = pathIdMapping.get(mainPathId)!;
          }
        }
      }
    }
  }

  /**
   * Process sub-workflow nodes - Handles both SUBGRAPH and EMBED_GRAPH
   * 
   * ARCHITECTURE CHANGE (Phase 1 & 3):
   * 
   * SUBGRAPH (Phase 1: Scheme C):
   * - OLD MODEL: Expanded during graph building (mergeGraph)
   * - NEW MODEL: Creates independent execution entities at runtime
   * - STATUS: @deprecated, will be removed in future version
   * 
   * EMBED_GRAPH (Phase 3):
   * - Lightweight graph expansion for pure control flow reuse
   * - NO variable isolation (shares parent's VariableManager)
   * - Strict validation: Cannot contain variables, triggers, or VARIABLE nodes
   * - Uses mergeGraph() internally for expansion
   * 
   * @param globalContext: Global context for accessing DI container
   * @param graph: The main graph
   * @param workflowRegistry: The workflow registry
   * @param maxRecursionDepth: The maximum recursion depth
   * @param currentDepth: The current recursion depth
   * @returns: Merge result containing mappings and changes
   */
  static async processSubgraphs(
    _globalContext: GlobalContext,
    graph: WorkflowGraphData,
    workflowRegistry: {
      get: (id: string) => unknown;
      registerSubgraphRelationship?: (
        parentWorkflowId: ID,
        subgraphNodeId: ID,
        subworkflowId: ID,
      ) => void;
    },
    maxRecursionDepth: number = 10,
    currentDepth: number = 0,
  ): Promise<SubgraphMergeResult> {
    if (currentDepth >= maxRecursionDepth) {
      return {
        success: false,
        nodeIdMapping: new Map<ID, ID>(),
        edgeIdMapping: new Map<ID, ID>(),
        addedNodeIds: [],
        addedEdgeIds: [],
        removedNodeIds: [],
        removedEdgeIds: [],
        errors: [`Maximum recursion depth (${maxRecursionDepth}) exceeded`],
        subworkflowIds: [],
      };
    }

    const allNodeIdMappings = new Map<ID, ID>();
    const allEdgeIdMappings = new Map<ID, ID>();
    const allAddedNodeIds: ID[] = [];
    const allAddedEdgeIds: ID[] = [];
    const allRemovedNodeIds: ID[] = [];
    const allRemovedEdgeIds: ID[] = [];
    const allErrors: string[] = [];
    const allSubworkflowIds: ID[] = [];

    // Collect all SUBGRAPH and EMBED_GRAPH nodes (check originalNode type)
    const subgraphNodes: Array<{ node: WorkflowNode; type: 'SUBGRAPH' | 'EMBED_GRAPH' }> = [];
    for (const node of graph.nodes.values()) {
      const originalType = node.originalNode?.type;
      if (originalType === "SUBGRAPH" || originalType === "EMBED_GRAPH") {
        subgraphNodes.push({ node, type: originalType as 'SUBGRAPH' | 'EMBED_GRAPH' });
      }
    }

    // Process each subgraph/embed_graph node
    for (const { node, type } of subgraphNodes) {
      try {
        const config = node.originalNode?.config as { subgraphId?: string; embedId?: string } | undefined;
        const subworkflowId = type === 'SUBGRAPH' ? config?.subgraphId : config?.embedId;
        
        if (!subworkflowId) {
          allErrors.push(`Node ${node.id} is missing ${type === 'SUBGRAPH' ? 'subgraphId' : 'embedId'} configuration`);
          continue;
        }

        // Get the subworkflow from registry
        const subworkflow = workflowRegistry.get(subworkflowId);
        if (!subworkflow) {
          allErrors.push(`Subworkflow '${subworkflowId}' not found in registry`);
          continue;
        }

        // Register relationship if available
        if (workflowRegistry.registerSubgraphRelationship) {
          workflowRegistry.registerSubgraphRelationship(
            (graph as any).workflowId || '',
            node.id,
            subworkflowId,
          );
        }

        // For EMBED_GRAPH, perform graph expansion using mergeGraph
        if (type === 'EMBED_GRAPH') {
          const subgraphGraph = (subworkflow as any).graph as WorkflowGraphData;
          if (!subgraphGraph) {
            allErrors.push(`EMBED_GRAPH '${node.id}' references invalid workflow '${subworkflowId}'`);
            continue;
          }

          // Validate EMBED_GRAPH constraints (no variables, no triggers)
          const validationErrors = this.validateEmbedGraphConstraints(subgraphGraph, node.id);
          if (validationErrors.length > 0) {
            allErrors.push(...validationErrors);
            continue;
          }

          // Perform graph expansion
          const mergeResult = this.mergeGraph(
            graph,
            subgraphGraph,
            node.id,
            {
              nodeIdPrefix: `${node.id}_`,
              edgeIdPrefix: `${node.id}_`,
              subworkflowId,
              parentWorkflowId: (graph as any).workflowId || '',
              depth: currentDepth + 1,
              workflowRegistry,
            },
          );

          // Collect results
          if (mergeResult.success) {
            for (const [key, value] of mergeResult.nodeIdMapping) {
              allNodeIdMappings.set(key, value);
            }
            for (const [key, value] of mergeResult.edgeIdMapping) {
              allEdgeIdMappings.set(key, value);
            }
            allAddedNodeIds.push(...mergeResult.addedNodeIds);
            allAddedEdgeIds.push(...mergeResult.addedEdgeIds);
            allRemovedNodeIds.push(...mergeResult.removedNodeIds);
            allRemovedEdgeIds.push(...mergeResult.removedEdgeIds);
            allSubworkflowIds.push(subworkflowId);
          } else {
            allErrors.push(...mergeResult.errors);
          }
        }
        // For SUBGRAPH, do NOT expand (Phase 1: Scheme C)
        // SUBGRAPH nodes remain in the graph and are executed as independent entities at runtime
        else {
          logger.debug(
            "SUBGRAPH node will not be expanded (Phase 1: Scheme C). " +
            "It will be executed as an independent child workflow at runtime.",
            { subgraphNodeId: node.id, subworkflowId }
          );
          allSubworkflowIds.push(subworkflowId);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        allErrors.push(`Failed to process ${type} node ${node.id}: ${errorMessage}`);
      }
    }

    return {
      success: allErrors.length === 0,
      nodeIdMapping: allNodeIdMappings,
      edgeIdMapping: allEdgeIdMappings,
      addedNodeIds: allAddedNodeIds,
      addedEdgeIds: allAddedEdgeIds,
      removedNodeIds: allRemovedNodeIds,
      removedEdgeIds: allRemovedEdgeIds,
      errors: allErrors,
      subworkflowIds: allSubworkflowIds,
    };
  }

  /**
   * Validate EMBED_GRAPH constraints
   * Ensures embedded workflows meet strict requirements:
   * - No variables defined
   * - No triggers
   * - No VARIABLE nodes
   * - No nested SUBGRAPH/EMBED_GRAPH with variables
   */
  private static validateEmbedGraphConstraints(
    subgraph: WorkflowGraphData,
    embedNodeId: ID,
  ): string[] {
    const errors: string[] = [];

    // Rule 1: Embedded workflow cannot define variables
    if ((subgraph as any).variables && (subgraph as any).variables.length > 0) {
      errors.push(
        `EMBED_GRAPH '${embedNodeId}' references workflow '${(subgraph as any).workflowId || 'unknown'}' ` +
        `which defines ${(subgraph as any).variables.length} variables. ` +
        `EmbedGraph workflows must be variable-free.`
      );
    }

    // Rule 2: Embedded workflow cannot have triggers
    if ((subgraph as any).triggers && (subgraph as any).triggers.length > 0) {
      errors.push(
        `EMBED_GRAPH '${embedNodeId}' references workflow '${(subgraph as any).workflowId || 'unknown'}' ` +
        `which defines ${(subgraph as any).triggers.length} triggers. ` +
        `EmbedGraph workflows cannot have triggers.`
      );
    }

    // Rule 3: Embedded workflow cannot contain VARIABLE nodes
    for (const node of subgraph.nodes.values()) {
      if (node.type === 'VARIABLE') {
        errors.push(
          `EMBED_GRAPH '${embedNodeId}' references workflow '${(subgraph as any).workflowId || 'unknown'}' ` +
          `which contains VARIABLE nodes. EmbedGraph workflows cannot modify variables.`
        );
        break;
      }
    }

    return errors;
  }

  /**
   * Merge sub-workflow diagrams into the main diagram
   * 
   * @deprecated For SUBGRAPH usage (Phase 1: Scheme C).
   * SUBGRAPH nodes are no longer expanded during graph building. Instead, they are
   * executed as independent child workflows at runtime using WorkflowExecutionBuilder.createSubgraph().
   * 
   * NOTE: This method is still used internally for EMBED_GRAPH expansion (Phase 3).
   * EMBED_GRAPH nodes perform lightweight graph expansion for pure control flow reuse.
   * 
   * For new code:
   * - Use SUBGRAPH when you need variable isolation and explicit mapping
   * - Use EMBED_GRAPH for performance-critical scenarios with simple control flow (no variables)
   * 
   * Migration Guide: See docs/architecture/workflow-agent/MIGRATION_GUIDE.md
   * 
   * @param mainGraph: The main diagram
   * @param subgraph: The sub-workflow diagram (WorkflowGraph extends WorkflowGraphStructure)
   * @param subgraphNodeId: The ID of the SUBGRAPH or EMBED_GRAPH node
   * @param options: Merge options
   * @returns: The merged result
   */
  private static mergeGraph(
    mainGraph: WorkflowGraphData,
    subgraph: WorkflowGraphData,
    subgraphNodeId: ID,
    options: SubgraphMergeOptions & {
      subworkflowId: ID;
      parentWorkflowId: ID;
      depth: number;
      workflowRegistry?: {
        registerSubgraphRelationship?: (
          parentWorkflowId: ID,
          subgraphNodeId: ID,
          subworkflowId: ID,
        ) => void;
      };
    },
  ): SubgraphMergeResult {
    // Phase 2: Add deprecation warning
    logger.warn(
      "DEPRECATED: mergeGraph() is deprecated. " +
      "SUBGRAPH nodes should now be executed as independent child workflows. " +
      "This method will be removed in a future version. " +
      "See docs/architecture/workflow-agent/MIGRATION_GUIDE.md for migration instructions.",
      { subgraphNodeId, subworkflowId: options.subworkflowId }
    );
    const nodeIdMapping = new Map<ID, ID>();
    const edgeIdMapping = new Map<ID, ID>();
    const addedNodeIds: ID[] = [];
    const addedEdgeIds: ID[] = [];
    const removedNodeIds: ID[] = [];
    const removedEdgeIds: ID[] = [];
    const errors: string[] = [];

    // Get the in-degree and out-degree of SUBGRAPH nodes.
    const incomingEdges = mainGraph.getIncomingEdges(subgraphNodeId);
    const outgoingEdges = mainGraph.getOutgoingEdges(subgraphNodeId);

    // Add a node for the sub-workflow (rename the ID)
    for (const node of subgraph.nodes.values()) {
      const newId = generateNamespacedNodeId(options.nodeIdPrefix || "", node.id);

      let newNode: WorkflowNode;
      
      // Transform START -> SUBGRAPH_START and END -> SUBGRAPH_END
      if (node.type === "START") {
        const startConfig = node.config as Record<string, unknown>;
        const subgraphStartConfig: Record<string, unknown> = {
          variableInputs: startConfig['variableInputs'],
          messageInputs: startConfig['messageInputs'],
          originalSubgraphNodeId: subgraphNodeId,
          namespace: options.nodeIdPrefix,
          depth: options.depth,
        };
        
        // Transfer variableInputs from SUBGRAPH node config to START node config
        const subgraphNode = mainGraph.getNode(subgraphNodeId);
        if (subgraphNode) {
          const subgraphConfig = subgraphNode.originalNode?.config as Record<string, unknown>;
          if (subgraphConfig && subgraphConfig['variableInputs']) {
            subgraphStartConfig['variableInputs'] = subgraphConfig['variableInputs'];
            logger.debug("Transferred variableInputs from SUBGRAPH node to SUBGRAPH_START node", {
              subgraphNodeId,
              startNodeId: newId,
              inputCount: (subgraphConfig['variableInputs'] as Array<Record<string, unknown>>).length,
            });
          }
        }
        
        newNode = {
          ...node,
          id: newId,
          type: "SUBGRAPH_START",
          config: subgraphStartConfig,
          originalNode: node.originalNode,
          workflowId: options.subworkflowId,
          parentWorkflowId: options.parentWorkflowId,
        };
        
      } else if (node.type === "END") {
        const endConfig = node.config as Record<string, unknown>;
        const subgraphEndConfig: Record<string, unknown> = {
          variableOutputs: endConfig['variableOutputs'],
          messageOutputs: endConfig['messageOutputs'],
          originalSubgraphNodeId: subgraphNodeId,
          namespace: options.nodeIdPrefix,
          depth: options.depth,
        };
        
        newNode = {
          ...node,
          id: newId,
          type: "SUBGRAPH_END",
          config: subgraphEndConfig,
          originalNode: node.originalNode,
          workflowId: options.subworkflowId,
          parentWorkflowId: options.parentWorkflowId,
        };
        
      } else {
        // Other nodes remain unchanged
        newNode = {
          ...node,
          id: newId,
          originalNode: node.originalNode,
          workflowId: options.subworkflowId,
          parentWorkflowId: options.parentWorkflowId,
        };
      }

      mainGraph.addNode(newNode);
      nodeIdMapping.set(node.id, newId);
      addedNodeIds.push(newId);
    }

    // Add an edge to the sub-workflow (rename the ID)
    for (const edge of subgraph.edges.values()) {
      const newId = generateNamespacedEdgeId(options.edgeIdPrefix || "", edge.id);
      const newSourceId = nodeIdMapping.get(edge.sourceNodeId) || edge.sourceNodeId;
      const newTargetId = nodeIdMapping.get(edge.targetNodeId) || edge.targetNodeId;

      const newEdge: WorkflowEdge = {
        ...edge,
        id: newId,
        sourceNodeId: newSourceId,
        targetNodeId: newTargetId,
        originalEdge: edge.originalEdge,
      };
      mainGraph.addEdge(newEdge);
      edgeIdMapping.set(edge.id, newId);
      addedEdgeIds.push(newId);
    }

    // Process the input mapping: Connect the incoming edges of the SUBGRAPH nodes to the START node of the sub-workflow.
    if (subgraph.startNodeId) {
      const newStartNodeId = nodeIdMapping.get(subgraph.startNodeId);
      if (newStartNodeId) {
        for (const incomingEdge of incomingEdges) {
          const newEdgeId = `${incomingEdge.id}_merged`;
          const newEdge: WorkflowEdge = {
            ...incomingEdge,
            id: newEdgeId,
            targetNodeId: newStartNodeId,
          };
          mainGraph.addEdge(newEdge);
          addedEdgeIds.push(newEdge.id);
          removedEdgeIds.push(incomingEdge.id);
        }
      }
    }

    // Process the output mapping: Connect the END node of the sub-workflow to the outgoing edge of the SUBGRAPH node.
    for (const endNodeId of subgraph.endNodeIds) {
      const newEndNodeId = nodeIdMapping.get(endNodeId);
      if (newEndNodeId) {
        for (const outgoingEdge of outgoingEdges) {
          const newEdgeId = `${outgoingEdge.id}_merged`;
          const newEdge: WorkflowEdge = {
            ...outgoingEdge,
            id: newEdgeId,
            sourceNodeId: newEndNodeId,
          };
          mainGraph.addEdge(newEdge);
          addedEdgeIds.push(newEdge.id);
          removedEdgeIds.push(outgoingEdge.id);
        }
      }
    }

    // Remove the SUBGRAPH node and its associated edges.
    mainGraph.nodes.delete(subgraphNodeId);
    removedNodeIds.push(subgraphNodeId);
    for (const edgeId of removedEdgeIds) {
      mainGraph.edges.delete(edgeId);
    }

    // Register workflow relationships
    if (options.workflowRegistry?.registerSubgraphRelationship) {
      options.workflowRegistry.registerSubgraphRelationship(
        options.parentWorkflowId,
        subgraphNodeId,
        options.subworkflowId,
      );
    }

    return {
      success: errors.length === 0,
      nodeIdMapping,
      edgeIdMapping,
      addedNodeIds,
      addedEdgeIds,
      removedNodeIds,
      removedEdgeIds,
      errors,
      subworkflowIds: [],
    };
  }
}
