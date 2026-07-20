/**
 * Skill Registry Service
 *
 * Responsible for skill discovery, parsing, management, content loading, resource loading,
 * metadata prompt generation, permission validation, and event emission.
 *
 * Skills are a cross-cutting concern shared by agents and workflows.
 * This registry is the single source of truth for all skill operations.
 *
 * Follows progressive disclosure design:
 * - Level 1: Metadata prompt (generateMetadataPrompt / injectSkillMetadata)
 * - Level 2: Load full content on demand (loadContent / toPrompt)
 * - Level 3: Load nested resources (loadResources)
 *
 * File I/O operations are delegated to SkillFileLoader to keep this class
 * focused on business logic and easily testable.
 */

import type { SkillFileLoader } from "../../services/skill-loader/types.js";
import type {
  Skill,
  SkillMetadata,
  SkillConfig,
  SkillResourceType,
  SkillLoadContext,
  SkillLoadResult,
  SkillLoadType,
} from "@wf-agent/types";
import {
  SkillParseError as SkillParseErrorClass,
  SkillValidationError as SkillValidationErrorClass,
} from "@wf-agent/types";
import { sdkLogger as logger } from "../../utils/logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import type { EventRegistry } from "./event-registry.js";
import {
  buildSkillLoadStartedEvent,
  buildSkillLoadCompletedEvent,
  buildSkillLoadFailedEvent,
} from "../events/builders/index.js";
import {
  generateSkillMetadataListPrompt,
  generateSkillContentPrompt,
} from "../../resources/predefined/prompt-templates/skill-templates.js";

/**
 * Skill Registry Class
 */
export class SkillRegistry {
  private skills: Map<string, Skill> = new Map();
  /** Runtime enabled state, separated from SkillMetadata to avoid state-in-metadata antipattern */
  private enabledSkills: Set<string> = new Set();
  private config: SkillConfig;
  private contentCache: Map<string, { content: string; timestamp: number }> = new Map();
  private resourceCache: Map<string, { content: string | Buffer; timestamp: number }> = new Map();
  private cacheClearHandlers: Array<() => void> = [];

  /** Internal cache control — not user-configurable */
  private static readonly CACHE_ENABLED = true;
  private static readonly CACHE_TTL = 300000; // 5 minutes
  /** Max entries in the content cache before eviction */
  private static readonly CACHE_MAX_SIZE = 100;
  /** Max entries in the resource cache before eviction */
  private static readonly RESOURCE_CACHE_MAX_SIZE = 500;

  constructor(
    config: SkillConfig,
    private fileLoader: SkillFileLoader,
    private eventManager?: EventRegistry,
  ) {
    this.config = {
      autoScan: true,
      ...config,
    };
  }

  /**
   * Initialize the Skill registry
   * Scan all configured Skill directories
   */
  async initialize(): Promise<void> {
    if (!this.config.autoScan) {
      return;
    }

    for (const skillPath of this.config.paths) {
      await this.scanSkills(skillPath);
    }
  }

  /**
   * Scan the Skill directory
   * @param skillsPath Path to the Skill directory
   */
  async scanSkills(skillsPath: string): Promise<void> {
    const absolutePath = this.fileLoader.resolve(skillsPath);

    try {
      const entries = await this.fileLoader.readDirectory(absolutePath);

      for (const entry of entries) {
        if (!entry.isDirectory) {
          continue;
        }

        const skillDir = this.fileLoader.join(absolutePath, entry.name);
        const skillMdPath = this.fileLoader.join(skillDir, "SKILL.md");

        if (await this.fileLoader.exists(skillMdPath)) {
          await this.loadSkill(skillDir);
        }
      }
    } catch (error) {
      // The directory does not exist or is inaccessible; ignore it.
      logger.warn(`Failed to scan skills directory: ${absolutePath}`, {
        path: absolutePath,
        error: getErrorOrNew(error),
      });
    }
  }

  /**
   * Load Skill
   * @param skillDir Path to the Skill directory
   * @throws SkillParseError If parsing fails
   */
  private async loadSkill(skillDir: string): Promise<void> {
    const skillMdPath = this.fileLoader.join(skillDir, "SKILL.md");

    try {
      const content = await this.fileLoader.readTextFile(skillMdPath);
      const metadata = this.parseSkillMd(content, skillDir);

      // Verify that the directory name matches the name field.
      const dirName = this.fileLoader.basename(skillDir);
      if (metadata.name !== dirName) {
        throw new SkillValidationErrorClass(
          metadata.name,
          `Skill directory name '${dirName}' does not match skill name '${metadata.name}'`,
        );
      }

      // Auto-discover resource directories and populate resource file names
      const resourceTypes: SkillResourceType[] = ["references", "examples", "scripts", "assets"];
      const resources: Partial<Pick<Skill, "references" | "examples" | "scripts" | "assets">> = {};

      for (const resourceType of resourceTypes) {
        try {
          const resourceDir = this.fileLoader.join(skillDir, resourceType);
          const files = await this.fileLoader.listFiles(resourceDir);

          if (files.length > 0) {
            (resources as Record<string, Record<string, undefined>>)[resourceType] = {};
          }
        } catch {
          // Resource directory doesn't exist; skip
        }
      }

      const skill: Skill = {
        metadata,
        path: skillDir,
        ...resources,
      };

      this.skills.set(metadata.name, skill);
      // Newly loaded skills are enabled by default
      this.enabledSkills.add(metadata.name);
    } catch (error) {
      if (error instanceof SkillValidationErrorClass) {
        throw error;
      }

      throw new SkillParseErrorClass(
        skillDir,
        "Failed to parse SKILL.md",
        error instanceof Error ? error : new Error(String(error)),
      );
    }
  }

  /**
   * Parsing the SKILL.md file
   * @param content SKILL.md file content
   * @param skillDir Skill directory path
   * @returns Skill metadata
   * @throws SkillParseError If parsing fails.
   */
  private parseSkillMd(content: string, skillDir: string): SkillMetadata {
    // Extract the YAML frontmatter.
    const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---/);

    if (!frontmatterMatch || !frontmatterMatch[1]) {
      throw new SkillParseErrorClass(skillDir, "Missing YAML frontmatter");
    }

    const frontmatter = frontmatterMatch[1];

    try {
      // Simple YAML parsing (without relying on external libraries)
      const metadata = this.parseYamlFrontmatter(frontmatter);

      // Verify required fields
      if (!metadata["name"]) {
        throw new SkillParseErrorClass(skillDir, "Missing required field: name");
      }

      if (!metadata["description"]) {
        throw new SkillParseErrorClass(skillDir, "Missing required field: description");
      }

      // Verify the name format.
      const name = metadata["name"] as string;
      if (!/^[a-z0-9-]+$/.test(name)) {
        throw new SkillParseErrorClass(
          skillDir,
          `Invalid skill name '${name}': must be lowercase alphanumeric with hyphens only`,
        );
      }

      return {
        name: metadata["name"] as string,
        description: metadata["description"] as string,
        whenToUse: metadata["when_to_use"] as string | undefined,
        version: metadata["version"] as string | undefined,
        license: metadata["license"] as string | undefined,
        allowedTools: metadata["allowedTools"] as string[] | undefined,
        metadata: metadata["metadata"] as Record<string, string> | undefined,
      };
    } catch (error) {
      if (error instanceof SkillParseErrorClass) {
        throw error;
      }

      throw new SkillParseErrorClass(
        skillDir,
        "Failed to parse YAML frontmatter",
        error instanceof Error ? error : new Error(String(error)),
      );
    }
  }

  /**
   * Simple YAML frontmatter parsing
   * @param yaml: A YAML string
   * @returns: The parsed object
   */
  private parseYamlFrontmatter(yaml: string): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    const lines = yaml.split("\n");
    let inArray = false;
    let arrayKey = "";

    for (const line of lines) {
      const trimmed = line.trim();

      if (!trimmed || trimmed.startsWith("#")) {
        continue;
      }

      // Array items
      if (trimmed.startsWith("- ")) {
        if (inArray && arrayKey) {
          if (!result[arrayKey]) {
            result[arrayKey] = [];
          }
          (result[arrayKey] as string[]).push(trimmed.substring(2).trim());
        }
        continue;
      }

      // key-value pair
      const colonIndex = trimmed.indexOf(":");
      if (colonIndex > 0) {
        const key = trimmed.substring(0, colonIndex).trim();
        const value = trimmed.substring(colonIndex + 1).trim();

        // Check if it is the start of an array.
        if (value === "" || value === "[]") {
          arrayKey = key;
          inArray = true;
          if (value === "[]") {
            result[key] = [];
          }
        } else {
          // Parse the value
          result[key] = this.parseYamlValue(value);
          inArray = false;
        }
      }
    }

    return result;
  }

  /**
   * Parse a YAML value
   * @param value A string representing the YAML value
   * @returns The parsed value
   */
  private parseYamlValue(value: string): unknown {
    // Remove the quotes.
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      return value.slice(1, -1);
    }

    // Boolean values
    if (value === "true") return true;
    if (value === "false") return false;

    // Numbers
    const num = Number(value);
    if (!isNaN(num)) return num;

    // null
    if (value === "null" || value === "~") return null;

    // The default is a string.
    return value;
  }

  /**
   * Get all Skill metadata
   * @returns Array of Skill metadata
   */
  getAllSkills(): SkillMetadata[] {
    return Array.from(this.skills.values()).map(skill => skill.metadata);
  }

  /**
   * Get the Skill based on the name
   * @param name: The name of the Skill
   * @returns: The Skill or undefined
   */
  getSkill(name: string): Skill | undefined {
    return this.skills.get(name);
  }

  /**
   * Enable a Skill
   * @param name Skill name
   * @throws Error if skill not found
   */
  enableSkill(name: string): void {
    if (!this.skills.has(name)) {
      throw new Error(`Skill '${name}' not found`);
    }
    this.enabledSkills.add(name);
    logger.info(`Skill enabled: ${name}`);
  }

  /**
   * Disable a Skill
   * @param name Skill name
   * @throws Error if skill not found
   */
  disableSkill(name: string): void {
    if (!this.skills.has(name)) {
      throw new Error(`Skill '${name}' not found`);
    }
    this.enabledSkills.delete(name);
    logger.info(`Skill disabled: ${name}`);
  }

  /**
   * Check if a Skill is enabled
   * @param name Skill name
   * @returns true if enabled
   */
  isSkillEnabled(name: string): boolean {
    return this.enabledSkills.has(name) && this.skills.has(name);
  }

  /**
   * Get all enabled Skill metadata
   * @returns Array of enabled Skill metadata
   */
  getEnabledSkills(): SkillMetadata[] {
    return Array.from(this.skills.values())
      .filter(skill => this.enabledSkills.has(skill.metadata.name))
      .map(skill => skill.metadata);
  }

  /**
   * Get all disabled Skill metadata
   * @returns Array of disabled Skill metadata
   */
  getDisabledSkills(): SkillMetadata[] {
    return Array.from(this.skills.values())
      .filter(skill => !this.enabledSkills.has(skill.metadata.name))
      .map(skill => skill.metadata);
  }

  /**
   * Load Skill full content
   * @param name Skill name
   * Markdown body of @returns SKILL.md
   */
  async loadSkillContent(name: string): Promise<string> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    // Check the cache.
    if (SkillRegistry.CACHE_ENABLED && skill.content) {
      const cached = this.contentCache.get(name);
      if (cached && Date.now() - cached.timestamp < SkillRegistry.CACHE_TTL) {
        return cached.content;
      }
    }

    // Read the file
    const skillMdPath = this.fileLoader.join(skill.path, "SKILL.md");
    const content = await this.fileLoader.readTextFile(skillMdPath);

    // Remove the YAML frontmatter.
    const bodyMatch = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)/);
    const body = bodyMatch && bodyMatch[1] ? bodyMatch[1].trim() : content;

    // Update the cache
    if (SkillRegistry.CACHE_ENABLED) {
      skill.content = body;
      this.contentCache.set(name, { content: body, timestamp: Date.now() });
      this.clearExpiredFromCache(this.contentCache);
      this.evictOldestFromCache(this.contentCache, SkillRegistry.CACHE_MAX_SIZE);
    }

    return body;
  }

  /**
   * Load Skill resource
   * @param name Skill name
   * @param resourceType Resource type
   * @param resourcePath Resource path (relative to the resource directory)
   * @returns Resource content
   */
  async loadSkillResource(
    name: string,
    resourceType: "references" | "examples" | "scripts" | "assets",
    resourcePath: string,
  ): Promise<string | Buffer> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    const cacheKey = `${name}:${resourceType}:${resourcePath}`;

    // Check the cache
    if (SkillRegistry.CACHE_ENABLED) {
      const cached = this.resourceCache.get(cacheKey);
      if (cached && Date.now() - cached.timestamp < SkillRegistry.CACHE_TTL) {
        return cached.content;
      }
    }

    // Read the file
    const fullPath = this.fileLoader.join(skill.path, resourceType, resourcePath);
    const content =
      resourceType === "assets"
        ? await this.fileLoader.readBinaryFile(fullPath)
        : await this.fileLoader.readTextFile(fullPath);

    // Update the cache
    if (SkillRegistry.CACHE_ENABLED) {
      this.resourceCache.set(cacheKey, {
        content: content as string | Buffer,
        timestamp: Date.now(),
      });
      this.clearExpiredFromCache(this.resourceCache);
      this.evictOldestFromCache(this.resourceCache, SkillRegistry.RESOURCE_CACHE_MAX_SIZE);
    }

    return content;
  }

  /**
   * List all resources for the Skill
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Array of resource paths
   */
  async listSkillResources(
    name: string,
    resourceType: "references" | "examples" | "scripts" | "assets",
  ): Promise<string[]> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    const resourceDir = this.fileLoader.join(skill.path, resourceType);

    try {
      return this.fileLoader.listFiles(resourceDir);
    } catch {
      return [];
    }
  }

  /**
   * Register a handler to be called when the cache is cleared or skills are reloaded.
   * Used to propagate cache invalidation to dependent services.
   *
   * @param handler Callback invoked after cache clear or reload
   */
  onCacheClear(handler: () => void): void {
    this.cacheClearHandlers.push(handler);
  }

  /**
   * Remove expired entries from a TTL cache map.
   * Must be called before checking capacity to free up space.
   */
  private clearExpiredFromCache<T>(
    cache: Map<string, { content: T; timestamp: number }>,
  ): void {
    const now = Date.now();
    for (const [key, entry] of cache.entries()) {
      if (now - entry.timestamp >= SkillRegistry.CACHE_TTL) {
        cache.delete(key);
      }
    }
  }

  /**
   * Evict the oldest entry from a cache map when it exceeds maxSize.
   * Expired entries should have been removed first via clearExpiredFromCache.
   */
  private evictOldestFromCache<T>(
    cache: Map<string, { content: T; timestamp: number }>,
    maxSize: number,
  ): void {
    while (cache.size >= maxSize) {
      // Find the oldest entry (by timestamp)
      let oldestKey: string | null = null;
      let oldestTimestamp = Infinity;
      for (const [key, entry] of cache.entries()) {
        if (entry.timestamp < oldestTimestamp) {
          oldestTimestamp = entry.timestamp;
          oldestKey = key;
        }
      }
      if (oldestKey) {
        cache.delete(oldestKey);
      } else {
        break; // Safety guard
      }
    }
  }

  /**
   * Clear the cache
   */
  clearCache(): void {
    this.contentCache.clear();
    this.resourceCache.clear();

    for (const skill of this.skills.values()) {
      skill.content = undefined;
      skill.references = undefined;
      skill.examples = undefined;
      skill.scripts = undefined;
      skill.assets = undefined;
    }

    // Notify all registered handlers
    for (const handler of this.cacheClearHandlers) {
      handler();
    }
  }

  /**
   * Reload all Skills
   */
  async reload(): Promise<void> {
    this.skills.clear();
    this.enabledSkills.clear();
    this.clearCache();
    await this.initialize();
  }

  // ============================================================
  // Progressive Disclosure Level 1: Metadata Prompt
  // ============================================================

  /**
   * Generate Skill Metadata Prompt
   *
   * Only display the metadata of enabled skills, without loading the complete content.
   * Used to inject into system prompts to let the Agent know which Skills are available.
   *
   * @returns Metadata prompt string
   */
  generateMetadataPrompt(): string {
    const skills = this.getEnabledSkills();

    if (skills.length === 0) {
      return "";
    }

    return generateSkillMetadataListPrompt(
      skills.map(skill => ({
        name: skill.name,
        description: skill.description,
        version: skill.version,
      })),
    );
  }

  /**
   * Inject skill metadata into a system prompt.
   *
   * - If prompt contains {SKILLS_METADATA}, replace with metadata
   * - If skills exist and no placeholder, append metadata at the end
   * - If no skills configured or metadata is empty, remove placeholder if present
   *
   * @param systemPrompt Original system prompt
   * @returns System prompt with skill metadata injected
   */
  injectSkillMetadata(systemPrompt: string): string {
    const metadataPrompt = this.generateMetadataPrompt();

    if (!metadataPrompt) {
      return systemPrompt.replace("{SKILLS_METADATA}", "");
    }

    if (systemPrompt.includes("{SKILLS_METADATA}")) {
      return systemPrompt.replace("{SKILLS_METADATA}", metadataPrompt);
    }

    if (this.getEnabledSkills().length === 0) {
      return systemPrompt;
    }

    return `${systemPrompt}\n\n${metadataPrompt}`;
  }

  // ============================================================
  // Progressive Disclosure Level 2: Content Loading
  // ============================================================

  /**
   * Loading Skill Content
   *
   * Load Skill's full content on demand (SKILL.md).
   * Supports variable substitution and permission validation.
   *
   * @param skillName Skill name
   * @param context Loading context (optional)
   * @returns Load results
   */
  async loadContent(
    skillName: string,
    context?: Partial<SkillLoadContext>,
  ): Promise<SkillLoadResult> {
    const startTime = Date.now();
    const loadType: SkillLoadType = "content";

    try {
      // Get Skill
      const skill = this.skills.get(skillName);
      if (!skill) {
        return {
          success: false,
          error: new Error(`Skill '${skillName}' not found`),
          loadTime: Date.now() - startTime,
        };
      }

      // Check if Skill is enabled
      if (!this.isSkillEnabled(skillName)) {
        return {
          success: false,
          error: new Error(`Skill '${skillName}' is disabled`),
          loadTime: Date.now() - startTime,
        };
      }

      // Verify permissions
      if (context?.tools && skill.metadata.allowedTools) {
        const hasPermission = this.validatePermissions(skill, context.tools);
        if (!hasPermission) {
          return {
            success: false,
            error: new Error(
              `Skill '${skillName}' requires tools that are not allowed: ` +
                skill.metadata.allowedTools.filter(t => !context.tools!.includes(t)).join(", "),
            ),
            loadTime: Date.now() - startTime,
          };
        }
      }

      // Check the cache for base content (without variable substitution)
      const cachedEntry = SkillRegistry.CACHE_ENABLED
        ? this.contentCache.get(skillName)
        : undefined;
      let baseContent: string | null = cachedEntry?.content ?? null;

      if (baseContent === null) {
        // Emit load started event
        await this.emitLoadStarted(skillName, loadType, context);

        // Load Skill content
        baseContent = await this.loadSkillContent(skillName);
      }

      // Perform variable substitution if variables are provided
      let finalContent = baseContent;
      if (context?.variables && Object.keys(context.variables).length > 0) {
        finalContent = this.substituteVariables(baseContent, context.variables);
      }

      // Emit load completed event (only if not from cache)
      if (cachedEntry === undefined) {
        await this.emitLoadCompleted(skillName, loadType, false, startTime);
      } else {
        await this.emitLoadCompleted(skillName, loadType, true, startTime);
      }

      return {
        success: true,
        content: finalContent,
        cached: cachedEntry !== undefined,
        loadTime: Date.now() - startTime,
      };
    } catch (error) {
      await this.emitLoadFailed(skillName, loadType, error, startTime);

      return {
        success: false,
        error: error instanceof Error ? error : new Error(String(error)),
        loadTime: Date.now() - startTime,
      };
    }
  }

  /**
   * Convert skill to prompt template format.
   *
   * @param skillName Skill name
   * @returns Formatted prompt string
   */
  async toPrompt(skillName: string): Promise<string> {
    const result = await this.loadContent(skillName);
    if (!result.success || !result.content) {
      throw result.error || new Error(`Failed to load skill: ${skillName}`);
    }

    const skill = this.skills.get(skillName);
    if (!skill) {
      throw new Error(`Skill not found: ${skillName}`);
    }

    return generateSkillContentPrompt({
      name: skill.metadata.name,
      description: skill.metadata.description,
      version: skill.metadata.version,
      content: result.content,
    });
  }

  /**
   * Load Skill Resources
   *
   * Progressive Disclosure Level 3:
   * Loading nested resources for the Skill (references, examples, scripts, assets).
   *
   * @param skillName Skill name
   * @param resourceType Resource type
   * @param context Loading context (optional)
   * @returns Resource content map
   */
  async loadResources(
    skillName: string,
    resourceType: SkillResourceType,
    context?: Partial<SkillLoadContext>,
  ): Promise<Map<string, string | Buffer>> {
    const startTime = Date.now();
    const loadType: SkillLoadType = "resources";

    try {
      await this.emitLoadStarted(skillName, loadType, context);

      const resources = new Map<string, string | Buffer>();
      const resourcePaths = await this.listSkillResources(skillName, resourceType);

      for (const resourcePath of resourcePaths) {
        const content = await this.loadSkillResource(skillName, resourceType, resourcePath);
        resources.set(resourcePath, content);
      }

      await this.emitLoadCompleted(skillName, loadType, false, startTime);

      return resources;
    } catch (error) {
      await this.emitLoadFailed(skillName, loadType, error, startTime);
      throw error;
    }
  }

  /**
   * Verify Skill permission
   * @param skill: The Skill definition
   * @param availableTools: List of available tools
   * @returns: Whether permission is granted
   */
  validatePermissions(skill: Skill, availableTools: string[]): boolean {
    if (!skill.metadata.allowedTools || skill.metadata.allowedTools.length === 0) {
      return true;
    }

    return skill.metadata.allowedTools.every(tool => availableTools.includes(tool));
  }

  /**
   * Constructing the Skill loading context
   * @param skill Skill definition
   * @param agentContext Agent context
   * @param availableTools List of tools available at runtime (for permission verification)
   * @returns Complete loading context
   */
  buildContext(skill: Skill, agentContext?: unknown, availableTools?: string[]): SkillLoadContext {
    return {
      skill,
      agentContext,
      variables: {},
      tools: availableTools || [],
    };
  }

  // ============================================================
  // Private methods
  // ============================================================

  /**
   * Emit Skill load started event
   */
  private async emitLoadStarted(
    skillName: string,
    loadType: SkillLoadType,
    context?: Partial<SkillLoadContext>,
  ): Promise<void> {
    if (!this.eventManager) {
      return;
    }

    await this.eventManager.emit(
      buildSkillLoadStartedEvent({
        skillName,
        loadType,
        executionId:
          ((context?.agentContext as Record<string, unknown>)?.["executionId"] as string) ||
          "skill-registry",
      }),
    );
  }

  /**
   * Emit Skill load completed event
   */
  private async emitLoadCompleted(
    skillName: string,
    loadType: SkillLoadType,
    cached: boolean,
    startTime: number,
  ): Promise<void> {
    if (!this.eventManager) {
      return;
    }

    await this.eventManager.emit(
      buildSkillLoadCompletedEvent({
        skillName,
        loadType,
        success: true,
        cached,
        loadTime: Date.now() - startTime,
        executionId: "skill-registry",
      }),
    );
  }

  /**
   * Emit Skill load failed event
   */
  private async emitLoadFailed(
    skillName: string,
    loadType: SkillLoadType,
    error: unknown,
    startTime: number,
  ): Promise<void> {
    if (!this.eventManager) {
      return;
    }

    await this.eventManager.emit(
      buildSkillLoadFailedEvent({
        skillName,
        loadType,
        error: error instanceof Error ? error : new Error(String(error)),
        loadTime: Date.now() - startTime,
        executionId: "skill-registry",
      }),
    );
  }

  /**
   * Perform variable substitution on skill content.
   * Replaces {{variableName}} placeholders with corresponding values.
   *
   * @param content The raw skill content
   * @param variables Key-value pairs for substitution
   * @returns Content with variables substituted
   */
  private substituteVariables(content: string, variables: Record<string, unknown>): string {
    let result = content;

    for (const [key, value] of Object.entries(variables)) {
      const placeholder = `{{${key}}}`;
      const stringValue = value !== null && value !== undefined ? String(value) : "";
      result = result.split(placeholder).join(stringValue);
    }

    return result;
  }
}
