/**
 * Skills Fragment Generator
 */

import { wrapSection } from "./utils.js";

/**
 * Generate Skills content
 */
export function generateSkillsContent(skills?: unknown): string {
  if (!skills || !Array.isArray(skills) || skills.length === 0) {
    return "";
  }

  const lines: string[] = ["Active skills:"];
  for (const skill of skills) {
    if (skill && typeof skill === "object" && (skill as { name?: string }).name) {
      const s = skill as { name: string; description?: string };
      lines.push(`  - ${s.name}: ${s.description || "No description"}`);
    }
  }

  return wrapSection("ACTIVE SKILLS", lines.join("\n"));
}
