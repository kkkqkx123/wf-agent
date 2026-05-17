/**
 * Available Tools Configuration (Workflow Static Definition)
 * 
 * Specifies the complete set of tools that can be used during workflow execution,
 * with support for initial availability, dynamic visibility control, and approval requirements.
 * 
 * This configuration is designed SPECIFICALLY for workflows that need:
 * - TOOL_VISIBILITY nodes for runtime tool management
 * - Initial vs. dynamic tool enablement
 * - Scope-based tool isolation (execution/subgraph)
 * 
 * ⚠️ NOT suitable for Agent Loop - use AgentToolConfig instead
 * 
 * Key Concepts:
 * - 'available': The complete pool of tools that CAN be used (forms the fixed schema)
 * - 'initial': Subset of available tools that are enabled at startup
 * - 'allowList': Whitelist of tools that do NOT require approval (unconditional access)
 * - 'requireApproval': Tools that require approval before execution
 * 
 * Constraints:
 * - initial ⊆ available
 * - allowList ⊆ available
 * - requireApproval ⊆ available
 * - allowList ∩ requireApproval = ∅ (a tool cannot be both allowed and approval-required)
 * - Tools NOT in available are completely excluded from the system
 * 
 * @example
 * ```typescript
 * // Basic usage - all tools available initially, no approval needed
 * const config: AvailableTools = {
 *   available: ["read_file", "write_file", "edit_file"]
 * };
 * 
 * // Advanced usage with phased enablement and approval
 * const config: AvailableTools = {
 *   available: ["read_file", "write_file", "edit_file", "delete_file", "execute_shell"],
 *   initial: ["read_file"],  // Start with read-only
 *   allowList: ["read_file", "write_file", "edit_file"],  // These don't need approval
 *   requireApproval: ["delete_file", "execute_shell"]  // Dangerous ops need approval
 * };
 * ```
 */

/**
 * Available Tools Configuration (Workflow-Specific)
 * 
 * Used by WorkflowTemplate to specify which tools are available during execution.
 * Supports dynamic tool visibility management via TOOL_VISIBILITY nodes.
 * 
 * ⚠️ For Agent Loop, use AgentToolConfig instead
 */
export interface AvailableTools {
  /**
   * Complete set of available tools (tool IDs)
   * 
   * This defines the pool of tools that can be dynamically enabled/disabled
   * during execution. The LLM tools schema is built from this set.
   * 
   * Tools NOT listed here are completely excluded from the system.
   */
  available: string[];
  
  /**
   * Initial set of enabled tools (subset of 'available')
   * 
   * These tools are enabled at startup and can be called immediately.
   * Other tools in 'available' can be enabled later via TOOL_VISIBILITY nodes.
   * 
   * If omitted or empty, all tools in 'available' are enabled initially.
   * 
   * Constraint: initial ⊆ available
   */
  initial?: string[];
  
  /**
   * Allowlist: Tools that do NOT require approval (unconditional access)
   * 
   * When specified, tools in this list can be called immediately without
   * any approval workflow. Tools in 'available' but NOT in 'allowList'
   * will require approval if they are also in 'requireApproval', or will
   * be blocked if not in either list.
   * 
   * Use case: Safe operations that should execute without user intervention.
   * 
   * Constraint: allowList ⊆ available
   * Constraint: allowList ∩ requireApproval = ∅
   */
  allowList?: string[];
  
  /**
   * Tools that require approval before execution
   * 
   * These tools appear in the LLM schema and can be called, but their execution
   * is intercepted and requires explicit approval (via approval workflow or
   * manual confirmation).
   * 
   * Use case: Dangerous operations like shell commands, file deletion, etc.
   * 
   * Constraint: requireApproval ⊆ available
   */
  requireApproval?: string[];
}

/**
 * Validate and normalize AvailableTools configuration
 * 
 * Ensures all constraints are met:
 * - initial ⊆ available
 * - allowList ⊆ available
 * - requireApproval ⊆ available
 * - allowList ∩ requireApproval = ∅ (no overlap)
 */
export function validateAvailableTools(config: AvailableTools): void {
  const { available, initial, allowList, requireApproval } = config;
  
  // Check initial ⊆ available
  if (initial) {
    for (const toolId of initial) {
      if (!available.includes(toolId)) {
        throw new Error(`Tool '${toolId}' in 'initial' is not in 'available'`);
      }
    }
  }
  
  // Check allowList ⊆ available
  if (allowList) {
    for (const toolId of allowList) {
      if (!available.includes(toolId)) {
        throw new Error(`Tool '${toolId}' in 'allowList' is not in 'available'`);
      }
    }
  }
  
  // Check requireApproval ⊆ available
  if (requireApproval) {
    for (const toolId of requireApproval) {
      if (!available.includes(toolId)) {
        throw new Error(`Tool '${toolId}' in 'requireApproval' is not in 'available'`);
      }
    }
  }
  
  // CRITICAL: Check allowList ∩ requireApproval = ∅
  if (allowList && requireApproval) {
    const allowSet = new Set(allowList);
    for (const toolId of requireApproval) {
      if (allowSet.has(toolId)) {
        throw new Error(
          `Tool '${toolId}' cannot be in both 'allowList' and 'requireApproval'. ` +
          `A tool must be either unconditionally allowed OR require approval, not both.`
        );
      }
    }
  }
}

/**
 * Resolve the effective set of tools for schema generation
 * 
 * Returns: available ∩ allowList (if allowList specified) OR available
 */
export function resolveSchemaTools(config: AvailableTools): string[] {
  const { available, allowList } = config;
  
  if (allowList) {
    // Intersection: only tools in both available and allowList
    return available.filter(id => allowList.includes(id));
  }
  
  return available;
}

/**
 * Resolve initial enabled tools
 * 
 * Returns: initial (if specified) OR all schema tools
 */
export function resolveInitialTools(config: AvailableTools): string[] {
  const schemaTools = resolveSchemaTools(config);
  
  if (config.initial && config.initial.length > 0) {
    return config.initial;
  }
  
  // If initial not specified, enable all schema tools
  return schemaTools;
}

/**
 * Check if a tool requires approval
 */
export function requiresApproval(
  config: AvailableTools | undefined,
  toolId: string
): boolean {
  return config?.requireApproval?.includes(toolId) || false;
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
  
  const schemaTools = resolveSchemaTools(availableTools);
  return schemaTools.includes(toolId);
}
