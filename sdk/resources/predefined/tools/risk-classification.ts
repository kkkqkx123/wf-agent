/**
 * Tool Risk Classification
 * Defines risk levels for all predefined tools
 */

import type { ToolRiskLevel } from "@wf-agent/types";

/**
 * Predefined tool risk classification
 * This mapping defines the risk level for each predefined tool
 *
 * Risk Levels:
 * - READ_ONLY: Safe to auto-approve by default
 * - WRITE: Requires explicit configuration
 * - EXECUTE: Requires command whitelist
 * - MCP: Independent category with fine-grained control
 * - NETWORK: Requires domain whitelist
 * - SYSTEM: Never auto-approves
 * - INTERACTION: Special handling (timeout auto-response)
 */
export const TOOL_RISK_CLASSIFICATION: Record<string, ToolRiskLevel> = {
  // === STATELESS - Filesystem (Read) ===
  read_file: "READ_ONLY",
  list_files: "READ_ONLY",
  grep: "READ_ONLY",

  // === STATELESS - Filesystem (Write) ===
  write_file: "WRITE",
  edit: "WRITE",
  apply_diff: "WRITE",
  apply_patch: "WRITE",

  // === STATELESS - Shell ===
  run_shell: "EXECUTE",

  // === STATELESS - Interaction ===
  ask_followup_question: "INTERACTION",
  run_slash_command: "EXECUTE",
  skill: "READ_ONLY", // Loading instructions is safe
  update_todo_list: "READ_ONLY", // Todo update is safe

  // === STATELESS - MCP (Independent Category) ===
  use_mcp: "MCP", // MCP protocol calls (stdio, etc.)

  // === STATEFUL - Memory ===
  record_note: "WRITE",
  recall_notes: "READ_ONLY",

  // === STATEFUL - Shell ===
  backend_shell: "EXECUTE",
  shell_output: "READ_ONLY",
  shell_kill: "EXECUTE",

  // === BUILTIN - Workflow ===
  execute_workflow: "SYSTEM",
  cancel_workflow: "SYSTEM",
  query_workflow_status: "READ_ONLY",
};

/**
 * Get risk level for a tool
 * @param toolId Tool ID
 * @returns Risk level, defaults to WRITE if not found
 */
export function getToolRiskLevel(toolId: string): ToolRiskLevel {
  return TOOL_RISK_CLASSIFICATION[toolId] ?? "WRITE";
}

/**
 * Check if a tool has a known risk level
 * @param toolId Tool ID
 * @returns True if the tool is in the classification map
 */
export function hasKnownRiskLevel(toolId: string): boolean {
  return toolId in TOOL_RISK_CLASSIFICATION;
}

/**
 * Get all tools with a specific risk level
 * @param riskLevel The risk level to filter by
 * @returns Array of tool IDs with the specified risk level
 */
export function getToolsByRiskLevel(riskLevel: ToolRiskLevel): string[] {
  return Object.entries(TOOL_RISK_CLASSIFICATION)
    .filter(([, level]) => level === riskLevel)
    .map(([toolId]) => toolId);
}

/**
 * Get risk level statistics
 * @returns Object with count of tools per risk level
 */
export function getRiskLevelStats(): Record<ToolRiskLevel, number> {
  const stats: Record<ToolRiskLevel, number> = {
    READ_ONLY: 0,
    WRITE: 0,
    EXECUTE: 0,
    MCP: 0,
    NETWORK: 0,
    SYSTEM: 0,
    INTERACTION: 0,
  };

  for (const level of Object.values(TOOL_RISK_CLASSIFICATION)) {
    stats[level]++;
  }

  return stats;
}
