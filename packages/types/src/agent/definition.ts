/**
 * Agent Loop Definition Type
 *
 * This type represents the static definition of an Agent Loop configuration.
 * It is used for file-based configuration (TOML/JSON) and is fully serializable.
 *
 * Key characteristics:
 * - Pure data structure with no executable functions (serializable to JSON/TOML)
 * - Includes metadata, versioning, and component definitions (hooks, triggers)
 * - Used by SDK's config parser to load agent configurations from files
 * - Transformed to AgentLoopRuntimeConfig by SDK processors
 *
 * ## Comparison with AgentLoopRuntimeConfig
 *
 * | Aspect | AgentLoopDefinition | AgentLoopRuntimeConfig |
 * |--------|---------------------|------------------------|
 * | Location | packages/types | packages/types |
 * | Purpose | Static/file-based config | Runtime config with callbacks |
 * | Functions | No functions | Contains TransformContextFn, ConvertToLlmFn |
 * | Serializable | Yes | No (due to functions) |
 * | Condition type | String expression | Condition object |
 * | Usage | Loaded from files | Used directly by AgentLoopEntity |
 *
 * @see AgentLoopRuntimeConfig - Runtime configuration with executable functions
 * @see AgentHookStatic - Static hook definition
 * @see AgentTriggerStatic - Static trigger definition
 */

import type { ID, Version, Timestamp } from "../common.js";
import type { Message } from "../message/index.js";
import type { AgentHookStatic, AgentTriggerStatic, AgentCheckpointConfig, AgentLoopMetadata } from "./static-config.js";

/**
 * Agent Loop Definition
 *
 * Static definition for Agent Loop configuration.
 * This is the primary type for file-based agent configuration.
 */
export interface AgentLoopDefinition {
  /** Agent loop unique identifier (required) */
  id: ID;

  /** Agent loop name */
  name?: string;

  /** Agent loop description */
  description?: string;

  /** Configuration version */
  version?: Version;

  // ========== LLM Configuration ==========

  /** LLM Profile ID */
  profileId?: ID;

  /** System prompt (direct string) */
  systemPrompt?: string;

  /** System prompt template ID (references prompt templates config) */
  systemPromptTemplateId?: string;

  /** System prompt template variables */
  systemPromptTemplateVariables?: Record<string, unknown>;

  // ========== Execution Configuration ==========

  /** Maximum iterations (-1 means unlimited) */
  maxIterations?: number;

  /** Initial message list */
  initialMessages?: Message[];

  /** List of allowed tools (array of tool IDs) */
  tools?: string[];

  /** Streaming output or not */
  stream?: boolean;

  // ========== Component Integration ==========

  /** Hook configuration list */
  hooks?: AgentHookStatic[];

  /** Trigger configuration list (for future extension) */
  triggers?: AgentTriggerStatic[];

  // ========== Checkpoint Configuration ==========

  /** Checkpoint configuration */
  checkpoint?: AgentCheckpointConfig;

  // ========== Metadata ==========

  /** Metadata information */
  metadata?: AgentLoopMetadata;

  // ========== Timestamps ==========

  /** Creation timestamp (ISO 8601 format) */
  createdAt?: Timestamp;

  /** Last update timestamp (ISO 8601 format) */
  updatedAt?: Timestamp;
}
