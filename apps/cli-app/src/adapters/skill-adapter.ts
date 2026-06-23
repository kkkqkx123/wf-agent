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
import type { SkillMetadata, SkillResourceType } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";
import { isSuccess, getData, getError } from "@wf-agent/sdk/api";

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
      await api.scanSkills(skillsDir);

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
      return await api.getAll(filter);
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
      return await api.get(name);
    }, "Get Skill");
  }

  /**
   * Load Skill full content (Progressive Disclosure Level 2)
   * @param name Skill name
   * @param variables Optional key-value pairs for template variable substitution
   * @returns Skill content
   */
  async loadContent(name: string, variables?: Record<string, unknown>): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.loadContent(name, {
        context: variables ? { variables } : undefined,
      });

      if (!isSuccess(result)) {
        throw getError(result);
      }

      const content = getData(result);
      if (!content) {
        throw new CLINotFoundError(`Skill not found: ${name}`, "Skill", name);
      }

      this.output.infoLog(`Skill content loaded: ${name}`);
      return content;
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

      if (!isSuccess(result)) {
        throw getError(result);
      }

      const resources = getData(result);
      if (!resources) {
        throw new CLINotFoundError(
          `Skill resource not found: ${name}, ${resourceType}`,
          "SkillResource",
          name,
        );
      }

      this.output.infoLog(`Skill resources loaded: ${name}, ${resourceType}`);
      return resources;
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

      if (!isSuccess(result)) {
        throw getError(result);
      }

      const prompt = getData(result);
      if (!prompt) {
        throw new CLINotFoundError(`Skill not found: ${name}`, "Skill", name);
      }

      return prompt;
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
   * List all resources of a Skill
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Resource path array
   */
  async listResources(name: string, resourceType: SkillResourceType): Promise<string[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      const result = await api.listResources(name, resourceType);

      if (!isSuccess(result)) {
        throw getError(result);
      }

      const resources = getData(result);
      if (!resources) {
        return [];
      }

      return resources;
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

  /**
   * Enable a Skill by name
   * @param name Skill name
   */
  async enable(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      await api.enable(name);

      this.output.infoLog(`Skill enabled: ${name}`);
    }, "Enable Skill");
  }

  /**
   * Disable a Skill by name
   * @param name Skill name
   */
  async disable(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      await api.disable(name);

      this.output.infoLog(`Skill disabled: ${name}`);
    }, "Disable Skill");
  }

  /**
   * Get enabled skills
   * @returns Enabled skills metadata array
   */
  async getEnabledSkills(): Promise<SkillMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      return api.getEnabledSkills();
    }, "Get Enabled Skills");
  }

  /**
   * Get disabled skills
   * @returns Disabled skills metadata array
   */
  async getDisabledSkills(): Promise<SkillMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.skills;
      return api.getDisabledSkills();
    }, "Get Disabled Skills");
  }
}
