/**
 * User Configuration Type Definition
 *
 * Define user-level configuration item types, such as fixed files, skill configurations, etc.
 */

import type { ID, Metadata, Timestamp } from "./common.js";

/**
 * Fixed document items
 *
 * Used by the user to mark files that need to be fixed in the prompt.
 */
export interface PinnedFileItem {
  /**
   * Document ID
   */
  id: ID;

  /**
   * File path (relative or absolute)
   */
  path: string;

  /**
   * Workspace URIs (multi-workspace environments)
   */
  workspaceUri?: string;

  /**
   * File name (for display)
   */
  filename?: string;

  /**
   * Enable or disable
   */
  enabled: boolean;

  /**
   * Add Time
   */
  addedAt: Timestamp;

  /**
   * File size (bytes, optional)
   */
  size?: number;

  /**
   * Document type (optional)
   */
  fileType?: string;

  /**
   * Metadata (optional)
   */
  metadata?: Metadata;
}

/**
 * Skill set items
 *
 * Used to configure available skills and their behavior
 */
export interface SkillConfigItem {
  /**
   * Skill ID
   */
  id: ID;

  /**
   * skill name
   */
  name: string;

  /**
   * Skill Description
   */
  description: string;

  /**
   * Enable or disable
   */
  enabled: boolean;

  /**
   * Whether to send content to LLM
   */
  sendContent: boolean;

  /**
   * Skills pathway (optional)
   */
  path?: string;

  /**
   * Skill version (optional)
   */
  version?: string;

  /**
   * List of pre-approved tools (optional)
   */
  allowedTools?: string[];

  /**
   * Metadata (optional)
   */
  metadata?: Metadata;
}

/**
 * Fixed file configuration
 */
export interface PinnedFilesConfig {
  /**
   * Fixed file list
   */
  files: PinnedFileItem[];

  /**
   * Maximum file size limit
   */
  maxFiles?: number;

  /**
   * Maximum total size limit (bytes)
   */
  maxSize?: number;

  /**
   * Whether to automatically clean up invalid files
   */
  autoCleanup?: boolean;
}

/**
 * skill configuration
 */
export interface SkillsConfig {
  /**
   * Skills List
   */
  skills: SkillConfigItem[];

  /**
   * Skill Catalog Path List
   */
  paths?: string[];

  /**
   * Whether to automatically scan the skill catalog
   */
  autoScan?: boolean;

  /**
   * Whether to enable caching
   */
  cacheEnabled?: boolean;

  /**
   * Cache expiration time (milliseconds)
   */
  cacheTTL?: number;
}

/**
 * User Configuration Item Type
 */
export type UserConfigType = "pinnedFiles" | "skills";

/**
 * User Configuration Items
 */
export type UserConfigItem = PinnedFileItem | SkillConfigItem;
