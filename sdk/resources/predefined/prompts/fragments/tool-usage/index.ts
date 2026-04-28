/**
 * Tool Usage Guidelines: Unified Export of Segments
 */

export { TOOL_USAGE_XML_SUMMARY_FRAGMENT, TOOL_USAGE_JSON_SUMMARY_FRAGMENT } from "./summary.js";

/**
 * Tool list description placeholder
 *
 * This will be dynamically replaced with descriptions of specific tools when in use.
 */
export const TOOL_LIST_PLACEHOLDER = "{{TOOL_LIST_DESCRIPTION}}";

/**
 * Build a complete tool usage guide that includes a list of tools
 *
 * @param summaryFragmentId Summary fragment ID
 * @param toolListDescription Dynamically generated description of the tool list
 * @returns Complete tool usage guide
 */
export function buildToolUsageWithList(
  summaryFragmentId: string,
  toolListDescription: string,
): string {
  // This will be implemented in composer.ts, obtaining fragments through the registry and combining them
  return `${TOOL_LIST_PLACEHOLDER}\n\n${toolListDescription}`;
}
