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
 * @param config Dynamic context configuration (controls which sections are included)
 * @returns System context prompt string
 */
export async function buildSystemContextPrompt(
  config?: DynamicContextConfig,
): Promise<string> {
  const sections: string[] = [];

  // 1. Add current time (disabled by default, see DynamicContextConfig)
  if (config?.includeCurrentTime === true) {
    sections.push(generateCurrentTimeSection());
  }

  // 2. Add environment information (disabled by default, see DynamicContextConfig)
  if (config?.includeEnvironmentInfo === true) {
    const envInfo = getDefaultEnvironmentInfo();
    const envSection = generateEnvironmentSection(envInfo);
    if (envSection) {
      sections.push(envSection);
    }
  }

  // 3. Custom sections
  if (config?.customSections) {
    for (const value of Object.values(config.customSections)) {
      sections.push(value);
    }
  }

  // Combine all sections
  const combinedPrompt = sections.filter(Boolean).join("\n\n");
  return cleanupEmptyLines(combinedPrompt);
}
