/**
 * Tool Definition Type
 */

import type { ID } from "../common.js";
import type { ToolType } from "./state.js";
import type { ToolParameterSchema, ToolMetadata } from "./static-config.js";
import type { ToolConfig } from "./runtime-config.js";

/**
 * Tool Definition Type
 */
export interface Tool {
  /** Tool Unique Identifier */
  id: ID;
  /** Tool name */
  name: string;
  /** Tool type */
  type: ToolType;
  /** Tool Description */
  description: string;
  /** Parameter schema (JSON Schema format) */
  parameters: ToolParameterSchema;
  /** Tool metadata */
  metadata?: ToolMetadata;
  /** Tool configuration (type-specific) */
  config?: ToolConfig;
  /** Whether checkpoints are created on tool invocation (new) */
  createCheckpoint?: boolean | "before" | "after" | "both";
  /** Checkpoint description template (new) */
  checkpointDescriptionTemplate?: string;
  /**
   * Whether to enable strict mode for OpenAI compatibility
   * When true, enforces strict validation rules aligned with OpenAI's strict mode
   * @default false
   */
  strict?: boolean;
}

/**
 * LLM tools invoke schema type
 */
export interface ToolSchema {
  /** Tool ID */
  id: string;
  /** Tool Description */
  description: string;
  /** Parameter schema */
  parameters: ToolParameterSchema;
}
