/**
 * SkillRegistryAPI - Skill Resource Management API
 * Encapsulates SkillRegistry and SkillLoader, providing functions for skill registration, querying, and loading.
 *
 * Design Patterns:
 * - Inherits from GenericResourceAPI to provide unified CRUD (Create, Read, Update, Delete) operations.
 * - Implements support for a progressive disclosure API layer.
 */

import {
  validateRequiredFields,
  validateStringLength,
} from "../../validation/validation-strategy.js";

import type { SkillMetadata, SkillMatchResult, SkillResourceType, Skill } from "@wf-agent/types";
import { NotFoundError, ExecutionError } from "@wf-agent/types";
import { ReadonlyResourceAPI } from "../generic-resource-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { ExecutionResult } from "../../types/execution-result.js";
import { success, failure } from "../../types/execution-result.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Skill Filter
 */
export interface SkillFilter {
  /** Skill Name (Fuzzy Search Supported) */
  name?: string;
  /** Tag array */
  tags?: string[];
  /** Version */
  version?: string;
}

/**
 * Skill loading options
 */
export interface SkillLoadOptions {
  /** Load context */
  context?: {
    tools?: string[];
    agentContext?: unknown;
  };
  /** Whether to use caching */
  useCache?: boolean;
}

/**
 * SkillRegistryAPI - Skill Resource Management API
 *
 * Provides Skill CRUD operations and progressive disclosure support:
 * - Level 1: generateMetadataPrompt() - metadata prompt
 * - Level 2: loadContent() - on-demand content loading
 * - Level 3: loadResources() - nested resource loading
 */
export class SkillRegistryAPI extends ReadonlyResourceAPI<SkillMetadata, string, SkillFilter> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  // ============================================================================
  // Implementation of the GenericResourceAPI abstract method
  // ============================================================================

  /**
   * Get metadata for a single Skill
   * @param name Skill name
   * @returns Skill metadata; returns null if not found
   */
  protected async getResource(name: string): Promise<SkillMetadata | null> {
    try {
      const skill = this.getSkillRegistry().getSkill(name);
      return skill ? skill.metadata : null;
    } catch (error) {
      if (error instanceof NotFoundError) {
        return null;
      }
      throw error;
    }
  }

  /**
   * Get all Skill metadata
   * @returns Array of Skill metadata
   */
  protected async getAllResources(): Promise<SkillMetadata[]> {
    return this.getSkillRegistry().getAllSkills();
  }

  /**
   * Apply filter criteria
   * @param skills Array of Skill metadata
   * @param filter Filter criteria
   * @returns Array of Skills after filtering
   */
  protected override applyFilter(skills: SkillMetadata[], filter: SkillFilter): SkillMetadata[] {
    return skills.filter(skill => {
      if (filter.name && !skill.name.includes(filter.name)) {
        return false;
      }
      if (filter.version && skill.version !== filter.version) {
        return false;
      }
      // Tag filtering (if Skill has a metadata field)
      if (filter.tags && skill.metadata) {
        const skillTags = Object.values(skill.metadata);
        if (!filter.tags.every(tag => skillTags.includes(tag))) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Verify Skill metadata
   * @param skill Skill metadata
   * @returns Verification result
   */
  protected async validateResource(
    skill: SkillMetadata,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Verify required fields
    const requiredResult = validateRequiredFields(skill, ["name", "description"], "skill");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Verify the name format.
    if (skill.name) {
      const nameResult = validateStringLength(skill.name, "Skill Name", 1, 100);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(error => error.message)));
      }
      // Verify name format: lowercase letters, numbers, and hyphens
      if (!/^[a-z0-9-]+$/.test(skill.name)) {
        errors.push(
          `Invalid skill name '${skill.name}': must be lowercase alphanumeric with hyphens only`,
        );
      }
    }

    // Verify the description length.
    if (skill.description) {
      const descResult = validateStringLength(skill.description, "Skill Description", 1, 1000);
      if (descResult.isErr()) {
        errors.push(...descResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  // ============================================================================
  // Skill-specific method - Progressive disclosure support
  // ============================================================================

  /**
   * Generate Skill Metadata Prompt (Progressive Disclosure Level 1)
   *
   * Used to inject into system prompts to let the Agent know which Skills are available
   *
   * @returns Metadata prompt string
   */
  generateMetadataPrompt(): string {
    return this.getSkillLoader().generateMetadataPrompt();
  }

  /**
   * Load Skill content (Progressive Disclosure Level 2)
   *
   * On-demand loading of complete Skill content (SKILL.md)
   *
   * @param name Skill name
   * @param options Load options
   * @returns Execution result
   */
  async loadContent(name: string, options?: SkillLoadOptions): Promise<ExecutionResult<string>> {
    const startTime = now();

    try {
      const result = await this.getSkillLoader().loadContent(name, options?.context);

      if (!result.success || !result.content) {
        const error =
          result.error instanceof Error
            ? new ExecutionError(result.error.message, undefined, undefined, {
                cause: result.error,
              })
            : new NotFoundError(`Failed to load skill: ${name}`, "skill", name);
        return failure(error, diffTimestamp(startTime, now()));
      }

      return success(result.content, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "LOAD_CONTENT", startTime);
    }
  }

  /**
   * Loading Skill resources (Progressive Disclosure Level 3)
   *
   * Loading nested resources of the Skill (references, examples, scripts, assets)
   *
   * @param name Skill name
   * @param resourceType Resource type
   * @param options Loading options
   * @returns Execution result
   */
  async loadResources(
    name: string,
    resourceType: SkillResourceType,
    options?: SkillLoadOptions,
  ): Promise<ExecutionResult<Map<string, string | Buffer>>> {
    const startTime = now();

    try {
      const resources = await this.getSkillLoader().loadResources(
        name,
        resourceType,
        options?.context,
      );
      return success(resources, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "LOAD_RESOURCES", startTime);
    }
  }

  /**
   * ```python
   * def convertSkill(name: str) -> str:
   *     return f"Convert {name} to prompt word format"
   * ```
   */
  async toPrompt(name: string): Promise<ExecutionResult<string>> {
    const startTime = now();

    try {
      const prompt = await this.getSkillLoader().toPrompt(name);
      return success(prompt, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "TO_PROMPT", startTime);
    }
  }

  /**
   * Match Skills based on the description
   *
   * @param query The query string
   * @returns An array of matching results
   */
  async matchSkills(query: string): Promise<ExecutionResult<SkillMatchResult[]>> {
    const startTime = now();

    try {
      const results = this.getSkillRegistry().matchSkills(query);
      return success(results, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "MATCH_SKILLS", startTime);
    }
  }

  /**
   * List all resources for the Skill
   *
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Array of resource paths
   */
  async listResources(
    name: string,
    resourceType: SkillResourceType,
  ): Promise<ExecutionResult<string[]>> {
    const startTime = now();

    try {
      const resources = await this.getSkillRegistry().listSkillResources(name, resourceType);
      return success(resources, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "LIST_RESOURCES", startTime);
    }
  }

  /**
   * Reload all Skills
   *
   * @returns Execution results
   */
  async reload(): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      await this.getSkillRegistry().reload();
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "RELOAD", startTime);
    }
  }

  /**
   * Clear Cache
   *
   * @param name (Optional) Specifies the name of the Skill to be cleared
   */
  clearCache(name?: string): void {
    if (name) {
      this.getSkillLoader().clearCache(name);
    } else {
      this.getSkillLoader().clearCache();
      this.getSkillRegistry().clearCache();
    }
  }

  /**
   * Get the complete Skill object (including path information)
   *
   * @param name Skill name
   * @returns Skill object or null
   */
  async getFullSkill(name: string): Promise<Skill | null> {
    return this.getSkillRegistry().getSkill(name) || null;
  }

  // ============================================================================
  // Private method
  // ============================================================================

  /**
   * Get an instance of SkillRegistry
   */
  private getSkillRegistry() {
    return this.dependencies.getSkillRegistry();
  }

  /**
   * Get a SkillLoader instance
   */
  private getSkillLoader() {
    return this.dependencies.getSkillLoader();
  }
}
