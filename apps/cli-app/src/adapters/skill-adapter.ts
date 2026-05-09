/**
 * Skill Adapter
 * Encapsulates SDK API calls related to Skills
 *
 * Implements Progressive Disclosure:
 * - Level 1: generateMetadataPrompt() - Metadata prompt
 * - Level 2: loadContent() - Load content on demand
 * - Level 3: loadResources() - Load nested resources
 */

import { BaseAdapter } from "./base-adapter.js";
import type { SkillMetadata, SkillMatchResult, SkillResourceType } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError } from "@wf-agent/sdk";

/**
 * Skill Adapter
 */
export class SkillAdapter extends BaseAdapter {
  /**
   * Initialize Skill registry
   * Scan the configured Skill directory
   *
   * @param skillsDir Skill directory path
   */
  async initialize(skillsDir: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Use the public scanSkills API method
      const api = this.sdk.skills;
      const result = await api.scanSkills(skillsDir);
      
      if (isFailure(result)) {
        throw getError(result);
      }

      const skills = await this.listSkills();
      this.output.infoLog(`Initialized ${skills.length} Skill(s)`);
    }, "Initialize Skill");
  }

  /**
   * List all Skills
   * @param filter Filter conditions
   * @returns Skill metadata array
   */
  async listSkills(filter?: {
    name?: string;
    tags?: string[];
    version?: string;
  }): Promise<SkillMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.getAll(filter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as SkillMetadata[];
    }, "List Skills");
  }

  /**
   * Get Skill details
   * @param name Skill name
   * @returns Skill metadata
   */
  async getSkill(name: string): Promise<SkillMetadata | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.get(name);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as SkillMetadata | null;
    }, "Get Skill");
  }

  /**
   * Load Skill full content (Progressive Disclosure Level 2)
   * @param name Skill name
   * @returns Skill content
   */
  async loadContent(name: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.loadContent(name);

      if (isFailure(result)) {
        throw new CLINotFoundError(`Skill not found: ${name}`, "Skill", name);
      }

      this.output.infoLog(`Skill content loaded: ${name}`);
      return getData(result) as string;
    }, "Load Skill Content");
  }

  /**
   * Load Skill resources (Progressive Disclosure Level 3)
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Resource mapping
   */
  async loadResources(
    name: string,
    resourceType: SkillResourceType,
  ): Promise<Map<string, string | Buffer>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.loadResources(name, resourceType);

      if (isFailure(result)) {
        throw new CLINotFoundError(
          `Skill resource not found: ${name}, ${resourceType}`,
          "SkillResource",
          name,
        );
      }

      this.output.infoLog(`Skill resources loaded: ${name}, ${resourceType}`);
      return getData(result) as Map<string, string | Buffer>;
    }, "Load Skill Resources");
  }

  /**
   * Convert Skill to prompt format
   * @param name Skill name
   * @returns Prompt string
   */
  async toPrompt(name: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.toPrompt(name);

      if (isFailure(result)) {
        throw new CLINotFoundError(`Skill not found: ${name}`, "Skill", name);
      }

      return getData(result) as string;
    }, "Convert Skill to prompt");
  }

  /**
   * Generate Skill metadata prompt (Progressive Disclosure Level 1)
   * Used for injection into system prompt
   *
   * @returns Metadata prompt string
   */
  generateMetadataPrompt(): string {
    const api = this.sdk.skills;
    return api.generateMetadataPrompt();
  }

  /**
   * Match Skills by description
   * @param query Query string
   * @returns Match results
   */
  async matchSkills(query: string): Promise<SkillMatchResult[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.matchSkills(query);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as SkillMatchResult[];
    }, "Match Skills");
  }

  /**
   * List all resources of a Skill
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Resource path array
   */
  async listResources(name: string, resourceType: SkillResourceType): Promise<string[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.listResources(name, resourceType);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as string[];
    }, "List Skill Resources");
  }

  /**
   * Reload all Skills
   */
  async reload(): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      await api.reload();
      this.output.infoLog("All Skills have been reloaded");
    }, "Reload Skills");
  }

  /**
   * Clear cache
   * @param name Optional, specify the Skill name to clear
   */
  clearCache(name?: string): void {
    const api = this.sdk.skills;
    api.clearCache(name);
    this.output.infoLog(
      name ? `Skill cache cleared: ${name}` : "All Skill caches have been cleared",
    );
  }

  /**
   * Get full Skill object
   * @param name Skill name
   * @returns Skill object
   */
  async getFullSkill(name: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      return await api.getFullSkill(name);
    }, "Get full Skill");
  }

  /**
   * Inject Skill metadata into system prompt
   * Replace {SKILLS_METADATA} placeholder
   *
   * @param systemPrompt Original system prompt
   * @returns Injected system prompt
   */
  injectSkillsMetadata(systemPrompt: string): string {
    const metadataPrompt = this.generateMetadataPrompt();

    if (metadataPrompt) {
      const result = systemPrompt.replace("{SKILLS_METADATA}", metadataPrompt);
      this.output.infoLog("Skill metadata has been injected into the system prompt");
      return result;
    }

    // No Skill, remove placeholder
    return systemPrompt.replace("{SKILLS_METADATA}", "");
  }
}
