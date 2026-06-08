/**
 * Skill Settings Loader
 *
 * File I/O operations for skill configuration settings.
 * Follows the same global/project config pattern as MCP settings:
 *
 * - Global config:  `{settingsDir}/skill-settings.json` — skills available to all projects
 * - Project config: `.agent/skills.json` — project-specific skill directories & overrides
 *
 * Load order:
 *   1. Global config is loaded first
 *   2. Project config is loaded second
 *   3. Paths are concatenated (union, deduplicated), project paths first
 *   4. autoScan uses project value if provided, otherwise global value, then default
 *
 * Lives in the application layer — the SDK only provides pure processing
 * functions (SkillConfig, skill-registry initialization).
 *
 * Cache control (cacheEnabled, cacheTTL) is intentionally NOT part of the
 * user-facing config — it is an internal concern of SkillRegistry, hardcoded
 * as static constants. See: skill-registry.ts CACHE_ENABLED / CACHE_TTL.
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { SkillConfig } from "@wf-agent/types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/**
 * Default skill settings file name (global)
 */
export const DEFAULT_SKILL_SETTINGS_FILE = "skill-settings.json";

/**
 * Default project skill file path (.agent/, traditional convention)
 */
export const PROJECT_SKILL_FILE = ".agent/skills.json";

/**
 * Project skill file path (.wf/, higher priority)
 */
export const PROJECT_WF_SKILL_FILE = ".wf/skills.json";

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

/**
 * Create default empty SkillConfig.
 */
export function createDefaultSkillConfig(): SkillConfig {
  return {
    paths: [],
    autoScan: true,
  };
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/**
 * Get global skill settings file path.
 * @param settingsDir - Global settings directory (e.g., ~/.config/wf-agent)
 */
export function getGlobalSkillSettingsPath(settingsDir: string): string {
  return path.join(settingsDir, DEFAULT_SKILL_SETTINGS_FILE);
}

/**
 * Get project skill file path (.agent/).
 * @param projectRoot - Absolute path to the project root
 */
export function getProjectSkillPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_SKILL_FILE);
}

/**
 * Get project skill file path (.wf/, higher priority).
 * @param projectRoot - Absolute path to the project root
 */
export function getProjectWfSkillPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_WF_SKILL_FILE);
}

/**
 * Get all possible project skill file paths in priority order.
 * .wf/ takes priority over .agent/.
 */
export function getProjectSkillPaths(projectRoot: string): string[] {
  return [
    getProjectWfSkillPath(projectRoot),
    getProjectSkillPath(projectRoot),
  ];
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/**
 * Check if a file exists.
 */
export async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

/**
 * Load skill config from a JSON file.
 * Returns null if the file does not exist.
 * @throws Error if the file exists but cannot be parsed or is invalid.
 */
export async function loadSkillConfig(
  filePath: string,
): Promise<SkillConfig | null> {
  if (!(await fileExists(filePath))) {
    return null;
  }

  const content = await fs.readFile(filePath, "utf-8");

  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch (parseError) {
    throw new Error(
      `Failed to parse skill settings file: ${filePath}. Invalid JSON syntax.`,
      { cause: parseError },
    );
  }

  return normalizeSkillConfig(parsed, filePath);
}

/**
 * Normalize and validate an unknown value into a SkillConfig.
 */
function normalizeSkillConfig(
  value: unknown,
  filePath: string,
): SkillConfig {
  if (typeof value !== "object" || value === null) {
    throw new Error(
      `Invalid skill settings in ${filePath}: expected a JSON object.`,
    );
  }

  const obj = value as Record<string, unknown>;

  // Validate paths (required)
  let paths: string[] = [];
  if (obj["paths"] !== undefined) {
    if (
      !Array.isArray(obj["paths"]) ||
      !(obj["paths"] as unknown[]).every((p) => typeof p === "string")
    ) {
      throw new Error(
        `Invalid skill settings in ${filePath}: 'paths' must be an array of strings.`,
      );
    }
    paths = obj["paths"] as string[];
  }

  // Validate autoScan (optional)
  let autoScan: boolean | undefined;
  if (obj["autoScan"] !== undefined) {
    if (typeof obj["autoScan"] !== "boolean") {
      throw new Error(
        `Invalid skill settings in ${filePath}: 'autoScan' must be a boolean.`,
      );
    }
    autoScan = obj["autoScan"] as boolean;
  }

  const config: SkillConfig = { paths };
  if (autoScan !== undefined) config.autoScan = autoScan;

  return config;
}

// ---------------------------------------------------------------------------
// Merge logic
// ---------------------------------------------------------------------------

/**
 * Merge global, .wf, and .agent SkillConfig into a single config.
 *
 * Merge rules:
 * - **Paths**: Concatenated (union), with .wf paths first, then .agent, then global.
 *   Duplicates are removed.
 * - **autoScan**: .wf wins if provided, then .agent, then global, then default (true).
 *
 * @param globalConfig - Global skill config (may be partial or null)
 * @param wfConfig - .wf/skills.json config (highest priority, may be partial or null)
 * @param agentConfig - .agent/skills.json config (medium priority, may be partial or null)
 * @returns Merged SkillConfig with all fields filled by defaults
 */
export function mergeSkillConfigs(
  globalConfig: SkillConfig | null,
  wfConfig: SkillConfig | null,
  agentConfig?: SkillConfig | null,
): SkillConfig {
  const defaults = createDefaultSkillConfig();

  // --- Paths: union, deduplicated, wf-first then agent then global ---
  const wfPaths = wfConfig?.paths ?? [];
  const agentPaths = agentConfig?.paths ?? [];
  const globalPaths = globalConfig?.paths ?? [];
  const seen = new Set<string>();
  const mergedPaths: string[] = [];

  for (const p of wfPaths) {
    if (!seen.has(p)) {
      seen.add(p);
      mergedPaths.push(p);
    }
  }
  for (const p of agentPaths) {
    if (!seen.has(p)) {
      seen.add(p);
      mergedPaths.push(p);
    }
  }
  for (const p of globalPaths) {
    if (!seen.has(p)) {
      seen.add(p);
      mergedPaths.push(p);
    }
  }

  // --- autoScan: wf → agent → global → default ---
  const autoScan =
    wfConfig?.autoScan ??
    agentConfig?.autoScan ??
    globalConfig?.autoScan ??
    defaults.autoScan!;

  return {
    paths: mergedPaths,
    autoScan,
  };
}

// ---------------------------------------------------------------------------
// Composite loader
// ---------------------------------------------------------------------------

/**
 * Load and merge skill settings from global and all project-level config files.
 *
 * This is the primary entry point — use it during application startup.
 *
 * Priority chain (highest first):
 *   1. .wf/skills.json (project-specific, highest priority)
 *   2. .agent/skills.json (project-specific)
 *   3. Global config at settingsDir (lowest priority)
 *
 * Merge rules:
 * - **Paths**: Union, deduplicated, higher-priority paths first.
 * - **autoScan**: Highest-priority source wins (`.wf` > `.agent` > global > default).
 *
 * @param settingsDir - Global settings directory
 * @param projectRoot - Absolute path to the project root
 * @returns Merged SkillConfig
 *
 * @example
 * ```ts
 * const config = await loadAndMergeSkillConfig(
 *   "~/.config/wf-agent",
 *   "/home/user/my-project",
 * );
 * // config.paths = [...wf paths, ...agent paths, ...global paths] (deduplicated)
 * // config.autoScan = .wf value || .agent value || global value || true
 * ```
 */
export async function loadAndMergeSkillConfig(
  settingsDir: string,
  projectRoot: string,
): Promise<SkillConfig> {
  const globalPath = getGlobalSkillSettingsPath(settingsDir);
  const projectPaths = getProjectSkillPaths(projectRoot);

  const [globalConfig, wfConfig, agentConfig] = await Promise.all([
    loadSkillConfig(globalPath),
    loadSkillConfig(projectPaths[0]!), // .wf/skills.json
    loadSkillConfig(projectPaths[1]!), // .agent/skills.json
  ]);

  return mergeSkillConfigs(globalConfig, wfConfig, agentConfig);
}

/**
 * Write skill config to a JSON file.
 */
export async function writeSkillConfig(
  filePath: string,
  config: SkillConfig,
): Promise<void> {
  const content = JSON.stringify(config, null, 2);
  await fs.writeFile(filePath, content, "utf-8");
}

/**
 * Ensure skill settings file exists, creating a default one if absent.
 * @returns true if the file was created, false if it already existed.
 */
export async function ensureSkillConfigFile(
  filePath: string,
): Promise<boolean> {
  if (await fileExists(filePath)) {
    return false;
  }

  const dir = path.dirname(filePath);
  await fs.mkdir(dir, { recursive: true });
  await writeSkillConfig(filePath, createDefaultSkillConfig());
  return true;
}