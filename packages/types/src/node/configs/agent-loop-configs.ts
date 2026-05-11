/**
 * Agent Loop Node Configuration Type Definition
 *
 * Design principle: Bridge configuration from workflow to agent execution
 * Agent Loop node acts as a bridge between workflow graph and standalone agent execution.
 *
 * This configuration is a simplified, declarative subset of AgentLoopRuntimeConfig,
 * suitable for workflow node definition (serializable to TOML/JSON).
 */

import type { ID } from "../../common.js";
import type { AvailableTools } from "../../available-tools.js";

/**
 * Agent Loop Node Configuration
 *
 * Agent Loop node embeds an agent execution engine within a workflow graph.
 * It bridges workflow context to agent runtime by providing:
 * - Reference to predefined agent configuration (agentLoopId), OR
 * - Inline configuration for simple scenarios
 *
 * For complex agent behaviors (hooks, transforms, custom logic), define a full
 * AgentLoopDefinition in config files and reference it via agentLoopId.
 *
 * @see AgentLoopDefinition - Full static agent configuration
 * @see AgentLoopRuntimeConfig - Runtime configuration with callbacks
 */
export interface AgentLoopNodeConfig {
  /**
   * Reference to predefined Agent Loop configuration ID
   * 
   * When provided, loads the complete agent configuration from the agent registry.
   * This is the recommended approach for complex agent setups with hooks,
   * custom transforms, or reusable configurations.
   * 
   * Priority: Higher than inlineConfig fields when both are provided.
   */
  agentLoopId?: ID;

  /**
   * Inline agent configuration (simplified subset)
   * 
   * For simple scenarios where you don't need a separate agent config file.
   * Provides basic agent parameters directly in the workflow node.
   * 
   * Note: This is a declarative subset. For advanced features (hooks, transforms),
   * use agentLoopId to reference a full AgentLoopDefinition.
   */
  inlineConfig?: {
    /** LLM Profile ID (required if using inlineConfig) */
    profileId: string;

    /**
     * Maximum number of iterations
     * @default 20
     * @description Force termination after this number of iterations, with or without tool calls.
     */
    maxIterations?: number;

    /**
     * Available tools configuration
     * @description Specifies which tools are available during agent loop execution.
     * If not specified, uses all available tools in the context.
     */
    availableTools?: AvailableTools;

    /**
     * System prompt (direct string, lower priority than systemPromptTemplateId)
     * @description Injected at each iteration of the agent loop
     */
    systemPrompt?: string;

    /**
     * System prompt template ID (references a predefined template, higher priority)
     * @description Renders the system prompt via the template registry
     */
    systemPromptTemplateId?: string;

    /**
     * System prompt template variables (required when systemPromptTemplateId is used)
     */
    systemPromptTemplateVariables?: Record<string, unknown>;
  };
}

/**
 * Agent Loop node execution data
 * Used to pass data through the node processor
 * 
 * @deprecated This type is no longer needed. The handler directly uses AgentLoopRuntimeConfig
 * constructed from AgentLoopNodeConfig. Kept for backward compatibility only.
 */
export interface AgentLoopExecutionData {
  /** Node ID */
  nodeId: ID;
  /** Execution ID */
  executionId: ID;
  /** Input prompt (from workflow variables) */
  prompt?: string;
  /** Node Configuration */
  config: AgentLoopNodeConfig;
}
