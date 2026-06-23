/**
 * Agent Tool Configuration (Static Definition)
 * 
 * Simple configuration for Agent Loop tool access control.
 * Agent does NOT need complex visibility management (initial/dynamic/TOOL_VISIBILITY).
 * 
 * Key Concepts:
 * - All tools listed are available to the agent
 * - 'requireApproval': Subset of tools that need explicit approval before execution
 * - Tools NOT in requireApproval can be called directly
 * 
 * Design Principles:
 * 1. Simplicity: Single list of available tools + optional approval subset
 * 2. Explicit: Clear declaration of which tools need approval
 * 3. Safe-by-default: Only listed tools are accessible
 * 
 * @example
 * ```typescript
 * // Basic usage - all tools available, some require approval
 * const config: AgentToolConfig = {
 *   tools: ["read_file", "write_file", "search_files", "execute_command"],
 *   requireApproval: ["write_file", "execute_command"]  // These need user confirmation
 * };
 * 
 * // All tools allowed without approval
 * const permissiveConfig: AgentToolConfig = {
 *   tools: ["read_file", "write_file", "execute_command"]
 *   // No requireApproval means all tools execute immediately
 * };
 * 
 * // All tools require approval (most restrictive)
 * const restrictiveConfig: AgentToolConfig = {
 *   tools: ["read_file", "write_file"],
 *   requireApproval: ["read_file", "write_file"]  // Everything needs approval
 * };
 * ```
 */

export interface AgentToolConfig {
  /**
   * Complete list of tools available to the agent
   * 
   * These tools will be included in the LLM schema and can be called by the agent.
   * This is the ONLY source of truth for available tools - no dynamic additions.
   */
  tools: string[];
  
  /**
   * Subset of tools that require explicit approval before execution
   * 
   * These tools appear in the LLM schema but their execution is intercepted
   * and requires user confirmation (via approval workflow or manual UI).
   * 
   * Constraint: requireApproval ⊆ tools
   * 
   * Use case: Dangerous operations like file writes, shell commands, etc.
   */
  requireApproval?: string[];

  /**
   * Workflow IDs that this agent is allowed to execute via execute_workflow tool.
   * 
   * When set, only these workflows are visible to the LLM and can be executed.
   * Workflow metadata (id, description, input schema) is injected into system prompt
   * so the LLM knows what workflows are available.
   * 
   * When omitted or empty, no workflows are available (execute_workflow will fail).
   * Set to ['*'] to allow all workflows registered in the system.
   * 
   * @example ['data-analysis', 'report-generator']
   */
  allowedWorkflows?: string[];
}

/**
 * Validate AgentToolConfig
 * 
 * Ensures:
 * 1. requireApproval ⊆ tools
 * 2. No duplicate entries
 */
export function validateAgentToolConfig(config: AgentToolConfig): void {
  const { tools, requireApproval = [] } = config;
  
  const toolSet = new Set(tools);
  
  // Check that requireApproval is a subset of tools
  for (const toolId of requireApproval) {
    if (!toolSet.has(toolId)) {
      throw new Error(
        `Tool '${toolId}' in requireApproval is not in the tools list`
      );
    }
  }
  
  // Check for duplicates in tools
  if (tools.length !== toolSet.size) {
    throw new Error('Duplicate entries found in tools list');
  }
}

/**
 * Check if a tool requires approval
 */
export function doesToolRequireApproval(
  config: AgentToolConfig | undefined,
  toolId: string
): boolean {
  return config?.requireApproval?.includes(toolId) || false;
}

/**
 * Check if a tool is available (in the tools list)
 */
export function isAgentToolAvailable(
  config: AgentToolConfig | undefined,
  toolId: string
): boolean {
  return config?.tools?.includes(toolId) || false;
}

/**
 * Get all available tools
 */
export function getAvailableTools(config: AgentToolConfig | undefined): string[] {
  return config?.tools || [];
}
