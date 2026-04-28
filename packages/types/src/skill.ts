/**
 * Skill 类型定义
 *
 * 遵循 Claude Code Skill 规范
 * @see https://github.com/anthropics/claude-code
 */

/**
 * Skill Metadata
 * 
 * Parsed from SKILL.md's YAML frontmatter
 */
export interface SkillMetadata {
  /**
   * Skill name
   * - Must use hyphen-case (lowercase letters + hyphen)
   * - Must match the name of the directory containing SKILL.md
   */
  name: string;

  /**
   * Skill Description
   * - Describes Skill's functionality and usage scenarios
   * - Used for Skill matching and discovery
   */
  description: string;

  /**
   * Skill version number (optional)
   */
  version?: string;

  /**
   * License (optional)
   */
  license?: string;

  /**
   * List of pre-approved tools (optional)
   * - Currently only supported in Claude Code
   */
  allowedTools?: string[];

  /**
   * Customized metadata (optional)
   * - Client can be used to store additional attributes
   * - Sensible unique key names are recommended to avoid conflicts
   */
  metadata?: Record<string, string>;
}

/**
 * Skill Resource type
 */
export type SkillResourceType = "references" | "examples" | "scripts" | "assets";

/**
 * Skill Definition
 *
 * Represents a complete Skill, including metadata and content.
 */
export interface Skill {
  /**
   * Skill Metadata
   */
  metadata: SkillMetadata;

  /**
   * Absolute path to the Skill directory
   */
  path: string;

  /**
   * Markdown body of SKILL.md
   * - Does not contain YAML frontmatter
   * - Lazy loading, only populate when needed
   */
  content?: string;

  /**
   * Reference Document
   * - Key: file name (relative path)
   * - Value: file content
   * - Lazy loading
   */
  references?: Record<string, string>;

  /**
   * Sample Code
   * - Key: file name (relative path)
   * - Value: file content
   * - Lazy loading
   */
  examples?: Record<string, string>;

  /**
   * Tool Scripts
   * - Key: file name (relative path)
   * - Value: file content
   * - Lazy loading
   */
  scripts?: Record<string, string>;

  /**
   * Resource File
   * - Key: file name (relative path)
   * - Value: file content or Buffer
   * - Lazy loading
   */
  assets?: Record<string, string | Buffer>;
}

/**
 * Skill Configuration
 *
 * Used to configure the behavior of the Skill system
 */
export interface SkillConfig {
  /**
   * Skill Directory Path List
   * - Support for multiple directories
   * - Supports relative and absolute paths
   */
  paths: string[];

  /**
   * Whether to automatically scan the Skill directory
   * - Default: true
   */
  autoScan?: boolean;

  /**
   * Enable or disable caching
   * - Default: true
   */
  cacheEnabled?: boolean;

  /**
   * Cache expiration time (milliseconds)
   * - Default: 300000 (5 minutes)
   */
  cacheTTL?: number;
}

/**
 * Skill Match Results
 */
export interface SkillMatchResult {
  /**
   * Matching Skill Metadata
   */
  skill: SkillMetadata;

  /**
   * Match Score
   * - Range: 0-1
   * - Higher indicates a better match
   */
  score: number;

  /**
   * Reason for matching
   */
  reason: string;
}

/**
 * Skill loading context
 */
export interface SkillLoadContext {
  /**
   * Skill to be loaded
   */
  skill: Skill;

  /**
   * Agent Context
   * - Used to access the Agent's state and resources
   */
  agentContext?: unknown;

  /**
   * Variables
   * - For variable substitution when Skill is loaded
   */
  variables?: Record<string, unknown>;

  /**
   * List of available tools
   * - For permission verification
   */
  tools?: string[];
}

/**
 * Skill loading results
 */
export interface SkillLoadResult {
  /**
   * success or failure
   */
  success: boolean;

  /**
   * Skill Content
   */
  content?: string;

  /**
   * Results data
   */
  data?: unknown;

  /**
   * error message
   */
  error?: Error;

  /**
   * Load time (milliseconds)
   */
  loadTime?: number;

  /**
   * Whether from cache
   */
  cached?: boolean;
}

/**
 * Skill parsing error
 */
export class SkillParseError extends Error {
  constructor(
    public readonly skillPath: string,
    public readonly reason: string,
    public readonly originalError?: Error,
  ) {
    super(`Failed to parse skill at ${skillPath}: ${reason}`);
    this.name = "SkillParseError";
  }
}

/**
 * Skill Authentication Error
 */
export class SkillValidationError extends Error {
  constructor(
    public readonly skillName: string,
    public readonly reason: string,
  ) {
    super(`Skill validation failed for ${skillName}: ${reason}`);
    this.name = "SkillValidationError";
  }
}
