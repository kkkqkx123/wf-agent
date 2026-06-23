/**
 * Dynamic Context Type Definition for VSCode Extensions
 * 
 * Extends the SDK's base types to add VSCode-specific functionality
 */

import type { DynamicContextConfig, DynamicRuntimeContext } from "@wf-agent/types";

/**
 * Dynamic Context Configuration for VSCode Extensions
 */
export interface VSCodeDynamicContextConfig extends DynamicContextConfig {
  /** Does it contain any open tabs? */
  includeOpenTabs?: boolean;
  /** Maximum number of tabs */
  maxOpenTabs?: number;
  /** Does it include an active editor? */
  includeActiveEditor?: boolean;
  /** Does it contain diagnostic information? */
  includeDiagnostics?: boolean;
}

/**
 * Runtime Context for VSCode Extensions
 */
export interface VSCodeDynamicRuntimeContext extends DynamicRuntimeContext {
  /** Open tab page */
  openTabs?: string[];
  /** Activity Editor */
  activeEditor?: string;
  /** Diagnostic information */
  diagnostics?: string;
}

/**
 * Diagnostic Information Configuration
 */
export interface DiagnosticsConfig {
  /** Whether to enable */
  enabled: boolean;
  /** The severity levels included are: */
  includeSeverities: ("error" | "warning" | "information" | "hint")[];
  /** Only workspace files */
  workspaceOnly: boolean;
  /** Only the files that are open are displayed. */
  openFilesOnly: boolean;
  /** Maximum number of files */
  maxFiles: number;
  /** The maximum number of diagnoses per file */
  maxDiagnosticsPerFile: number;
}
