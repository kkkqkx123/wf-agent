/**
 * Performs node configuration type definition
 * Contains CODE, LLM, and TOOL node configurations
 */

import type { ID } from '../../common.js';
import { ScriptRiskLevel } from '../../script/script-security.js';
import type { ScriptExecutorConfig } from '../../script/script-executor.js';

/**
 * Script Node Output
 * - result: unknown - The raw script execution return value
 */
export interface ScriptNodeOutput {
  result: unknown;
}

/**
 * Code Node Configuration
 * For standard SCRIPT nodes - stateless, one-off execution
 * Supports: direct/shared executor, template rendering, flow orchestration
 */
export interface ScriptNodeConfig {
  /** screenplay title */
  scriptName: string;
  /** Risk level [different execution policies will be implemented in the application layer, e.g. none not checked, HIGH runs in the sandbox] */
  risk: ScriptRiskLevel;
  /** Whether it is an inline code */
  inline?: boolean;
  /** Inline command template (alternative to scriptName, uses {{var}} placeholders) */
  template?: string;
  /** Executor configuration override */
  executor?: ScriptExecutorConfig;
  /** Flow blueprint ID reference (delegates to ScriptFlowEngine) */
  flowId?: string;
  /** Argument overrides for template rendering */
  arguments?: Record<string, unknown>;
}

/**
 * Interactive Script Node Output
 * - result: unknown - The raw script execution return value
 */
export interface InteractiveScriptNodeOutput {
  result: unknown;
}

/**
 * Interactive Script Node Configuration
 * For INTERACTIVE_SCRIPT nodes that require runtime user/LLM input
 * Uses pty executor by default for interactive session support
 */
export interface InteractiveScriptNodeConfig {
  /** Script name (references a registered Script in ScriptRegistry) */
  scriptName: string;
  /** Risk level */
  risk: ScriptRiskLevel;
  /** Executor configuration override (defaults to pty for interactive scripts) */
  executor?: ScriptExecutorConfig;
  /** Flow blueprint ID reference for interactive flow execution */
  flowId?: string;
  /** Interaction mode (blocking/llm-assisted/hybrid) */
  interactionMode?: import("../../script/script-interactive.js").InteractionMode;
  /** Prompt patterns to detect (regex strings indicating script is waiting for input) */
  promptPatterns?: string[];
  /** Maximum interaction rounds */
  maxRounds?: number;
  /** Timeout per interaction round (milliseconds) */
  roundTimeout?: number;
}

/**
 * LLM Node Output
 * - content: string - The LLM response text content
 * - toolCalls?: Array<{ id: string, name: string, arguments: unknown }> - Any tool calls made by the LLM
 */
export interface LLMNodeOutput {
  content: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: unknown;
  }>;
}

/**
 * LLM Node Configuration
 *
 * Simplified configuration using a single named message context.
 * Replaces the complex template system with direct context reference.
 */
export interface LLMNodeConfig {
  /** Referenced LLM Profile ID */
  profileId: ID;

  /**
   * Message context ID to pull messages from.
   *
   * Defaults to 'current' if not specified.
   * Can be a built-in ID (e.g., 'current') or a custom ID created by Context Processor.
   */
  contextId?: string;

  /**
   * Optional: Whether to append LLM response to specified context
   *
   * Defaults to 'current' if not specified.
   */
  outputContext?: string;

  /** Optional parameter override (overrides parameters in Profile) */
  parameters?: Record<string, unknown>;

  /** Maximum number of tool calls returned by a single LLM call (default 3, error thrown if exceeded) */
  maxToolCallsPerRequest?: number;
}

/**
 * Tool Visibility Node Output
 * - action: 'block' | 'unblock' - The action performed
 * - toolIds: string[] - The tools affected
 */
export interface ToolVisibilityNodeOutput {
  action: 'block' | 'unblock';
  toolIds: string[];
}

/**
 * Tool Visibility Node Configuration
 *
 * Manages tool permissions at runtime by blocking/unblocking tools.
 * This allows phased workflows where different tools are available at different stages.
 *
 * @example
 * ```typescript
 * // Phase 1: Disable editing during exploration
 * const config: ToolVisibilityNodeConfig = {
 *   action: 'block',
 *   toolIds: ['write_file', 'edit_file', 'delete_file'],
 *   reason: 'Complete code exploration first'
 * };
 *
 * // Phase 2: Enable editing after exploration
 * const config: ToolVisibilityNodeConfig = {
 *   action: 'unblock',
 *   toolIds: ['write_file', 'edit_file']
 * };
 * ```
 */
export interface ToolVisibilityNodeConfig {
  /** Action to perform */
  action: 'block' | 'unblock';

  /** Tool IDs to block/unblock */
  toolIds: string[];

  /** Optional reason for blocking (used in rejection message) */
  reason?: string;
}
