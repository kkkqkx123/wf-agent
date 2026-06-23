/**
 * Workflows Fragment Generator
 *
 * Generates a dynamic context section listing available workflows
 * for LLM discovery. Mirrors the skills.ts pattern.
 */

import { wrapSection } from "./utils.js";

/**
 * Generate Workflows content for dynamic context injection.
 *
 * Accepts unknown[] and performs runtime type checking for safety,
 * matching the pattern used by generateSkillsContent.
 *
 * @param workflows - Array of workflow info objects (id, name, description)
 * @returns Formatted section string or empty string
 */
export function generateWorkflowsContent(workflows?: unknown): string {
  if (!workflows || !Array.isArray(workflows) || workflows.length === 0) {
    return "";
  }

  const lines: string[] = ["Available workflows:"];
  for (const wf of workflows) {
    if (wf && typeof wf === "object") {
      const w = wf as { id?: string; name?: string; description?: string };
      if (w.id) {
        const desc = w.description || w.name || "No description";
        lines.push(`  - ${w.id}: ${desc}`);
      }
    }
  }

  if (lines.length === 1) {
    // Only the header, no valid workflows found
    return "";
  }

  return wrapSection("AVAILABLE WORKFLOWS", lines.join("\n"));
}
