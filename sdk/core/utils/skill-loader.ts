/**
 * Skill Loader
 *
 * Responsible for loading Skills, performing permission verification, and managing access control to resources. A Skill is a collection of static resources (prompt words, reference scripts, and resource files) that should be "loaded" rather than "executed".
 *
 *
 */

import type {
  Skill,
  SkillLoadContext,
  SkillLoadResult,
  SkillResourceType,
  SkillLoadType,
} from "@wf-agent/types";
import type { SkillRegistry } from "../registry/skill-registry.js";
import type { EventRegistry } from "../registry/event-registry.js";
import { CheckpointStore } from "../checkpoint/utils/checkpoint-store.js";
import {
  buildSkillLoadStartedEvent,
  buildSkillLoadCompletedEvent,
  buildSkillLoadFailedEvent,
} from "../utils/event/builders/index.js";
import {
  generateSkillMetadataListPrompt,
  generateSkillContentPrompt,
} from "@wf-agent/prompt-templates";

/**
 * Skill Loader Class
 *
 * Implementing three levels of progressive disclosure:
 * - Level 1: Only display metadata (generateMetadataPrompt)
 * - Level 2: Load the full content on demand (loadContent)
 * - Level 3: Load nested resources (loadResources)
 */
export class SkillLoader {
  /** Content Caching Service */
  private contentCache: CheckpointStore<string>;

  constructor(
    private skillRegistry: SkillRegistry,
    private eventManager: EventRegistry,
    cacheTTL: number = 300000, // Default is 5 minutes.
  ) {
    this.contentCache = new CheckpointStore<string>({ ttl: cacheTTL });

    // Register cache invalidation handler: when SkillRegistry clears its cache,
    // also clear SkillLoader's cache to prevent stale data
    this.skillRegistry.onCacheClear(() => {
      this.contentCache.clear();
    });
  }

  /**
   * Generate Skill Metadata Prompt
   *
   * Progressive Disclosure Level 1:
   * Only display the metadata of the Skill, without loading the complete content.
   * This is used to inject into system prompts to let the Agent know which Skills are available.
   * Only enabled skills are included.
   *
   * @returns Metadata prompt string
   */
  generateMetadataPrompt(): string {
    const skills = this.skillRegistry.getEnabledSkills();

    if (skills.length === 0) {
      return "";
    }

    // Use template from prompt-templates package
    return generateSkillMetadataListPrompt(
      skills.map(skill => ({
        name: skill.name,
        description: skill.description,
        version: skill.version,
      })),
    );
  }

  /**
   * Loading Skill Content
   *
   * Progressive Disclosure Level 2.
   * Load Skill's full content on demand (SKILL.md).
   *
   * @param skillName Skill name
   * @param context Loading context
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
      const skill = this.skillRegistry.getSkill(skillName);
      if (!skill) {
        return {
          success: false,
          error: new Error(`Skill '${skillName}' not found`),
          loadTime: Date.now() - startTime,
        };
      }

      // Check if Skill is enabled
      if (!this.skillRegistry.isSkillEnabled(skillName)) {
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
      const fromCache = this.contentCache.get(skillName);
      let baseContent = fromCache !== null ? fromCache : null;

      if (baseContent === null) {
        // Send the Skill loading start event only when actually loading
        await this.emitLoadStarted(skillName, loadType, context);

        // Load Skill content from registry
        baseContent = await this.skillRegistry.loadSkillContent(skillName);

        // Cache the base content (before variable substitution)
        this.contentCache.set(skillName, baseContent);
      }

      // Perform variable substitution if variables are provided
      let finalContent = baseContent;
      if (context?.variables && Object.keys(context.variables).length > 0) {
        finalContent = this.substituteVariables(baseContent, context.variables);
      }

      // Emit load completed event if we actually loaded (not from cache)
      if (fromCache === null) {
        await this.emitLoadCompleted(skillName, loadType, false, startTime);
      } else {
        await this.emitLoadCompleted(skillName, loadType, true, startTime);
      }

      return {
        success: true,
        content: finalContent,
        cached: fromCache !== null,
        loadTime: Date.now() - startTime,
      };
    } catch (error) {
      // Skill loading failed event.
      await this.emitLoadFailed(skillName, loadType, error, startTime);

      return {
        success: false,
        error: error instanceof Error ? error : new Error(String(error)),
        loadTime: Date.now() - startTime,
      };
    }
  }

  /**
   * Load Skill Resources
   *
   * Progressive Disclosure Level 3:
   * Loading nested resources for the Skill (references, examples, scripts, assets).
   *
   * @param skillName Skill name
   * @param resourceType Resource type
   * @param context Loading context
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
      // Send the Skill loading start event
      await this.emitLoadStarted(skillName, loadType, context);

      const resources = new Map<string, string | Buffer>();
      const resourcePaths = await this.skillRegistry.listSkillResources(skillName, resourceType);

      for (const resourcePath of resourcePaths) {
        const content = await this.skillRegistry.loadSkillResource(
          skillName,
          resourceType,
          resourcePath,
        );
        resources.set(resourcePath, content);
      }

      // Send the Skill loading completion event
      await this.emitLoadCompleted(skillName, loadType, false, startTime);

      return resources;
    } catch (error) {
      // Sending a Skill loading failure event.
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

    // Check whether all the required tools are in the list of available tools.
    return skill.metadata.allowedTools.every(tool => availableTools.includes(tool));
  }

  /**
   * Constructing the Skill loading context
   * @param skill Skill definition
   * @param agentContext Agent context
   * @param availableTools List of tools available at runtime (for permission verification)
   * @returns Complete loading context
   */
  buildContext(
    skill: Skill,
    agentContext?: unknown,
    availableTools?: string[],
  ): SkillLoadContext {
    return {
      skill,
      agentContext,
      variables: {},
      tools: availableTools || [],
    };
  }

  /**
   * Translate from auto to en:
   *
   * ```plaintext
   * function convertSkillToTipWord(skillName) {
   */
  async toPrompt(skillName: string): Promise<string> {
    const result = await this.loadContent(skillName);
    if (!result.success || !result.content) {
      throw result.error || new Error(`Failed to load skill: ${skillName}`);
    }

    const skill = this.skillRegistry.getSkill(skillName);
    if (!skill) {
      throw new Error(`Skill not found: ${skillName}`);
    }

    // Use template from prompt-templates package
    return generateSkillContentPrompt({
      name: skill.metadata.name,
      description: skill.metadata.description,
      version: skill.metadata.version,
      content: result.content,
    });
  }

  /**
   * Clear cache
   * @param skillName (optional) The name of the skill to clear; if not specified, all skills will be cleared
   */
  clearCache(skillName?: string): void {
    if (skillName) {
      this.contentCache.delete(skillName);
    } else {
      this.contentCache.clear();
    }
  }

  /**
   * Get cache statistics information
   * @returns Cache statistics
   */
  getCacheStats() {
    return this.contentCache.getStats();
  }

  // ============================================================
  // Private method
  // ============================================================

  /**
   * Send the Skill loading start event
   */
  private async emitLoadStarted(
    skillName: string,
    loadType: SkillLoadType,
    context?: Partial<SkillLoadContext>,
  ): Promise<void> {
    await this.eventManager.emit(
      buildSkillLoadStartedEvent({
        skillName,
        loadType,
        executionId:
          ((context?.agentContext as Record<string, unknown>)?.["executionId"] as string) ||
          "skill-loader",
      }),
    );
  }

  /**
   * Send the Skill loading completion event
   */
  private async emitLoadCompleted(
    skillName: string,
    loadType: SkillLoadType,
    cached: boolean,
    startTime: number,
  ): Promise<void> {
    await this.eventManager.emit(
      buildSkillLoadCompletedEvent({
        skillName,
        loadType,
        success: true,
        cached,
        loadTime: Date.now() - startTime,
        executionId: "skill-loader",
      }),
    );
  }

  /**
   * Skill loading failed event.
   */
  private async emitLoadFailed(
    skillName: string,
    loadType: SkillLoadType,
    error: unknown,
    startTime: number,
  ): Promise<void> {
    await this.eventManager.emit(
      buildSkillLoadFailedEvent({
        skillName,
        loadType,
        error: error instanceof Error ? error : new Error(String(error)),
        loadTime: Date.now() - startTime,
        executionId: "skill-loader",
      }),
    );
  }

  /**
   * Perform variable substitution on skill content.
   * Replaces {{variableName}} placeholders with corresponding values from the variables map.
   *
   * @param content The raw skill content
   * @param variables Key-value pairs for substitution
   * @returns Content with variables substituted
   */
  private substituteVariables(
    content: string,
    variables: Record<string, unknown>,
  ): string {
    let result = content;

    for (const [key, value] of Object.entries(variables)) {
      const placeholder = `{{${key}}}`;
      const stringValue = value !== null && value !== undefined ? String(value) : "";
      // Replace all occurrences of the placeholder
      result = result.split(placeholder).join(stringValue);
    }

    return result;
  }
}
