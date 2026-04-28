/**
 * Graph Builder
 * Responsible for constructing a DirectedGraph from WorkflowDefinition
 * Supports recursive processing of sub-workflows
 */

import type {
  WorkflowDefinition,
  NodeType,
  ID,
  GraphNode,
  GraphEdge,
  GraphBuildOptions,
  SubgraphMergeOptions,
  SubgraphMergeResult,
  PreprocessedGraph,
} from "@wf-agent/types";
import { GraphData } from "../entities/graph-data.js";
import { GraphValidator } from "../validation/graph-validator.js";
import {
  generateSubgraphNamespace,
  generateNamespacedNodeId,
  generateNamespacedEdgeId,
  generateId,
} from "../../utils/index.js";
import { SUBGRAPH_METADATA_KEYS } from "@wf-agent/types";
import { getContainer } from "../../core/di/index.js";
import * as Identifiers from "../../core/di/service-identifiers.js";

/**
 * Graph Builder Class
 * Does not copy metadata from nodes or edges
 */
export class GraphBuilder {
  /**
   * Constructing a directed graph from workflow definitions
   */
  static build(workflow: WorkflowDefinition): GraphData {
    const graph = new GraphData();

    // Build nodes
    for (const node of workflow.nodes) {
      const graphNode: GraphNode = {
        id: node.id,
        type: node.type,
        name: node.name,
        description: node.description,
        originalNode: node,
        workflowId: workflow.id,
      };
      graph.addNode(graphNode);

      // Record the START and END nodes.
      if (node.type === ("START" as NodeType)) {
        graph.startNodeId = node.id;
      } else if (node.type === ("END" as NodeType)) {
        graph.endNodeIds.add(node.id);
      }
    }

    // Build a border
    for (const edge of workflow.edges) {
      const graphEdge: GraphEdge = {
        id: edge.id,
        sourceNodeId: edge.sourceNodeId,
        targetNodeId: edge.targetNodeId,
        type: edge.type,
        label: edge.label,
        description: edge.description,
        weight: edge.weight,
        originalEdge: edge,
      };
      graph.addEdge(graphEdge);
    }

    return graph;
  }

  /**
   * The complete build and verification process
   */
  static buildAndValidate(
    workflow: WorkflowDefinition,
    options: GraphBuildOptions = {},
  ): {
    graph: GraphData;
    isValid: boolean;
    errors: string[];
  } {
    // Build a graph
    const graph = this.build(workflow);

    // Handling the global uniqueness of Fork/Join Path IDs
    this.processForkJoinPathIds(graph);

    // Use GraphValidator for validation.
    const validationResult = GraphValidator.validate(graph, {
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
  private static processForkJoinPathIds(graph: GraphData): void {
    const pathIdMapping = new Map<ID, ID>(); // Original Path ID -> Globally Unique Path ID

    // Collect all Fork nodes and generate a globally unique Path ID.
    for (const node of graph.nodes.values()) {
      if (node.type === ("FORK" as NodeType)) {
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
      if (node.type === ("JOIN" as NodeType)) {
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
          // 设计目的：mainPathId必须指向forkPathIds中的一个值，当forkPathIds被更新为全局唯一ID后(避免重复)，mainPathId也必须更新
          const mainPathId = config?.["mainPathId"] as ID;
          if (mainPathId && pathIdMapping.has(mainPathId)) {
            config["mainPathId"] = pathIdMapping.get(mainPathId)!;
          }
        }
      }
    }
  }

  /**
   * Process sub-workflow nodes
   * Recursively merge the sub-workflow graph into the main graph
   * @param graph: The main graph
   * @param workflowRegistry: The workflow registry
   * @param maxRecursionDepth: The maximum recursion depth
   * @param currentDepth: The current recursion depth
   * @returns: The merged result
   */
  static async processSubgraphs(
    graph: GraphData,
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
    const nodeIdMapping = new Map<ID, ID>();
    const edgeIdMapping = new Map<ID, ID>();
    const addedNodeIds: ID[] = [];
    const addedEdgeIds: ID[] = [];
    const removedNodeIds: ID[] = [];
    const removedEdgeIds: ID[] = [];
    const errors: string[] = [];
    const subworkflowIds: ID[] = [];

    // Check the recursion depth.
    if (currentDepth >= maxRecursionDepth) {
      errors.push(`Maximum recursion depth (${maxRecursionDepth}) exceeded`);
      return {
        success: false,
        nodeIdMapping,
        edgeIdMapping,
        addedNodeIds,
        addedEdgeIds,
        removedNodeIds,
        removedEdgeIds,
        errors,
        subworkflowIds,
      };
    }

    // Find all SUBGRAPH nodes
    const subgraphNodes: GraphNode[] = [];
    for (const node of graph.nodes.values()) {
      if (node.type === ("SUBGRAPH" as NodeType)) {
        subgraphNodes.push(node);
      }
    }

    // Process each SUBGRAPH node
    for (const subgraphNode of subgraphNodes) {
      const subgraphConfig = subgraphNode.originalNode?.config as Record<string, unknown>;
      if (!subgraphConfig || !subgraphConfig["subgraphId"]) {
        errors.push(`SUBGRAPH node (${subgraphNode.id}) missing subgraphId`);
        continue;
      }

      const subworkflowId = subgraphConfig["subgraphId"] as ID;

      // Ensure that the sub-workflows have been fully preprocessed (including reference expansion and handling of nested sub-workflows).
      const container = getContainer();
      const graphRegistry = container.get(Identifiers.GraphRegistry) as {
        get: (id: string) => PreprocessedGraph | undefined;
      };
      let processedSubworkflow = graphRegistry.get(subworkflowId);

      if (!processedSubworkflow) {
        // If the sub-workflow is not preprocessed, preprocess it first.
        const subworkflow = workflowRegistry.get(subworkflowId);
        if (!subworkflow) {
          errors.push(
            `Subworkflow (${subworkflowId}) not found for SUBGRAPH node (${subgraphNode.id})`,
          );
          continue;
        }

        // Preprocess sub-workflows (all references and nested sub-workflows are recursively processed)
        // Note: The preprocessing logic has been moved to the workflow-registry, here we just get the preprocessed graph
        // If the graph does not exist, preprocessing failed or was not completed
        processedSubworkflow = graphRegistry.get(subworkflowId);
        if (!processedSubworkflow) {
          errors.push(
            `Failed to preprocess subworkflow (${subworkflowId}) for SUBGRAPH node (${subgraphNode.id})`,
          );
          continue;
        }
      }

      // Record the sub-workflow ID
      subworkflowIds.push(subworkflowId);

      // Generate a namespace
      const namespace = generateSubgraphNamespace(subworkflowId, subgraphNode.id);

      // Use the preprocessed sub-workflow graph (PreprocessedGraph itself is a Graph).
      const subgraph = processedSubworkflow;

      // Merge sub-workflow diagrams
      const mergeOptions = {
        nodeIdPrefix: namespace,
        edgeIdPrefix: namespace,
        preserveIdMapping: true,
        subworkflowId: subworkflowId,
        parentWorkflowId: subgraphNode.workflowId,
        depth: currentDepth + 1,
        workflowRegistry,
      };

      const mergeResult = this.mergeGraph(
        graph,
        subgraph as unknown as GraphData,
        subgraphNode.id,
        mergeOptions,
      );

      if (!mergeResult.success) {
        errors.push(
          `Failed to merge subworkflow (${subworkflowId}): ${mergeResult.errors.join(", ")}`,
        );
        continue;
      }

      // Update the mapping.
      mergeResult.nodeIdMapping.forEach((newId, oldId) => nodeIdMapping.set(oldId, newId));
      mergeResult.edgeIdMapping.forEach((newId, oldId) => edgeIdMapping.set(oldId, newId));
      addedNodeIds.push(...mergeResult.addedNodeIds);
      addedEdgeIds.push(...mergeResult.addedEdgeIds);
      removedNodeIds.push(...mergeResult.removedNodeIds);
      removedEdgeIds.push(...mergeResult.removedEdgeIds);
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
      subworkflowIds,
    };
  }

  /**
   * Merge sub-workflow diagrams into the main diagram
   * @param mainGraph: The main diagram
   * @param subgraph: The sub-workflow diagram (PreprocessedGraph extends Graph)
   * @param subgraphNodeId: The ID of the SUBGRAPH node
   * @param options: Merge options
   * @returns: The merged result
   */
  private static mergeGraph(
    mainGraph: GraphData,
    subgraph: GraphData, // `PreprocessedGraph` extends `Graph` and can accept...
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

      const newNode: GraphNode = {
        ...node,
        id: newId,
        originalNode: node.originalNode, // Keep the original references without performing a deep copy.
        workflowId: options.subworkflowId,
        parentWorkflowId: options.parentWorkflowId,
      };

      // Add the internalMetadata tag to the boundary nodes.
      if (node.type === ("START" as NodeType)) {
        newNode.internalMetadata = {
          ...newNode.internalMetadata,
          [SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE]: "entry",
          [SUBGRAPH_METADATA_KEYS.ORIGINAL_NODE_ID]: subgraphNodeId,
          [SUBGRAPH_METADATA_KEYS.NAMESPACE]: options.nodeIdPrefix,
          [SUBGRAPH_METADATA_KEYS.DEPTH]: options.depth,
        };
      } else if (node.type === ("END" as NodeType)) {
        newNode.internalMetadata = {
          ...newNode.internalMetadata,
          [SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE]: "exit",
          [SUBGRAPH_METADATA_KEYS.ORIGINAL_NODE_ID]: subgraphNodeId,
          [SUBGRAPH_METADATA_KEYS.NAMESPACE]: options.nodeIdPrefix,
          [SUBGRAPH_METADATA_KEYS.DEPTH]: options.depth,
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
      const newEdge: GraphEdge = {
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
          const newEdge: GraphEdge = {
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
          const newEdge: GraphEdge = {
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
