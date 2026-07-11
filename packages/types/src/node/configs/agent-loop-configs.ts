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
import type { AgentToolConfig } from "../../agent/tool-config.js";
import type { WorkflowDataInput, WorkflowMessageInput, WorkflowMessageOutput } from "../../workflow/boundary-config.js";

/**
 * Agent Loop Node Output
 * - finalResponse?: string - The final response from the agent
 * - toolCallCount: number - Number of tool calls made during the loop
 * - iterationCount: number - Number of iterations executed
 */
export interface AgentLoopNodeOutput {
  finalResponse?: string;
  toolCallCount: number;
  iterationCount: number;
}

/**
 * Agent Loop Node Configuration
 *
 * Agent Loop node embeds an agent execution engine within a workflow graph.
 * It bridges workflow context to agent runtime by providing:
 * - Reference to predefined agent configuration (agentLoopId), with optional
 *   inline overrides for specific fields, OR
 * - Fully inline configuration for simple scenarios
 *
 * ## Merge Semantics (agentLoopId + inlineConfig)
 *
 * When both agentLoopId and inlineConfig are provided, the inlineConfig fields
 * serve as **selective overrides** on top of the static definition loaded from
 * agentLoopId. This allows reusing a base agent configuration while customizing
 * specific aspects for a particular workflow node.
 *
 * | Scenario | agentLoopId | inlineConfig |
 * |----------|------------|--------------|
 * | Full static reference | ✅ required | ❌ omitted |
 * | Static + overrides | ✅ required | ✅ optional fields |
 * | Fully inline | ❌ omitted | ✅ required fields |
 *
 * Overridable fields: profileId, maxIterations, availableTools, dataInputs,
 * messageInputs, messageOutputs
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
    /**
     * LLM Profile ID
     *
     * Required when inlineConfig is used without agentLoopId.
     * Optional when agentLoopId is provided — in that case, the agent definition's
     * profileId is used as the base and this field overrides it.
     */
    profileId?: string;

    /**
     * System prompt for the agent loop
     *
     * When agentLoopId is provided, this overrides the template's default system prompt.
     * When used standalone (no agentLoopId), this is required.
     */
    systemPrompt?: string;

    /**
     * Maximum number of iterations
     * @default 20
     * @description Force termination after this number of iterations, with or without tool calls.
     */
    maxIterations?: number;

    /**
     * Maximum number of automatic retries on failure
     * @default 0 (no automatic retry)
     */
    maxRetries?: number;

    /**
     * Delay between retries in milliseconds
     * @default 1000
     */
    retryDelay?: number;

    /**
     * Whether to use exponential backoff for retry delays
     * @default true
     */
    exponentialBackoff?: boolean;

    /**
     * Interval for periodic checkpoint creation in milliseconds.
     * When set, a checkpoint is created at this interval during long-running execution.
     * @default 0 (no periodic checkpoint)
     */
    checkpointIntervalMs?: number;

    /**
     * Available tools configuration
     * @description Specifies which tools are available during agent loop execution.
     * If not specified, uses all available tools in the context.
     */
    availableTools?: AgentToolConfig;

    /**
     * Working context ID for Agent Loop internal operations
     *
     * Defaults to 'current' if not specified.
     */
    workingContext?: string;

    /**
     * Data inputs - maps fields from the workflow execution input to internal variables.
     *
     * This enables explicit data passing from the workflow's execution input data
     * into the agent loop's variable system, similar to how START_FROM_TRIGGER
     * processes dataInputs.
     *
     * Example:
     *   Workflow execution input: { query: "hello", userId: "abc" }
     *   dataInputs: [{ parentField: "query", internalName: "query_text" }]
     *   Result: variable "query_text" gets "hello"
     */
    dataInputs?: WorkflowDataInput[];

    /**
     * Message context inputs - explicitly maps named message contexts from
     * the workflow's MessageContextRegistry to the agent loop's initial messages.
     *
     * Each entry specifies a source context (externalName) in the workflow registry
     * and an internal name for the agent loop. The messages from all specified
     * contexts are concatenated as initial messages for the agent loop execution.
     *
     * Example:
     *   messageInputs: [
     *     { externalName: "system-context", internalName: "system" },
     *     { externalName: "conversation", internalName: "chat", required: true }
     *   ]
     */
    messageInputs?: WorkflowMessageInput[];

    /**
     * Message context outputs - explicitly maps the agent loop's accumulated
     * messages back to named contexts in the workflow's MessageContextRegistry.
     *
     * After the agent loop completes execution, the messages from the agent loop's
     * conversation manager are copied to the workflow registry under the specified
     * external names. This ensures the workflow retains the conversation state
     * for subsequent nodes or downstream processing.
     *
     * Example:
     *   messageOutputs: [
     *     { internalName: "agent-chat", externalName: "updated-conversation" }
     *   ]
     *   Result: workflow registry gets context "updated-conversation" with all messages
     */
    messageOutputs?: WorkflowMessageOutput[];
  };
}