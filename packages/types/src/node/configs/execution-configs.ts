/**
 * Performs node configuration type definition
 * Contains CODE, LLM, and TOOL node configurations
 */

import type { ID } from '../../common.js';
import { ScriptRiskLevel } from '../../script/script-security.js';

/**
 * Code Node Configuration
 */
export interface ScriptNodeConfig {
  /** screenplay title */
  scriptName: string;
  /** Risk level [different execution policies will be implemented in the application layer, e.g. none not checked, HIGH runs in the sandbox] */
  risk: ScriptRiskLevel;
  /** Whether it is an inline code */
  inline?: boolean;
}

/**
 * LLM Node Configuration
 * 
 * Simplified configuration using named message contexts.
 * Replaces the complex template system with direct context references.
 */
export interface LLMNodeConfig {
  /** Referenced LLM Profile ID */
  profileId: ID;
  
  /**
   * Message context references
   * 
   * - Can be built-in IDs (e.g., 'current', 'system')
   * - Can be custom IDs (created by Context Processor)
   * - Supports multiple contexts, merged in order
   * 
   * @example ["system", "research-notes", "current"]
   */
  contextRefs?: string[];
  
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
 * Tool to add node configuration
 */
export interface AddToolNodeConfig {
  /** List of tool IDs or names to add */
  toolIds: string[];
  /** Tool description template (optional, for dynamic generation of tool descriptions) */
  descriptionTemplate?: string;
  /** Tool scope (optional, defaults to EXECUTION)
   * - EXECUTION: Tools available in current execution instance
   * - LOCAL: Tools available only in current local/subgraph context
   */
  scope?: 'EXECUTION' | 'LOCAL';
  /** Whether to overwrite existing tools (default false) */
  overwrite?: boolean;
  /** Tool metadata (optional) */
  metadata?: Record<string, unknown>;
}
