/**
 * ID Mapping Builder
 * Responsible for generating ID mappings and updating ID references in node/trigger configurations.
 */

import type {
  WorkflowDefinition,
  ID,
  IdMapping,
  SubgraphRelationship,
  WorkflowTrigger,
} from "@wf-agent/types";
import { GraphData } from "../entities/graph-data.js";
import { updateIdReferences } from "./utils/node-config-updaters.js";
import { generateSubgraphNamespace } from "../../utils/index.js";

/**
 * ID Mapping Builder Class
 * Responsible for generating ID mappings and updating ID references in the configuration.
 */
export class IdMappingBuilder {
  private nodeIndexCounter = 0;
  private edgeIndexCounter = 0;
  private idMapping: IdMapping = {
    nodeIds: new Map(),
    edgeIds: new Map(),
    reverseNodeIds: new Map(),
    reverseEdgeIds: new Map(),
    subgraphNamespaces: new Map(),
  };

  /**
   * Build an ID map and update the configuration
   * @param graph The constructed graph data
   * @param workflow The workflow definition
   * @returns The ID map and the updated configuration
   */
  async build(
    graph: GraphData,
    workflow: WorkflowDefinition,
  ): Promise<{
    idMapping: IdMapping;
    nodeConfigs: Map<ID, unknown>;
    triggerConfigs: Map<ID, unknown>;
    subgraphRelationships: SubgraphRelationship[];
  }> {
    // Step 1: Generate ID mappings for the nodes and edges in the graph.
    this.generateIdMapping(graph);

    // Step 2: Update node configuration
    const nodeConfigs = await this.updateNodeConfigs(workflow);

    // Step 3: Update trigger configuration
    const triggerConfigs = await this.updateTriggerConfigs(workflow);

    // Step 4: Constructing subgraph relationships
    const subgraphRelationships = await this.buildSubgraphRelationships(workflow);

    return {
      idMapping: this.idMapping,
      nodeConfigs,
      triggerConfigs,
      subgraphRelationships,
    };
  }

  /**
   * Generate an ID mapping for the nodes and edges in the graph.
   * @param graph Graph data
   */
  private generateIdMapping(graph: GraphData): void {
    // Generate an index ID mapping for the nodes.
    for (const node of graph.nodes.values()) {
      const originalId = node.originalNode?.id || node.id;
      if (!this.idMapping.nodeIds.has(originalId)) {
        this.allocateNodeId(originalId);
      }
    }

    // Generate an index ID mapping for the edges.
    for (const edge of graph.edges.values()) {
      const originalId = edge.originalEdge?.id || edge.id;
      if (!this.idMapping.edgeIds.has(originalId)) {
        this.allocateEdgeId(originalId);
      }
    }
  }

  /**
   * Update node configuration
   * @param workflow: Workflow definition
   * @returns: Node configuration map
   */
  private async updateNodeConfigs(workflow: WorkflowDefinition): Promise<Map<ID, unknown>> {
    const nodeConfigs = new Map<ID, unknown>();

    for (const node of workflow.nodes) {
      const indexId = this.idMapping.nodeIds.get(node.id);
      if (indexId === undefined) {
        continue;
      }

      // Use the updater to update the configuration.
      const updatedNode = updateIdReferences(node, this.idMapping);
      nodeConfigs.set(indexId.toString(), updatedNode.config);
    }

    return nodeConfigs;
  }

  /**
   * Update trigger configuration
   * @param workflow: Workflow definition
   * @returns: Trigger configuration mapping
   */
  private async updateTriggerConfigs(workflow: WorkflowDefinition): Promise<Map<ID, unknown>> {
    const triggerConfigs = new Map<ID, unknown>();

    if (!workflow.triggers) {
      return triggerConfigs;
    }

    for (const trigger of workflow.triggers) {
      // Skip TriggerReference and only process WorkflowTrigger.
      if (!("id" in trigger)) {
        continue;
      }

      // Update the ID references in the trigger configuration.
      const updatedTrigger = this.updateTriggerIdReferences(trigger as WorkflowTrigger);
      triggerConfigs.set(trigger.id, updatedTrigger);
    }

    return triggerConfigs;
  }

  /**
   * Update the ID references in the trigger configuration
   * @param trigger The trigger
   * @returns The updated trigger
   */
  private updateTriggerIdReferences(trigger: WorkflowTrigger): WorkflowTrigger {
    // TODO: 实现触发器配置的ID更新逻辑
    // The trigger configuration may not currently contain references to node IDs; return it as is.
    return trigger;
  }

  /**
   * Construct subgraph relationships
   * @param workflow Workflow definition
   * @returns Array of subgraph relationships
   */
  private async buildSubgraphRelationships(
    workflow: WorkflowDefinition,
  ): Promise<SubgraphRelationship[]> {
    const relationships: SubgraphRelationship[] = [];

    const subgraphNodes = workflow.nodes.filter(n => n.type === "SUBGRAPH");

    for (const subgraphNode of subgraphNodes) {
      const subgraphConfig = subgraphNode.config as unknown as Record<string, unknown>;
      if (!subgraphConfig || !subgraphConfig["subgraphId"]) {
        continue;
      }

      const subworkflowId = subgraphConfig["subgraphId"] as string;
      const namespace = this.idMapping.subgraphNamespaces.get(subworkflowId);

      if (!namespace) {
        // Generate subgraph namespaces
        const generatedNamespace = this.generateSubgraphNamespace(subworkflowId, subgraphNode.id);
        this.idMapping.subgraphNamespaces.set(subworkflowId, generatedNamespace);
      }

      const finalNamespace = this.idMapping.subgraphNamespaces.get(subworkflowId);

      if (finalNamespace) {
        relationships.push({
          parentWorkflowId: workflow.id,
          subgraphNodeId: subgraphNode.id,
          childWorkflowId: subworkflowId as string,
          namespace: finalNamespace as string,
        });
      }
    }

    return relationships;
  }

  /**
   * Assign a node index ID
   * @param originalId The original node ID
   * @returns The index ID
   */
  private allocateNodeId(originalId: ID): number {
    const index = this.nodeIndexCounter++;
    this.idMapping.nodeIds.set(originalId, index);
    this.idMapping.reverseNodeIds.set(index, originalId);
    return index;
  }

  /**
   * Assign edge index ID
   * @param originalId: Original edge ID
   * @returns: Index ID
   */
  private allocateEdgeId(originalId: ID): number {
    const index = this.edgeIndexCounter++;
    this.idMapping.edgeIds.set(originalId, index);
    this.idMapping.reverseEdgeIds.set(index, originalId);
    return index;
  }

  /**
   * Generate a subgraph namespace
   * @param subworkflowId Sub-workflow ID
   * @param subgraphNodeId SUBGRAPH node ID
   * @returns Namespace
   */
  private generateSubgraphNamespace(subworkflowId: ID, subgraphNodeId: ID): string {
    return generateSubgraphNamespace(subworkflowId, subgraphNodeId);
  }
}
