/**
 * Skills Fragment Generator
 */

import type { SkillConfigItem } from "@wf-agent/types";
import { wrapSection } from "./utils.js";

/**
 * Generate Skills content for dynamic context injection.
 *
 * @param skills - Array of SkillConfigItem objects from DynamicRuntimeContext
 * @returns Formatted section string or empty string
 */
export function generateSkillsContent(skills?: SkillConfigItem[]): string {
  if (!skills || skills.length === 0) {
    return "";
  }

  const lines: string[] = ["Active skills:"];
  for (const skill of skills) {
    if (skill.name) {
      lines.push(`  - ${skill.name}: ${skill.description || "No description"}`);
    }
  }

  if (lines.length === 1) {
    // Only the header, no valid skills found
    return "";
  }

  return wrapSection("ACTIVE SKILLS", lines.join("\n"));
}
