/**
 * Dynamic Context Type Definition
 *
 * Defines type interfaces related to dynamic cues
 * Used to dynamically generate context content at runtime
 */

import type { TodoItem } from "./todo.js";
import type { PinnedFileItem } from "./user-config.js";
import type { SkillConfigItem } from "./user-config.js";

/**
 * Dynamic Context Configuration
 *
 * Configure what dynamic content should be included in the cue word
 */
export interface DynamicContextConfig {
  /**
   * Whether to include the current time
   */
  includeCurrentTime?: boolean;

  /**
   * Whether to include a TODO list
   */
  includeTodoList?: boolean;

  /**
   * Whether to include the workspace file tree
   */
  includeWorkspaceFiles?: boolean;

  /**
   * Maximum depth of file tree
   */
  maxFileDepth?: number;

  /**
   * Ignored file mode (glob mode)
   */
  ignorePatterns?: string[];

  /**
   * Whether or not it contains fixed files
   */
  includePinnedFiles?: boolean;

  /**
   * Includes Skills
   */
  includeSkills?: boolean;

  /**
   * Does it contain environmental information
   */
  includeEnvironmentInfo?: boolean;

  /**
   * Customized content snippets (optional)
   */
  customSections?: Record<string, string>;
}

/**
 * Dynamic Runtime Context
 *
 * Dynamic data from session metadata or other sources
 */
export interface DynamicRuntimeContext {
  /**
   * TODO List
   */
  todoList?: TodoItem[];

  /**
   * Fixed documents
   */
  pinnedFiles?: PinnedFileItem[];

  /**
   * Skills
   */
  skills?: SkillConfigItem[];

  /**
   * Workspace file tree (optional pre-generation)
   */
  workspaceFileTree?: string;

  /**
   * current timestamp
   */
  currentTime?: number;

  /**
   * Customizing Runtime Data
   */
  customData?: Record<string, unknown>;
}

/**
 * dynamic context message (computing)
 *
 * The format of the generated dynamic context message
 */
export interface DynamicContextMessage {
  /**
   * Role (fixed to user)
   */
  role: "user";

  /**
   * element
   */
  content: string;

  /**
   * Message type (optional)
   */
  type?: "context" | "system" | "info";

  /**
   * Metadata (optional)
   */
  metadata?: Record<string, unknown>;
}

/**
 * Dynamic cue word generation results
 *
 * Complete cue generation results with static and dynamic parts
 */
export interface DynamicPromptResult {
  /**
   * Static system prompt words (cacheable)
   */
  staticPrompt: string;

  /**
   * Dynamic context message (inserted on each request)
   */
  dynamicMessages: DynamicContextMessage[];

  /**
   * Total number of tokens (optional)
   */
  totalTokens?: number;

  /**
   * Generation time (milliseconds, optional)
   */
  generationTime?: number;
}

/**
 * Dynamic context generation options
 */
export interface DynamicContextOptions {
  /**
   * Maximum token limit
   */
  maxTokens?: number;

  /**
   * Whether to enable compression
   */
  enableCompression?: boolean;

  /**
   * Whether to enable caching
   */
  enableCache?: boolean;

  /**
   * Cache expiration time (milliseconds)
   */
  cacheTTL?: number;
}

/**
 * Dynamic Contextual Fragments
 *
 * Reusable Dynamic Content Fragments
 */
export interface DynamicContextFragment {
  /**
   * Clip name
   */
  name: string;

  /**
   * Content of the clip
   */
  content: string;

  /**
   * Segment prioritization (for sorting)
   */
  priority?: number;

  /**
   * Required or not
   */
  required?: boolean;

  /**
   * Conditional functions (optional)
   */
  condition?: (config: DynamicContextConfig, runtime?: DynamicRuntimeContext) => boolean;
}
