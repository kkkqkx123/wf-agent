/**
 * Workflow Tool Type Definitions
 *
 * Types for workflow tool handler configuration, mirroring the skill handler pattern.
 */

import type { WorkflowTemplateType } from "@wf-agent/types";

/**
 * Simplified variable info for workflow discovery.
 * Only includes fields relevant for LLM context, not full execution details.
 */
export interface WorkflowVariableInfo {
  name: string;
  type?: string;
  description?: string;
}

/**
 * Workflow metadata, used by LLM to understand available workflows
 */
export interface WorkflowInfo {
  id: string;
  name: string;
  description: string;
  type: WorkflowTemplateType;
  variables?: WorkflowVariableInfo[];
  tags?: string[];
  category?: string;
}

/**
 * Configuration for workflow tool handlers
 *
 * Provides a loader interface that workflow tools use to:
 * - List available workflows (for LLM context)
 * - Check workflow existence (for validation)
 * - Load full workflow definition (on-demand)
 *
 * Mirrors the SkillHandlerConfig pattern for consistency.
 */
export interface WorkflowHandlerConfig {
  loader: {
    /** Get the list of all available workflows with their metadata. */
    getAvailableWorkflows: () => WorkflowInfo[];

    /** Check whether a workflow with the given ID exists. */
    hasWorkflow: (id: string) => boolean;

    /** Load the full workflow definition by ID. */
    loadDefinition: (id: string) => Promise<{ id: string; name: string; description?: string }>;
  };
}

/**
 * Format the list of available workflows into a human-readable string.
 */
export function formatAvailableWorkflows(workflows: WorkflowInfo[]): string {
  if (workflows.length === 0) {
    return "(no workflows available)";
  }
  return workflows
    .map(w => {
      const vars = w.variables?.length
        ? ` (inputs: ${w.variables.map(v => v.name).join(", ")})`
        : "";
      return `  - ${w.id}: ${w.description}${vars}`;
    })
    .join("\n");
}
