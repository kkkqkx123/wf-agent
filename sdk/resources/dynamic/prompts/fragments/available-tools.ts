/**
 * Available Tools Fragment Generator
 * Dynamically generates tool list descriptions based on available tools
 */

import type { Tool } from "@wf-agent/types";
import { wrapSection } from "./utils.js";
import {
  generateToolListDescription,
  generateToolAvailabilitySection,
  type ToolDescriptionFormat,
} from "../../../../core/utils/tools/tool-description-generator.js";

export type { ToolDescriptionFormat };

/**
 * Available Tools Configuration
 */
export interface AvailableToolsConfig {
  /** Array of tools to describe */
  tools: Tool[];
  /** Description format */
  format?: ToolDescriptionFormat;
  /** Whether to include usage rules */
  includeRules?: boolean;
  /** Group tools by category */
  groupByCategory?: boolean;
}

/**
 * Generate available tools list content
 * @param config Tool configuration
 * @returns Formatted content string or null if no tools
 */
export function generateAvailableToolsContent(config: AvailableToolsConfig): string | null {
  const { tools, format = "detailed", includeRules = false, groupByCategory = true } = config;

  if (!tools || tools.length === 0) {
    return null;
  }

  // Use the generator for complete tool descriptions
  const toolDescriptions = includeRules
    ? generateToolAvailabilitySection(tools, format)
    : generateToolListDescription(tools, format, { groupByCategory });

  if (toolDescriptions.length === 0) {
    return null;
  }

  return wrapSection("AVAILABLE TOOLS", toolDescriptions);
}

/**
 * Generate tool description message for LLM
 * @param tools Array of tools
 * @param format Description format (default: 'detailed')
 * @returns LLM message object or null if no tools
 */
export function generateToolDescriptionMessage(
  tools: Tool[],
  format: ToolDescriptionFormat = "detailed",
): { role: "system"; content: string } | null {
  const content = generateAvailableToolsContent({ tools, format, includeRules: true });

  if (!content) {
    return null;
  }

  return {
    role: "system",
    content,
  };
}

/**
 * Generate compact tool list for limited context scenarios
 * @param tools Array of tools
 * @returns Formatted content string or null if no tools
 */
export function generateCompactToolsContent(tools: Tool[]): string | null {
  return generateAvailableToolsContent({
    tools,
    format: "compact",
    groupByCategory: false,
  });
}

/**
 * Generate detailed tool documentation for system prompts
 * @param tools Array of tools
 * @returns Complete tool documentation with usage rules
 */
export function generateToolDocumentation(tools: Tool[]): string | null {
  return generateAvailableToolsContent({
    tools,
    format: "detailed",
    includeRules: true,
    groupByCategory: true,
  });
}
