/**
 * LLM Profile Adapter
 * Encapsulates SDK API calls related to LLM Profile
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import type { LLMProfile } from "@wf-agent/types";
import { parseLLMProfile } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/config-processor";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * LLM Profile Adapter
 */
export class LLMProfileAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Register LLM Profile from file
   * @param filePath Configuration file path
   * @returns LLM Profile
   */
  async registerFromFile(filePath: string): Promise<LLMProfile> {
    return this.executeWithErrorHandling(async () => {
      // Use SDK to load the configuration.
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const profile = parseLLMProfile(content, format);

      // Using an instance of the inherited SDK
      const api = this.sdk.profiles;
      await api.create(profile);

      this.output.infoLog(`LLM Profile registered: ${profile.id}`);
      return profile;
    }, "Register an LLM Profile");
  }

  /**
   * Batch register LLM Profiles from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
   * @param options Load options
   * @returns Registration result
   */
  async registerFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: LLMProfile[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/llm-profiles",
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return parseLLMProfile(content, format);
        },
        register: async (profile) => {
          await this.sdk.profiles.create(profile);
        },
        onSuccess: (profile) => {
          this.output.infoLog(`LLM Profile registered: ${profile.id}`);
        },
        onFailure: (file) => {
          this.output.errorLog(`Failed to register LLM Profile: ${file}`);
        },
      });
    }, "Batch registration of LLM Profiles");
  }

  /**
   * List all LLM Profiles
   */
  async listProfiles(filter?: Record<string, unknown>): Promise<LLMProfile[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      return await api.getAll(filter);
    }, "List LLM Profiles");
  }

  /**
   * Get LLM Profile details
   */
  async getProfile(id: string): Promise<LLMProfile> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const profile = await api.get(id);

      if (!profile) {
        throw new CLINotFoundError(`LLM Profile not found: ${id}`, "LLMProfile", id);
      }

      return profile as LLMProfile;
    }, "Get LLM Profile");
  }

  /**
   * Delete LLM Profile
   */
  async deleteProfile(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      await api.delete(id);
      this.output.infoLog(`LLM Profile deleted: ${id}`);
    }, "Delete LLM Profile");
  }

  /**
   * Update LLM Profile
   */
  async updateProfile(id: string, updates: Partial<LLMProfile>): Promise<LLMProfile> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      await api.update(id, updates);

      const profile = await api.get(id);

      if (!profile) {
        throw new CLINotFoundError(`LLM Profile not found: ${id}`, "LLMProfile", id);
      }

      this.output.infoLog(`LLM Profile updated: ${id}`);
      return profile as LLMProfile;
    }, "Update LLM Profile");
  }

  /**
   * Validate LLM Profile configuration
   */
  async validateProfile(filePath: string): Promise<{ valid: boolean; errors: string[] }> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const profile = parseLLMProfile(content, format);

      const api = this.sdk.profiles;
      const result = await api.validateProfile(profile);

      if (result.valid) {
        this.output.infoLog(`LLM Profile validated: ${filePath}`);
      } else {
        this.output.errorLog(`LLM Profile validation failed: ${filePath}`);
      }

      return result;
    }, "Validate LLM Profile");
  }

  /**
   * Set default LLM Profile
   */
  async setDefaultProfile(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      await api.setDefaultProfile(id);
      this.output.infoLog(`Default LLM Profile set: ${id}`);
    }, "Set default LLM Profile");
  }

  /**
   * Get default LLM Profile
   */
  async getDefaultProfile(): Promise<LLMProfile | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const result = await api.getDefaultProfile();
      return result;
    }, "Get default LLM Profile");
  }

  /**
   * Export LLM Profile
   */
  async exportProfile(id: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const result = await api.exportProfile(id);
      return result;
    }, "Export LLM Profile");
  }

  /**
   * Import LLM Profile
   */
  async importProfile(json: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const profileId = await api.importProfile(json);
      this.output.infoLog(`LLM Profile imported: ${profileId}`);
      return profileId;
    }, "Import LLM Profile");
  }
}
