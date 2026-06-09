/**
 * Configuration Index Types
 *
 * Defines types for configuration index files that enable efficient discovery
 * and loading of large numbers of configuration files.
 *
 * Design Principles:
 * - Index files contain ONLY path mappings (minimal and simple)
 * - Metadata is defined in individual config files (single source of truth)
 * - Supports glob patterns for flexible path matching
 * - Index is just a "pointer" to actual config files
 *
 * Example index.json:
 * ```json
 * {
 *   "version": "1.0",
 *   "type": "llm_profiles",
 *   "paths": [
 *     "./openai/*.toml",
 *     "./anthropic/*.toml",
 *     "./gemini-native/*.toml"
 *   ]
 * }
 * ```
 */

/**
 * Base configuration index file structure (simplified)
 * Contains only path mappings - metadata comes from individual files
 */
export interface ConfigIndexFile {
  /** Index file format version */
  version: string;
  /** Index type identifier */
  type: IndexType;
  /**
   * Path patterns to scan for config files.
   * Supports glob patterns:
   * - `./*.toml` - all TOML files in current directory
   * - `./subdir/*.toml` - all TOML files in subdirectory
   * - `./**` + `/*.toml` - all TOML files recursively
   * - `./openai/gpt-*.toml` - specific pattern matching
   */
  paths: string[];
}

/**
 * Resolved index entry with metadata extracted from config file
 * This is the runtime representation after loading and parsing
 */
export interface ResolvedIndexEntry {
  /** Unique identifier from the config file */
  id: string;
  /** Human-readable name from the config file */
  name?: string;
  /** Description from the config file */
  description?: string;
  /** Tags from the config file */
  tags?: string[];
  /** Category from the config file */
  category?: string;
  /** Absolute path to the config file */
  filePath: string;
  /** File format (toml or json) */
  format: "toml" | "json";
  /** Additional metadata from the config file */
  metadata?: Record<string, unknown>;
}

/**
 * LLM Profile specific resolved entry
 */
export interface ResolvedLLMProfileEntry extends ResolvedIndexEntry {
  /** Provider type */
  provider?: string;
  /** Model identifier */
  model?: string;
}

/**
 * Workflow specific resolved entry
 */
export interface ResolvedWorkflowEntry extends ResolvedIndexEntry {
  /** Workflow type */
  type?: string;
  /** Workflow version */
  version?: string;
  /** Author */
  author?: string;
}

/**
 * Node Template specific resolved entry
 */
export interface ResolvedNodeTemplateEntry extends ResolvedIndexEntry {
  /** Node type */
  type?: string;
}

/**
 * Script specific resolved entry
 */
export interface ResolvedScriptEntry extends ResolvedIndexEntry {
  /** Script category */
  category?: string;
  /** Executor type */
  executor?: string;
}

/**
 * Resolved index with all entries loaded
 */
export interface ResolvedIndex<T extends ResolvedIndexEntry = ResolvedIndexEntry> {
  /** Index type */
  type: IndexType;
  /** All resolved entries */
  entries: T[];
  /** Index metadata */
  metadata?: {
    /** When the index was resolved */
    resolvedAt: string;
    /** Total count of entries */
    totalCount: number;
    /** Files that failed to load */
    failures: Array<{ path: string; error: string }>;
  };
}

/**
 * Supported index types
 */
export type IndexType =
  | "llm_profiles"
  | "workflows"
  | "node_templates"
  | "trigger_templates"
  | "hook_templates"
  | "scripts"
  | "prompt_templates"
  | "agent_loops"
  // Preset-based index types
  | "mcp_presets"
  | "skill_presets"
  | "infrastructure_presets";

/**
 * Index file names for each type
 */
export const INDEX_FILE_NAMES: Record<IndexType, string> = {
  llm_profiles: "index.json",
  workflows: "index.json",
  node_templates: "index.json",
  trigger_templates: "index.json",
  hook_templates: "index.json",
  scripts: "index.json",
  prompt_templates: "index.json",
  agent_loops: "index.json",
  mcp_presets: "index.json",
  skill_presets: "index.json",
  infrastructure_presets: "index.json",
} as const;

/**
 * Default config directories for each type
 */
export const DEFAULT_CONFIG_DIRS: Record<IndexType, string> = {
  llm_profiles: "configs/llm-profiles",
  workflows: "configs/workflows",
  node_templates: "configs/node-templates",
  trigger_templates: "configs/trigger-templates",
  hook_templates: "configs/hook-templates",
  scripts: "configs/scripts",
  prompt_templates: "configs/prompt-templates",
  agent_loops: "configs/agent-loops",
  mcp_presets: "configs/mcp",
  skill_presets: "configs/skills",
  infrastructure_presets: "configs/infrastructure",
} as const;

// ---------------------------------------------------------------------------
// Preset-related Types
// ---------------------------------------------------------------------------

/**
 * Skill collection definition file (multi-file preset)
 * Contains paths to actual skill definition files.
 */
export interface SkillCollectionFile {
  /** Collection identifier (matches filename without extension) */
  id: string;
  /** Human-readable name */
  name?: string;
  /** Description */
  description?: string;
  /** Tags for filtering */
  tags?: string[];
  /**
   * Path patterns to scan for skill definition files.
   * Supports same glob patterns as ConfigIndexFile.paths.
   */
  paths: string[];
}

/**
 * Infrastructure preset definition file (multi-file preset)
 * Maps each config domain to its file path.
 */
export interface InfrastructurePresetFile {
  /** Preset identifier (matches filename without extension) */
  id: string;
  /** Human-readable name */
  name?: string;
  /** Description */
  description?: string;
  /** Tags for filtering */
  tags?: string[];
  /** File path mapping for each config domain */
  files: {
    metrics?: string;
    timeout?: string;
    storage?: string;
    output?: string;
    fileCheckpoint?: string;
    presets?: string;
    /** Tool-specific configuration file (readFile, writeFile, etc.) */
    tools?: string;
  };
}
