/**
 * Available Tools Configuration
 * 
 * Specifies which tools are available during execution.
 * Supports static initial tools and dynamic additions during execution.
 * 
 * @example
 * ```typescript
 * // Basic usage - just initial tools
 * const config: AvailableTools = {
 *   initial: ["read_file", "write_file"]
 * };
 * 
 * // Advanced usage with filtering
 * const config: AvailableTools = {
 *   initial: ["read_file", "write_file", "run_shell"],
 *   filterMode: "allowlist",
 *   allowList: ["read_file", "write_file"]
 * };
 * ```
 */

/**
 * Tool filtering mode
 * - 'none': No filtering, all tools in initial set are available
 * - 'allowlist': Only tools in allowList are available (intersection with initial)
 * - 'blocklist': All tools except those in blockList are available
 */
export type ToolFilterMode = 'none' | 'allowlist' | 'blocklist';

/**
 * Available Tools Configuration
 * 
 * Used by both WorkflowTemplate and AgentLoopRuntimeConfig to specify
 * which tools are available during execution.
 */
export interface AvailableTools {
  /**
   * Initial set of available tools (tool IDs or names)
   * 
   * These tools are available from the start of execution.
   * Can be extended dynamically during execution via ADD_TOOL nodes (workflow)
   * or addTools() method (agent loop).
   */
  initial: string[];
  
  /**
   * Dynamic tools added during execution
   * 
   * This set is populated when tools are added dynamically.
   * Managed internally by the execution engine.
   * Not typically set in configuration files.
   */
  dynamic?: Set<string>;
  
  /**
   * Tool filtering mode
   * 
   * Controls how the available tools are filtered:
   * - 'none' (default): All tools in initial set are available
   * - 'allowlist': Only tools explicitly listed in allowList are available
   * - 'blocklist': All tools except those in blockList are available
   * 
   * @default 'none'
   */
  filterMode?: ToolFilterMode;
  
  /**
   * Explicit allowlist of tool IDs
   * 
   * When filterMode is 'allowlist', only tools in this list
   * (that are also in the initial set) will be available.
   * 
   * Useful for restricting tool access even when many tools are registered.
   */
  allowList?: string[];
  
  /**
   * Explicit blocklist of tool IDs
   * 
   * When filterMode is 'blocklist', all tools except those in this list
   * will be available.
   * 
   * Useful for excluding specific dangerous or unwanted tools.
   */
  blockList?: string[];
}

/**
 * Check if a tool is available based on AvailableTools config
 * 
 * @param availableTools - The available tools configuration
 * @param toolId - The tool ID to check
 * @returns true if the tool is available, false otherwise
 */
export function isToolAvailable(availableTools: AvailableTools | undefined, toolId: string): boolean {
  if (!availableTools) {
    return false;
  }
  
  const { initial, filterMode = 'none', allowList, blockList } = availableTools;
  
  // Check if tool is in initial set
  const isInInitial = initial.includes(toolId);
  if (!isInInitial) {
    return false;
  }
  
  // Apply filtering based on mode
  switch (filterMode) {
    case 'allowlist':
      return allowList ? allowList.includes(toolId) : true;
    case 'blocklist':
      return blockList ? !blockList.includes(toolId) : true;
    case 'none':
    default:
      return true;
  }
}
