/**
 * Builtin Tool Types
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type { WorkflowHandlerConfig } from "./workflow/types.js";
import type { AgentHandlerConfig } from "./agent/call-agent/index.js";

/**
 * Builtin tool category
 */
export type BuiltinToolCategory =
  | "workflow" // Workflow execution tools
  | "agent" // Agent interaction tools
  | "interaction"; // User interaction tools

/**
 * Builtin tool definition
 */
export interface BuiltinToolDefinition {
  /** Tool ID */
  id: string;
  /** Tool name */
  name: string;
  /** Tool category */
  category: BuiltinToolCategory;
  /** Tool description */
  description: string;
  /** Parameter schema */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  parameters: any;
  /** Create handler function */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  createHandler: () => (params: any, context: BuiltinToolExecutionContext) => Promise<any>;
}

/**
 * Builtin tools configuration options
 */
export interface BuiltinToolsOptions {
  /** Only enable the specified tools (allowlist) */
  allowList?: string[];
  /** Disable the specified tools (blocklist) */
  blockList?: string[];
  /** Workflow tool configuration (for visibility control) */
  workflow?: WorkflowHandlerConfig;
  /** Agent tool configuration (for agent profile discovery) */
  agent?: AgentHandlerConfig;
}
