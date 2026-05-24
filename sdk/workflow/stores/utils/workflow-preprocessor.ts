/**
 * Workflow Preprocessor
 * Provides async preprocessing of workflow definitions, including graph building and validation.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import type { WorkflowRelationshipRegistry } from "../../stores/workflow-relationship-registry.js";
import { WorkflowGraphBuilder } from "../../builder/workflow-graph-builder.js";

/**
 * Preprocess a workflow by building and validating its graph.
 * Recursively processes EMBED_GRAPH dependents first, then builds the graph and merges subgraphs.
 * @param workflow Workflow template to preprocess
 * @param deps Dependencies including registries
 */
export async function preprocessWorkflow(
  workflow: WorkflowTemplate,
  deps: {
    workflowRegistry: WorkflowRegistry;
    graphRegistry: WorkflowGraphRegistry;
    relationshipRegistry: WorkflowRelationshipRegistry;
  },
): Promise<void> {
  // Check if it has already been preprocessed.
  if (deps.graphRegistry.has(workflow.id)) {
    return;
  }

  // Recursively preprocess EMBED_GRAPH dependents first
  // This ensures subworkflow graphs are available when the parent graph is expanded
  for (const node of workflow.nodes) {
    if (node.type === "EMBED_GRAPH") {
      const embedId = (node.config as { embedId?: string })?.embedId;
      if (embedId && !deps.graphRegistry.has(embedId)) {
        const subworkflow = deps.workflowRegistry.get(embedId);
        if (subworkflow) {
          await preprocessWorkflow(subworkflow, deps);
        }
      }
    }
  }

  // Use WorkflowGraphBuilder to build and validate the graph
  // buildAndValidate creates a WorkflowGraph with workflowId/workflowVersion preset
  const { graph, isValid, errors } = WorkflowGraphBuilder.buildAndValidate(workflow);

  if (!isValid) {
    throw new ConfigurationValidationError(`Workflow validation failed: ${errors.join(", ")}`, {
      configPath: workflow.id,
      context: { errors },
    });
  }

  // Expand EMBED_GRAPH nodes (merge subworkflow graphs into parent)
  const mergeResult = await WorkflowGraphBuilder.processSubgraphs(
    graph,
    deps.relationshipRegistry,
    deps.graphRegistry,
  );

  if (!mergeResult.success) {
    throw new ConfigurationValidationError(
      `EMBED_GRAPH expansion failed for workflow '${workflow.id}': ${mergeResult.errors.join("; ")}`,
      { configPath: workflow.id, context: { errors: mergeResult.errors } },
    );
  }

  // Cache processing results
  deps.graphRegistry.register(graph);
}
