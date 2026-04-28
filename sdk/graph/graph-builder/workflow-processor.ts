/**
 * Workflow Processor
 * Responsible for processing workflows, including node expansion, graph construction, validation, etc.
 * Based on the existing GraphBuilder and GraphValidator
 * Integrates an ID mapping scheme
 * Returns a PreprocessedGraph instead of a ProcessedWorkflowDefinition
 */

import type {
  WorkflowDefinition,
  GraphBuildOptions,
  SubgraphMergeLog,
  PreprocessValidationResult,
  ID,
  PreprocessedGraph,
  NodeType,
} from "@wf-agent/types";
import type { Node } from "@wf-agent/types";
import type { WorkflowTrigger } from "@wf-agent/types";
import type { TriggerReference } from "@wf-agent/types";
import { GraphBuilder } from "./graph-builder.js";
import { GraphValidator } from "../validation/graph-validator.js";
import { WorkflowValidator } from "../validation/workflow-validator.js";
import { IdMappingBuilder } from "./id-mapping-builder.js";
import { PreprocessedGraphData } from "../entities/preprocessed-graph-data.js";
import { now } from "@wf-agent/common-utils";
import {
  ConfigurationValidationError,
  NodeTemplateNotFoundError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import { getContainer } from "../../core/di/index.js";
import * as Identifiers from "../../core/di/service-identifiers.js";
import type { GraphRegistry } from "../stores/graph-registry.js";

export interface ProcessOptions extends GraphBuildOptions {
  workflowRegistry?: { get: (id: string) => unknown };
  maxRecursionDepth?: number;
}

/**
 * Preprocessing Workflow
 * Return PreprocessedGraph instead of ProcessedWorkflowDefinition
 */
export async function processWorkflow(
  workflow: WorkflowDefinition,
  options: ProcessOptions = {},
): Promise<PreprocessedGraph> {
  const validator = new WorkflowValidator();

  // 1. Verify the workflow definition.
  const validationResult = validator.validate(workflow);
  if (validationResult.isErr()) {
    throw new ConfigurationValidationError(
      `Workflow validation failed: ${validationResult.error.map(e => e.message).join(", ")}`,
      {
        configType: "workflow",
        configPath: "workflow",
      },
    );
  }

  // 2. Expand node references
  const expandedNodes = expandNodeReferences(workflow.nodes);

  // 3. Expand trigger references
  const expandedTriggers = expandTriggerReferences(workflow.triggers || []);

  // 4. Create the expanded workflow definition
  const expandedWorkflow: WorkflowDefinition = {
    ...workflow,
    nodes: expandedNodes,
    triggers: expandedTriggers,
  };

  // 5. Building the graph
  const buildOptions: GraphBuildOptions = {
    validate: true,
    computeTopologicalOrder: true,
    detectCycles: true,
    analyzeReachability: true,
    maxRecursionDepth: options.maxRecursionDepth ?? 10,
    workflowRegistry: options.workflowRegistry,
  };

  const buildResult = GraphBuilder.buildAndValidate(expandedWorkflow, buildOptions);
  if (!buildResult.isValid) {
    throw new ConfigurationValidationError(`Graph build failed: ${buildResult.errors.join(", ")}`, {
      configType: "workflow",
      configPath: "workflow.graph",
    });
  }

  // 6. Handling Sub-workflows
  const subgraphMergeLogs: SubgraphMergeLog[] = [];
  let hasSubgraphs = false;
  const subworkflowIds = new Set<ID>();

  if (options.workflowRegistry) {
    const subgraphResult = await GraphBuilder.processSubgraphs(
      buildResult.graph,
      options.workflowRegistry,
      options.maxRecursionDepth ?? 10,
    );

    if (!subgraphResult.success) {
      throw new ConfigurationValidationError(
        `Subgraph processing failed: ${subgraphResult.errors.join(", ")}`,
        {
          configType: "workflow",
          configPath: "workflow.subgraphs",
        },
      );
    }

    // Record sub-workflow information
    if (subgraphResult.subworkflowIds.length > 0) {
      hasSubgraphs = true;
      subgraphResult.subworkflowIds.forEach(id => subworkflowIds.add(id));

      // Create a merge log for each sub-workflow.
      for (const subworkflowId of subgraphResult.subworkflowIds) {
        const subworkflow = options.workflowRegistry?.get(subworkflowId) as WorkflowDefinition;
        if (subworkflow) {
          // Find the corresponding SUBGRAPH node.
          const subgraphNode = workflow.nodes.find(
            node =>
              node.type === "SUBGRAPH" &&
              (node.config as { subgraphId?: string } | undefined)?.subgraphId === subworkflowId,
          );

          if (subgraphNode) {
            const mergeLog: SubgraphMergeLog = {
              subworkflowId,
              subworkflowName: subworkflow.name,
              subgraphNodeId: subgraphNode.id,
              nodeIdMapping: subgraphResult.nodeIdMapping,
              edgeIdMapping: subgraphResult.edgeIdMapping,
              mergedAt: now(),
            };
            subgraphMergeLogs.push(mergeLog);
          }
        }
      }
    }
  }

  // 7. Handling workflows referenced by triggers
  if (options.workflowRegistry) {
    const triggeredWorkflowIds = extractTriggeredWorkflowIds(expandedTriggers);
    const container = getContainer();
    const graphRegistry = container.get(Identifiers.GraphRegistry) as GraphRegistry;

    for (const triggeredWorkflowId of triggeredWorkflowIds) {
      // Get pre-processed graphs from graph-registry
      // Preprocessing logic has been moved to the workflow-registry and is handled automatically during registration
      const processedTriggeredWorkflow = graphRegistry.get(triggeredWorkflowId);

      if (!processedTriggeredWorkflow) {
        throw new WorkflowNotFoundError(
          `Triggered workflow '${triggeredWorkflowId}' referenced in triggers not found or not preprocessed`,
          triggeredWorkflowId,
        );
      }

      // Record the workflow ID referenced by the trigger.
      subworkflowIds.add(triggeredWorkflowId);
    }
  }

  // 8. Verify the image
  const graphValidationResult = GraphValidator.validate(buildResult.graph);
  if (graphValidationResult.isErr()) {
    const errors = graphValidationResult.error
      .map((e: { message: string }) => e.message)
      .join(", ");
    throw new ConfigurationValidationError(`Graph validation failed: ${errors}`, {
      configType: "workflow",
      configPath: "workflow.graph",
    });
  }

  // 9. Analysis Diagram
  const graphAnalysis = GraphValidator.analyze(buildResult.graph);

  // 10. Create preprocessing validation results
  const preprocessValidation: PreprocessValidationResult = {
    isValid: true,
    errors: [],
    warnings: [],
    validatedAt: now(),
  };

  // 11. Use IdMappingBuilder to generate the ID mapping and update the configuration.
  const idMappingBuilder = new IdMappingBuilder();
  const idMappingResult = await idMappingBuilder.build(buildResult.graph, expandedWorkflow);

  // 12. Create PreprocessedGraphData
  const preprocessedGraph = new PreprocessedGraphData();

  // Copy the graph structure (the graph constructed using GraphBuilder).
  preprocessedGraph.nodes = buildResult.graph.nodes;
  preprocessedGraph.edges = buildResult.graph.edges;
  preprocessedGraph.adjacencyList = buildResult.graph.adjacencyList;
  preprocessedGraph.reverseAdjacencyList = buildResult.graph.reverseAdjacencyList;
  preprocessedGraph.startNodeId = buildResult.graph.startNodeId;
  preprocessedGraph.endNodeIds = buildResult.graph.endNodeIds;

  // Set the fields related to ID mapping (using the results from IdMappingBuilder)
  preprocessedGraph.idMapping = idMappingResult.idMapping;
  preprocessedGraph.nodeConfigs = idMappingResult.nodeConfigs;
  preprocessedGraph.triggerConfigs = idMappingResult.triggerConfigs;
  preprocessedGraph.subgraphRelationships = idMappingResult.subgraphRelationships;

  // Set preprocessing metadata
  preprocessedGraph.graphAnalysis = graphAnalysis;
  preprocessedGraph.validationResult = preprocessValidation;
  preprocessedGraph.topologicalOrder = graphAnalysis.topologicalSort.sortedNodes;
  preprocessedGraph.subgraphMergeLogs = subgraphMergeLogs;
  preprocessedGraph.processedAt = now();

  // Set workflow metadata
  preprocessedGraph.workflowId = expandedWorkflow.id;
  preprocessedGraph.workflowVersion = expandedWorkflow.version;
  preprocessedGraph.triggers = expandedTriggers;
  preprocessedGraph.variables = expandedWorkflow.variables;
  preprocessedGraph.hasSubgraphs = hasSubgraphs;
  preprocessedGraph.subworkflowIds = subworkflowIds;

  return preprocessedGraph;
}

/**
 * Expand node references
 */
function expandNodeReferences(nodes: Node[]): Node[] {
  const expandedNodes: Node[] = [];

  for (const node of nodes) {
    // Check if it is a node reference.
    if (isNodeReference(node)) {
      const config = node.config as {
        templateName: string;
        nodeId: string;
        nodeName?: string;
        configOverride?: Record<string, unknown>;
      };
      const templateName = config.templateName;
      const nodeId = config.nodeId;
      const nodeName = config.nodeName;
      const configOverride = config.configOverride;

      // Get the node template
      const container = getContainer();
      const nodeTemplateRegistry = container.get(Identifiers.NodeTemplateRegistry) as {
        get: (name: string) =>
          | {
              type: string;
              name: string;
              config: Record<string, unknown>;
              description?: string;
              metadata?: Record<string, unknown>;
            }
          | undefined;
      };
      const template = nodeTemplateRegistry.get(templateName);
      if (!template) {
        throw new NodeTemplateNotFoundError(
          `Node template not found: ${templateName}`,
          templateName,
        );
      }

      // Merge configuration overrides
      const mergedConfig = configOverride
        ? { ...template.config, ...configOverride }
        : template.config;

      // Create the expanded node
      const expandedNode = {
        id: nodeId,
        type: template.type as NodeType,
        name: nodeName || template.name,
        config: mergedConfig,
        description: template.description,
        metadata: template.metadata,
        outgoingEdgeIds: node.outgoingEdgeIds,
        incomingEdgeIds: node.incomingEdgeIds,
      } as Node;

      expandedNodes.push(expandedNode);
    } else {
      // For regular nodes, just add them directly.
      expandedNodes.push(node);
    }
  }

  return expandedNodes;
}

/**
 * Check if the node is a node reference.
 */
function isNodeReference(node: Node): boolean {
  const config = node.config as Record<string, unknown> | undefined;
  return config !== null && typeof config === "object" && "templateName" in config;
}

/**
 * Expand trigger reference
 */
function expandTriggerReferences(
  triggers: (WorkflowTrigger | TriggerReference)[],
): WorkflowTrigger[] {
  const expandedTriggers: WorkflowTrigger[] = [];

  for (const trigger of triggers) {
    // Check if it is a trigger reference.
    if (isTriggerReference(trigger)) {
      const reference = trigger as TriggerReference;

      // Use the conversion method of TriggerTemplateRegistry
      const container = getContainer();
      const triggerTemplateRegistry = container.get(Identifiers.TriggerTemplateRegistry) as {
        convertToWorkflowTrigger: (
          templateName: string,
          triggerId: string,
          triggerName?: string,
          configOverride?: Record<string, unknown>,
        ) => WorkflowTrigger;
      };
      const workflowTrigger = triggerTemplateRegistry.convertToWorkflowTrigger(
        reference.templateName,
        reference.triggerId,
        reference.triggerName,
        reference.configOverride as Record<string, unknown> | undefined,
      );

      expandedTriggers.push(workflowTrigger);
    } else {
      // Regular trigger, add it directly.
      expandedTriggers.push(trigger as WorkflowTrigger);
    }
  }

  return expandedTriggers;
}

/**
 * Check whether the trigger is a trigger reference.
 */
function isTriggerReference(trigger: WorkflowTrigger | TriggerReference): boolean {
  const triggerObj = trigger as unknown as Record<string, unknown>;
  return triggerObj !== null && typeof triggerObj === "object" && "templateName" in triggerObj;
}

/**
 * Extract all workflow IDs referenced by EXECUTE_TRIGGERED_SUBGRAPH actions from the trigger list.
 * @param triggers: The list of triggers
 * @returns: A set of workflow IDs triggered
 */
function extractTriggeredWorkflowIds(triggers: WorkflowTrigger[]): Set<string> {
  const triggeredWorkflowIds = new Set<string>();

  for (const trigger of triggers) {
    const triggerObj = trigger as {
      action?: {
        type?: string;
        parameters?: { triggeredWorkflowId?: string };
      };
    };

    // Check whether the action type is EXECUTE_TRIGGERED_SUBGRAPH.
    if (triggerObj?.action?.type === "execute_triggered_subgraph") {
      const triggeredWorkflowId = triggerObj.action.parameters?.triggeredWorkflowId;
      if (triggeredWorkflowId) {
        triggeredWorkflowIds.add(triggeredWorkflowId);
      }
    }
  }

  return triggeredWorkflowIds;
}
