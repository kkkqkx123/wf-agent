/**
 * WorkflowPreprocessor - Workflow Preprocessor
 * Responsible for async preprocessing of workflow definitions, including graph building and validation.
 *
 * This module only exports class definitions; instances are managed uniformly through DI container.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowRegistry } from "./workflow-registry.js";
import type { WorkflowGraphRegistry } from "./workflow-graph-registry.js";
import type { WorkflowRelationshipRegistry } from "./workflow-relationship-registry.js";
import { WorkflowGraphBuilder } from "../builder/workflow-graph-builder.js";

/**
 * WorkflowPreprocessor
 * Extracted preprocessing logic from WorkflowRegistry to isolate graph-building concerns.
 */
export class WorkflowPreprocessor {
  constructor(
    private readonly workflowRegistry: WorkflowRegistry,
    private readonly graphRegistry: WorkflowGraphRegistry,
    private readonly relationshipRegistry: WorkflowRelationshipRegistry,
  ) {}

  /**
   * Preprocess a workflow by building and validating its graph.
   * Recursively processes EMBED_GRAPH dependents first, then builds the graph and merges subgraphs.
   * @param workflow Workflow template to preprocess
   */
  async preprocess(workflow: WorkflowTemplate): Promise<void> {
    // Check if it has already been preprocessed.
    if (this.graphRegistry.has(workflow.id)) {
      return;
    }

    // Recursively preprocess EMBED_GRAPH dependents first
    // This ensures subworkflow graphs are available when the parent graph is expanded
    for (const node of workflow.nodes) {
      if (node.type === "EMBED_GRAPH") {
        const embedId = (node.config as { embedId?: string })?.embedId;
        if (embedId && !this.graphRegistry.has(embedId)) {
          const subworkflow = this.workflowRegistry.get(embedId);
          if (subworkflow) {
            await this.preprocess(subworkflow);
          }
        }
      }
    }

    // Use WorkflowGraphBuilder to build and validate the graph
    // buildAndValidate creates a WorkflowGraph with workflowId/workflowVersion preset
    const { graph, isValid, errors } = WorkflowGraphBuilder.buildAndValidate(workflow);

    if (!isValid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${errors.join(", ")}`,
        { configPath: workflow.id, context: { errors } },
      );
    }

    // Expand EMBED_GRAPH nodes (merge subworkflow graphs into parent)
    const mergeResult = await WorkflowGraphBuilder.processSubgraphs(
      graph,
      this.relationshipRegistry,
      this.graphRegistry,
    );

    if (!mergeResult.success) {
      throw new ConfigurationValidationError(
        `EMBED_GRAPH expansion failed for workflow '${workflow.id}': ${mergeResult.errors.join("; ")}`,
        { configPath: workflow.id, context: { errors: mergeResult.errors } },
      );
    }

    // Cache processing results
    this.graphRegistry.register(graph);
  }
}
