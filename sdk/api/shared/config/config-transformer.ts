/**
 * Configuration Converter
 * Responsible for converting the configuration file format to WorkflowTemplate
 */

import type { WorkflowConfigFile, IConfigTransformer } from "./types.js";
import type { WorkflowTemplate } from "@wf-agent/types";
import type { Node } from "@wf-agent/types";
import type { Edge as EdgeType } from "@wf-agent/types";
import { generateId } from "../../../utils/id-utils.js";
import { substituteParameters } from "./config-utils.js";

/**
 * Configuration converter class
 */
export class ConfigTransformer implements IConfigTransformer {
  /**
   * Converting the config file format to WorkflowTemplate
   * @param configFile The parsed configuration file.
   * @param parameters runtime parameters (for template replacement)
   * @returns WorkflowTemplate
   */
  transformToWorkflow(
    configFile: WorkflowConfigFile,
    parameters?: Record<string, unknown>,
  ): WorkflowTemplate {
    // 1. Flatten the workflow properties to top level (TOML format has workflow section)
    const flattenedConfig = this.flattenWorkflowConfig(configFile);

    // 2. Processing parameter substitution ({{parameters.xxx}} → actual values)
    const processedConfig = this.processParameters(flattenedConfig, parameters);

    // 3. Transform nodes from config format to internal format
    const transformedNodes = this.transformNodes(processedConfig.nodes || []);

    // 4. Transform edges from config format to internal format
    const transformedEdges = this.transformEdges(processedConfig.edges || []);

    // 5. Update edge references of the nodes
    this.updateNodeEdgeReferences(transformedNodes, transformedEdges);

    // 6. Build and return the complete WorkflowTemplate
    const now = Date.now();
    const workflowDef: WorkflowTemplate = {
      ...processedConfig,
      nodes: transformedNodes,
      edges: transformedEdges,
      createdAt: typeof processedConfig.createdAt === "number" ? processedConfig.createdAt : now,
      updatedAt: typeof processedConfig.updatedAt === "number" ? processedConfig.updatedAt : now,
    } as WorkflowTemplate;

    return workflowDef;
  }

  /**
   * Flatten workflow configuration from TOML format
   * TOML format: { workflow: { id, name, ... }, nodes: [...], edges: [...] }
   * WorkflowTemplate: { id, name, nodes: [...], edges: [...], ... }
   * @param configFile The parsed configuration file
   * @returns Flattened WorkflowTemplate
   */
  private flattenWorkflowConfig(configFile: WorkflowConfigFile): WorkflowTemplate {
    // Check if the config has a nested 'workflow' property (TOML format)
    const configRecord = configFile as unknown as Record<string, unknown>;
    if (configRecord["workflow"] && typeof configRecord["workflow"] === "object") {
      const { workflow, ...rest } = configRecord;
      return {
        ...(workflow as Record<string, unknown>),
        ...rest,
      } as unknown as WorkflowTemplate;
    }
    // Already in flat format (JSON format)
    return configFile;
  }

  /**
   * Handling parameter substitution
   * @param configFile Configuration file
   * @param parameters Runtime parameters
   * @returns Processed configuration file
   */
  private processParameters(
    configFile: WorkflowConfigFile,
    parameters?: Record<string, unknown>,
  ): WorkflowConfigFile {
    if (!parameters || Object.keys(parameters).length === 0) {
      return configFile;
    }

    return substituteParameters(configFile, parameters);
  }

  /**
   * Transform nodes from config format to internal format
   * Config format: { id, type, config, name? }
   * Internal format: { id, type, config, name, incomingEdgeIds, outgoingEdgeIds, ... }
   * @param nodeConfigs Array of node configurations
   * @returns Array of transformed nodes
   */
  private transformNodes(nodeConfigs: unknown[]): Node[] {
    return nodeConfigs.map(nodeConfig =>
      this.transformNode(
        nodeConfig as {
          id: string;
          type: string;
          name?: string;
          config?: unknown;
          description?: string;
          metadata?: unknown;
          properties?: unknown;
          hooks?: unknown;
          checkpointBeforeExecute?: boolean;
          checkpointAfterExecute?: boolean;
        },
      ),
    );
  }

  /**
   * Converting a single node configuration
   * @param nodeConfig Node Configuration
   * @returns Node object
   */
  private transformNode(nodeConfig: {
    id: string;
    type: string;
    name?: string;
    config?: unknown;
    description?: string;
    metadata?: unknown;
    properties?: unknown;
    hooks?: unknown;
    checkpointBeforeExecute?: boolean;
    checkpointAfterExecute?: boolean;
  }): Node {
    return {
      id: nodeConfig.id,
      type: nodeConfig.type,
      name: nodeConfig.name || nodeConfig.id, // Default name to id if not provided
      config: nodeConfig.config || {},
      description: nodeConfig.description,
      metadata: nodeConfig.metadata,
      incomingEdgeIds: [],
      outgoingEdgeIds: [],
      properties: nodeConfig.properties,
      hooks: nodeConfig.hooks,
      checkpointBeforeExecute: nodeConfig.checkpointBeforeExecute,
      checkpointAfterExecute: nodeConfig.checkpointAfterExecute,
    } as Node;
  }

  /**
   * Transform edges from config format to internal format
   * Config format: { from, to, condition? }
   * Internal format: { id, sourceNodeId, targetNodeId, type, condition?, ... }
   * @param edgeConfigs Array of edge configurations
   * @returns Array of transformed edges
   */
  private transformEdges(edgeConfigs: unknown[]): EdgeType[] {
    return edgeConfigs.map(edgeConfig =>
      this.transformEdge(
        edgeConfig as {
          id?: string;
          sourceNodeId?: string;
          from?: string;
          targetNodeId?: string;
          to?: string;
          type?: string;
          condition?: unknown;
          label?: string;
          description?: string;
          weight?: number;
          metadata?: unknown;
        },
      ),
    );
  }

  /**
   * Converting a Single Edge Configuration
   * @param edgeConfig Edge Configuration
   * @returns Edge object
   */
  private transformEdge(edgeConfig: {
    id?: string;
    sourceNodeId?: string;
    from?: string;
    targetNodeId?: string;
    to?: string;
    type?: string;
    condition?: unknown;
    label?: string;
    description?: string;
    weight?: number;
    metadata?: unknown;
  }): EdgeType {
    const hasCondition = edgeConfig.condition !== undefined;
    return {
      id: edgeConfig.id || generateId(),
      sourceNodeId: edgeConfig.sourceNodeId || edgeConfig.from || "", // Support both formats
      targetNodeId: edgeConfig.targetNodeId || edgeConfig.to || "", // Support both formats
      type: edgeConfig.type || (hasCondition ? "CONDITIONAL" : "DEFAULT"),
      condition: edgeConfig.condition,
      label: edgeConfig.label,
      description: edgeConfig.description,
      weight: edgeConfig.weight,
      metadata: edgeConfig.metadata,
    } as EdgeType;
  }

  /**
   * Update edge references of nodes
   * @param nodes array of nodes
   * @param edges array of edges
   */
  private updateNodeEdgeReferences(nodes: Node[], edges: EdgeType[]): void {
    // Creating a node ID to node mapping
    const nodeMap = new Map<string, Node>();
    for (const node of nodes) {
      nodeMap.set(node.id, node);
    }

    // Update edge references for each node
    for (const edge of edges) {
      const fromNode = nodeMap.get(edge.sourceNodeId);
      const toNode = nodeMap.get(edge.targetNodeId);

      if (fromNode) {
        fromNode.outgoingEdgeIds.push(edge.id);
      }

      if (toNode) {
        toNode.incomingEdgeIds.push(edge.id);
      }
    }
  }

  /**
   * Converting a WorkflowTemplate to a Configuration File Format
   * @param workflowDef WorkflowTemplate
   * @returns Configuration file format
   */
  transformFromWorkflow(workflowDef: WorkflowTemplate): WorkflowConfigFile {
    return workflowDef;
  }
}
