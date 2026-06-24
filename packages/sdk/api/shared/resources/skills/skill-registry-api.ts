/**
 * SkillRegistryAPI - Skill Resource Management API
 * Encapsulates SkillRegistry, providing functions for skill registration, querying, and loading.
 *
 * Design Patterns:
 * - Inherits from GenericResourceAPI to provide unified read-only operations.
 * - Implements support for a progressive disclosure API layer.
 */

import {
  validateRequiredFields,
  validateStringLength,
} from "../../validation/validation-strategy.js";

import type { SkillMetadata, SkillResourceType, Skill } from "@wf-agent/types";
import { NotFoundError, ExecutionError, SDKError } from "@wf-agent/types";
import { QueryableResourceAPI } from "../generic-resource-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { ExecutionResult } from "../../types/execution-result.js";
import { success, failure } from "../../types/execution-result.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import type { DeleteCheckResult } from "../../../../shared/registry/types.js";

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
    variables?: Record<string, unknown>;
  };
  /** Whether to use caching */
  useCache?: boolean;
}

/**
 * SkillRegistryAPI - Skill Resource Management API
 *
 * Provides Skill CRUD operations and progressive disclosure support:
 * - Level 1: generateMetadataPrompt() / injectSkillMetadata() - metadata prompt
 * - Level 2: loadContent() - on-demand content loading
 * - Level 3: loadResources() - nested resource loading
 */
export class SkillRegistryAPI extends QueryableResourceAPI<SkillMetadata, string, SkillFilter> {
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
      const skill = this.getRegistry().getSkill(name);
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
    return this.getRegistry().getAllSkills();
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
    return this.getRegistry().generateMetadataPrompt();
  }

  /**
   * Inject skill metadata into a system prompt.
   *
   * @param systemPrompt Original system prompt
   * @returns System prompt with skill metadata injected
   */
  injectSkillMetadata(systemPrompt: string): string {
    return this.getRegistry().injectSkillMetadata(systemPrompt);
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
      const result = await this.getRegistry().loadContent(name, options?.context);

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
      const resources = await this.getRegistry().loadResources(
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
   * Convert skill to prompt format.
   *
   * @param name Skill name
   * @returns Execution result
   */
  async toPrompt(name: string): Promise<ExecutionResult<string>> {
    const startTime = now();

    try {
      const prompt = await this.getRegistry().toPrompt(name);
      return success(prompt, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "TO_PROMPT", startTime);
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
      const resources = await this.getRegistry().listSkillResources(name, resourceType);
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
      await this.getRegistry().reload();
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "RELOAD", startTime);
    }
  }

  /**
   * Enable a Skill by name
   *
   * @param name Skill name
   * @returns Execution result
   */
  async enable(name: string): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      this.getRegistry().enableSkill(name);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "ENABLE_SKILL", startTime);
    }
  }

  /**
   * Disable a Skill by name
   *
   * @param name Skill name
   * @returns Execution result
   */
  async disable(name: string): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      this.getRegistry().disableSkill(name);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "DISABLE_SKILL", startTime);
    }
  }

  /**
   * Get all enabled skills
   *
   * @returns Array of enabled Skill metadata
   */
  getEnabledSkills(): SkillMetadata[] {
    return this.getRegistry().getEnabledSkills();
  }

  /**
   * Get all disabled skills
   *
   * @returns Array of disabled Skill metadata
   */
  getDisabledSkills(): SkillMetadata[] {
    return this.getRegistry().getDisabledSkills();
  }

  /**
   * Clear Cache
   *
   * @param name (Optional) Specifies the name of the Skill to be cleared
   */
  clearCache(name?: string): void {
    if (name) {
      const skill = this.getRegistry().getSkill(name);
      if (skill) {
        skill.content = undefined;
      }
    } else {
      this.getRegistry().clearCache();
    }
  }

  /**
   * Get the complete Skill object (including path information)
   *
   * @param name Skill name
   * @returns Skill object or null
   */
  async getFullSkill(name: string): Promise<Skill | null> {
    return this.getRegistry().getSkill(name) || null;
  }

  /**
   * Scan and load skills from directory
   * This is a convenience method that encapsulates direct registry access
   *
   * @param skillsDir Directory path to scan
   * @returns Execution result
   */
  async scanSkills(skillsDir: string): Promise<ExecutionResult<void>> {
    try {
      const registry = this.getRegistry();
      await registry.scanSkills(skillsDir);
      return success(undefined, 0);
    } catch (error) {
      const sdkError =
        error instanceof Error
          ? new SDKError(error.message, "error", undefined, error)
          : new SDKError(String(error), "error");
      return failure(sdkError, 0);
    }
  }

  // ============================================================================
  // Private method
  // ============================================================================

  /**
   * Get an instance of SkillRegistry
   */
  private getRegistry() {
    return this.dependencies.getSkillRegistry();
  }

  /**
   * Check for workflow references to this skill before deletion.
   * Note: Skills are not currently referenced by workflow nodes, so this always returns safe.
   * @param _id Skill name
   * @returns Delete check result
   */
  async checkDeleteReferences(_id: string): Promise<DeleteCheckResult> {
    return {
      canDelete: true,
      details: "No references found",
      references: [],
    };
  }
}
