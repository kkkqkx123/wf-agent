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
  /** 脚本语言(shell/cmd/powershell/python/javascript) */
  scriptType: 'shell' | 'cmd' | 'powershell' | 'python' | 'javascript';
  /** Risk level [different execution policies will be implemented in the application layer, e.g. none not checked, HIGH runs in the sandbox] */
  risk: ScriptRiskLevel;
  /** Whether it is an inline code */
  inline?: boolean;
}

/**
 * LLM Node Configuration
 */
export interface LLMNodeConfig {
  /** Referenced LLM Profile ID */
  profileId: ID;
  /** Cue word (specified directly, lower priority than templateId) */
  prompt?: string;
  /** Prompt word template ID (references a predefined template, higher priority than prompt) */
  promptTemplateId?: string;
  /** Template variable (required when promptTemplateId is used) */
  promptTemplateVariables?: Record<string, unknown>;
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
  /** Tool scope (optional, defaults to EXECUTION) */
  scope?: 'GLOBAL' | 'EXECUTION' | 'LOCAL';
  /** Whether to overwrite existing tools (default false) */
  overwrite?: boolean;
  /** Tool metadata (optional) */
  metadata?: Record<string, unknown>;
}
