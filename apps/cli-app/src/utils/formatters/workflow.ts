/**
 * Workflow & Workflow Execution formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";

// Explicit format interfaces — declare exactly the fields each formatter needs.
// Any object structurally matching these fields is accepted; no runtime reflection required.
export interface FormattableWorkflow {
  id?: string;
  name?: string;
  type?: string;
  version?: string;
  description?: string;
  status?: string;
  createdAt?: string | number;
  updatedAt?: string | number;
  nodes?: unknown[];
  edges?: unknown[];
  triggers?: unknown[];
}

export interface FormattableExecution {
  id?: string;
  executionId?: string;
  workflowId?: string;
  status?: string;
  createdAt?: string | number;
}

// ============================================
// Workflow Formatters
// ============================================

export function formatWorkflow(workflow: FormattableWorkflow, options?: { verbose?: boolean }): string {
  const formatter = getFormatter();
  if (options?.verbose) {
    return formatWorkflowVerbose(workflow);
  }
  return formatter.workflow({
    id: workflow.id,
    name: workflow.name,
    status: workflow.status,
    type: workflow.type,
    triggers: workflow.triggers as Array<{ id: string; name?: string }> | undefined,
  });
}

function formatWorkflowVerbose(workflow: FormattableWorkflow): string {
  const lines: string[] = [];

  lines.push(`ID: ${workflow.id ?? "N/A"}`);
  lines.push(`Name: ${workflow.name ?? "Unnamed"}`);

  if (workflow.type !== undefined) {
    lines.push(`Type: ${String(workflow.type)}`);
  }

  if (workflow.version !== undefined) {
    lines.push(`Version: ${String(workflow.version)}`);
  }

  if (workflow.description !== undefined && workflow.description !== "") {
    lines.push(`Description: ${String(workflow.description)}`);
  }

  lines.push(`Status: ${workflow.status ?? "unknown"}`);
  lines.push(`Creation time: ${String(workflow.createdAt ?? "N/A")}`);

  if (workflow.updatedAt !== undefined && workflow.updatedAt !== "") {
    lines.push(`Update time: ${String(workflow.updatedAt)}`);
  }

  if (Array.isArray(workflow.nodes)) {
    lines.push(`Number of nodes: ${workflow.nodes.length}`);
  }

  if (Array.isArray(workflow.edges)) {
    lines.push(`Number of edges: ${workflow.edges.length}`);
  }

  if (Array.isArray(workflow.triggers)) {
    lines.push(`Number of triggers: ${workflow.triggers.length}`);
  }

  return lines.join("\n");
}

export function formatWorkflowList(
  workflows: FormattableWorkflow[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  const formatter = getFormatter();
  if (workflows.length === 0) {
    return emptyMsg("workflows");
  }

  if (options?.table) {
    const headers = ["ID", "Name", "Type", "Version", "Status", "Creation time"];
    const rows = workflows.map(w => [
      shortId(w.id),
      w.name || "unnamed",
      w.type ?? "",
      w.version ?? "",
      w.status || "unknown",
      String(w.createdAt || "N/A"),
    ]);
    return formatter.table(headers, rows);
  }

  return workflows.map(w => formatWorkflow(w, options)).join("\n");
}

// ============================================
// Workflow Execution Formatters
// ============================================

export function formatWorkflowExecution(execution: FormattableExecution, options?: { verbose?: boolean }): string {
  return formatWith(execution, options, (formatter) => {
    return formatter.workflowExecution(execution);
  });
}

export function formatWorkflowExecutionList(
  executions: FormattableExecution[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  const formatter = getFormatter();
  if (executions.length === 0) {
    return emptyMsg("executions");
  }

  if (options?.table) {
    const headers = ["Execution ID", "Workflow ID", "Status", "Creation time"];
    const rows = executions.map(e => [
      shortId(e.executionId ?? e.id),
      shortId(e.workflowId),
      e.status || "unknown",
      String(e.createdAt || "N/A"),
    ]);
    return formatter.table(headers, rows);
  }

  return executions.map(e => formatWorkflowExecution(e, options)).join("\n");
}