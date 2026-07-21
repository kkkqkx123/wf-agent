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
import type { AgentToolConfig } from "./tool-config.js";
import type { DynamicContextConfig } from "../dynamic-context.js";
import type { ToolCallFormatConfig } from "../llm/tool-call-format.js";
import type { ToolCallProtocolViolationPolicy } from "../llm/protocol-config.js";

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

  /**
   * Tool configuration for Agent Loop
   * 
   * Specifies which tools are available during agent loop execution.
   * Uses simplified AgentToolConfig (no initial/dynamic concepts needed).
   */
  availableTools?: AgentToolConfig;

  /** Streaming output or not */
  stream?: boolean;

  // ========== Component Integration ==========

  /** Hook configuration list */
  hooks?: AgentHookStatic[];

  /** Trigger configuration list (for future extension) */
  triggers?: AgentTriggerStatic[];

  // ========== Dynamic Context Configuration ==========

  /**
   * Dynamic context configuration
   *
   * Specifies what dynamic content to include during agent execution:
   * - Current time and timezone
   * - TODO lists and pinned files
   * - Environment information
   * - Workspace files
   * - Skills and workflows
   *
   * These are injected before each LLM call for KV cache optimization.
   * If not specified, defaults will be applied.
   */
  dynamicContext?: DynamicContextConfig;

  // ========== Checkpoint Configuration ==========

  /** Checkpoint configuration */
  checkpoint?: AgentCheckpointConfig;

  // ========== Tool Call Format Configuration ==========

  /**
   * Tool call format configuration.
   *
   * Specifies the expected tool call protocol for this agent.
   * If set, must be compatible with the referenced LLMProfile.toolCallFormat.
   * If not set, inherits from LLMProfile.toolCallFormat.
   *
   * Purpose: allows static definition to declare the expected protocol,
   * enabling pre-check at load time and locking at runtime.
   */
  toolCallFormat?: ToolCallFormatConfig;

  /**
   * Protocol violation policy for this agent definition.
   * Overrides the global default policy.
   * Resolution order: request-level > agent-level > global default.
   */
  violationPolicy?: ToolCallProtocolViolationPolicy;

  // ========== Metadata ==========

  /** Metadata information */
  metadata?: AgentLoopMetadata;

  // ========== Timestamps ==========

  /** Creation timestamp (ISO 8601 format) */
  createdAt?: Timestamp;

  /** Last update timestamp (ISO 8601 format) */
  updatedAt?: Timestamp;
}

/**
 * Agent Template - Complete agent definition template for reuse.
 *
 * Extends AgentLoopDefinition with template-specific metadata
 * for organizing, searching, and tracking agent templates.
 */
export interface AgentTemplate extends AgentLoopDefinition {
  /** Template ID (overrides AgentLoopDefinition's id) */
  id: string;
  /** Template name (for display) */
  templateName: string;
  /** Template category (for organization) */
  templateCategory?: string;
  /** Template tags (for searching) */
  templateTags?: string[];
  /** Whether this is a public template */
  isPublic?: boolean;
  /** Number of times used */
  usageCount?: number;
  /** Whether template is enabled */
  enabled?: boolean;
}
