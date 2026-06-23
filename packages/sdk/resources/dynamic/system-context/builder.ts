/**
 * System Context Builder
 *
 * Builds stable system context that rarely changes during execution:
 * - Current time and timezone
 * - Environment information (OS, workspaces, language)
 *
 * These fragments are injected into the system message and enable
 * KV cache hits because they change infrequently.
 */

import type { DynamicContextConfig } from "@wf-agent/types";
import { generateCurrentTimeSection } from "./fragments/current-time.js";
import { generateEnvironmentSection, getDefaultEnvironmentInfo } from "./fragments/environment.js";
import { cleanupEmptyLines } from "./fragments/utils.js";

/**
 * Build system context prompt
 *
 * Generates stable content for system message that can be cached.
 * Includes: time, environment information.
 *
 * Note: Tool documentation should be injected by the application layer
 * when tools are available, as it's configuration-dependent.
 *
 * @param _config Dynamic context configuration (reserved for future use)
 * @returns System context prompt string
 */
export async function buildSystemContextPrompt(
  _config?: DynamicContextConfig,
): Promise<string> {
  const sections: string[] = [];

  // 1. Add current time (always stable)
  sections.push(generateCurrentTimeSection());

  // 2. Add environment information (mostly static)
  const envInfo = getDefaultEnvironmentInfo();
  const envSection = generateEnvironmentSection(envInfo);
  if (envSection) {
    sections.push(envSection);
  }

  // Combine all sections
  const combinedPrompt = sections.filter(Boolean).join("\n\n");
  return cleanupEmptyLines(combinedPrompt);
}
