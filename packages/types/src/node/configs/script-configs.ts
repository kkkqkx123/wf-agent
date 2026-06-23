/**
 * Script Node Configuration Type Definition
 * Contains SCRIPT and INTERACTIVE_SCRIPT node configurations
 */

import { ScriptRiskLevel } from '../../script/script-security.js';
import type { ScriptExecutorConfig } from '../../script/script-executor.js';
import { InteractionMode } from "../../script/script-interactive.js"

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
  /** Sandbox configuration for script isolation */
  sandboxConfig?: import("../../script/script-sandbox.js").SandboxConfig;
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
  interactionMode?: InteractionMode;
  /** Prompt patterns to detect (regex strings indicating script is waiting for input) */
  promptPatterns?: string[];
  /** Maximum interaction rounds */
  maxRounds?: number;
  /** Timeout per interaction round (milliseconds) */
  roundTimeout?: number;
  /** Sandbox configuration for script isolation */
  sandboxConfig?: import("../../script/script-sandbox.js").SandboxConfig;
}