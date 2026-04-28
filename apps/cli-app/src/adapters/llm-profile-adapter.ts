/**
 * LLM Profile Adapter
 * Encapsulates SDK API calls related to LLM Profile
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve, join, extname } from "path";
import type { LLMProfile } from "@wf-agent/types";
import { loadConfigContent, parseLLMProfile } from "@wf-agent/sdk";

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
      const { content, format } = await loadConfigContent(fullPath);
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
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/llm-profiles";
      const files: string[] = [];

      const scanDir = async (currentDir: string) => {
        const entries = await readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = join(currentDir, entry.name);
          if (entry.isDirectory() && options.recursive !== false) {
            await scanDir(fullPath);
          } else if (entry.isFile()) {
            const ext = extname(entry.name).toLowerCase();
            if (ext === ".toml" || ext === ".json") {
              if (!options.filePattern || options.filePattern.test(entry.name)) {
                files.push(fullPath);
              }
            }
          }
        }
      };

      await scanDir(dir);

      const success: LLMProfile[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.profiles;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigContent(file);
          const profile = parseLLMProfile(content, format);
          await api.create(profile);
          success.push(profile);
          this.output.infoLog(`LLM Profile registered: ${profile.id}`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          this.output.errorLog(`Failed to register LLM Profile: ${file}`);
        }
      }

      return { success, failures };
    }, "Batch registration of LLM Profiles");
  }

  /**
   * List all LLM Profiles
   */
  async listProfiles(filter?: any): Promise<LLMProfile[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const result = await api.getAll(filter);
      const profiles = (result as any).data || result;
      return profiles as LLMProfile[];
    }, "List LLM Profiles");
  }

  /**
   * Get LLM Profile details
   */
  async getProfile(id: string): Promise<LLMProfile> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.profiles;
      const result = await api.get(id);
      const profile = (result as any).data || result;
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
      const result = await api.get(id);
      const profile = (result as any).data || result;
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
      const { content, format } = await loadConfigContent(fullPath);
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
