/**
 * ProfileRegistryAPI - Profile Resource Management API
 * Encapsulates ProfileManager to provide LLM Profile management functionality.
 * Revised version: Inherits from GenericResourceAPI to enhance code reuse and consistency.
 */

import { now } from "@wf-agent/common-utils";
import { ProfileManager } from "../../../../core/llm/profile-manager.js";
import type { LLMProfile, LLMProvider } from "@wf-agent/types";
import {
  ValidationError,
  NotFoundError,
  ConfigurationValidationError,
  NodeTemplateNotFoundError,
} from "@wf-agent/types";
import { CrudResourceAPI } from "../generic-resource-api.js";
import { isSuccess, getData } from "../../types/execution-result.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";

/**
 * Profile template type
 */
export interface LLMProfileTemplate {
  /** Template Name */
  name: string;
  /** Template Description */
  description: string;
  /** TemplateProfile */
  profile: Partial<LLMProfile>;
}

/**
 * Profile Filter
 */
export interface LLMProfileFilter {
  /** Profile ID */
  id?: string;
  /** Profile Name */
  name?: string;
  /** LLM provider */
  provider?: LLMProvider;
  /** Model Name */
  model?: string;
}

/**
 * ProfileRegistryAPI - Profile Resource Management API
 *
 * Reconstruction Notes:
 * - Inherit from GenericResourceAPI to reuse common CRUD operations.
 * - Implement all abstract methods to integrate with ProfileManager.
 * - Maintain all existing API methods to ensure backward compatibility.
 * - Add enhancements such as caching, logging, and validation.
 */
export class LLMProfileRegistryAPI extends CrudResourceAPI<LLMProfile, string, LLMProfileFilter> {
  private profileManager: ProfileManager;
  private templates: Map<string, LLMProfileTemplate> = new Map();

  constructor() {
    super();
    this.profileManager = new ProfileManager();
    this.initializeTemplates();
  }

  /**
   * Get a single Profile
   * @param id Profile ID
   * @returns LLM Profile; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<LLMProfile | null> {
    const profile = this.profileManager.get(id);
    return profile || null;
  }

  /**
   * Get all Profiles
   * @returns List of Profiles
   */
  protected async getAllResources(): Promise<LLMProfile[]> {
    return this.profileManager.list();
  }

  /**
   * Create a Profile
   * @param resource LLM Profile configuration
   */
  protected async createResource(resource: LLMProfile): Promise<void> {
    this.profileManager.register(resource);
  }

  /**
   * Update Profile
   * @param id Profile ID
   * @param updates Updates to be applied
   */
  protected async updateResource(id: string, updates: Partial<LLMProfile>): Promise<void> {
    const profile = this.profileManager.get(id);
    if (!profile) {
      throw new NotFoundError(`Profile not found: ${id}`, "PROFILE", id, {
        availableProfiles: this.profileManager.list().map((p: LLMProfile) => p.id),
      });
    }

    // Merge and update
    const updatedProfile = { ...profile, ...updates };
    this.profileManager.remove(id);
    this.profileManager.register(updatedProfile);
  }

  /**
   * Delete Profile
   * @param id Profile ID
   */
  protected async deleteResource(id: string): Promise<void> {
    this.profileManager.remove(id);
  }

  /**
   * Apply filter criteria
   * @param resources Array of Profiles
   * @param filter Filter criteria
   * @returns Array of Profiles after filtering
   */
  protected override applyFilter(resources: LLMProfile[], filter: LLMProfileFilter): LLMProfile[] {
    return resources.filter(profile => {
      if (filter.id && !profile.id.includes(filter.id)) {
        return false;
      }
      if (filter.name && !profile.name.includes(filter.name)) {
        return false;
      }
      if (filter.provider && profile.provider !== filter.provider) {
        return false;
      }
      if (filter.model && !profile.model.includes(filter.model)) {
        return false;
      }
      return true;
    });
  }

  /**
   * Set the default Profile
   * @param profileId Profile ID
   */
  async setDefaultProfile(profileId: string): Promise<void> {
    this.profileManager.setDefault(profileId);
  }

  /**
   * Get the default Profile
   * @returns The default Profile; if it does not exist, return null
   */
  async getDefaultProfile(): Promise<LLMProfile | null> {
    const profile = this.profileManager.getDefault();
    return profile || null;
  }

  /**
   * Get the default Profile ID
   * @returns The default Profile ID; returns null if it does not exist
   */
  async getDefaultProfileId(): Promise<string | null> {
    const profile = this.profileManager.getDefault();
    return profile?.id || null;
  }

  /**
   * Verify Profile
   * @param profile LLM Profile configuration
   * @returns Verification result
   */
  async validateProfile(
    profile: Partial<LLMProfile>,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    if (!profile.id) {
      errors.push("Profile ID is required");
    }

    if (!profile.name) {
      errors.push("Profile name is required");
    }

    if (!profile.provider) {
      errors.push("Profile provider is required");
    }

    if (!profile.model) {
      errors.push("Profile model is required");
    }

    if (!profile.apiKey) {
      errors.push("Profile apiKey is required");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Export Profile
   * @param profileId Profile ID
   * @returns JSON string
   */
  async exportProfile(profileId: string): Promise<string> {
    const profile = await this.get(profileId);
    if (!profile) {
      throw new NotFoundError(`Profile not found: ${profileId}`, "PROFILE", profileId, {
        availableProfiles: this.profileManager.list().map((p: LLMProfile) => p.id),
      });
    }

    // Hide sensitive information
    const exportData = {
      ...profile,
      apiKey: "***HIDDEN***",
    };

    return JSON.stringify(exportData, null, 2);
  }

  /**
   * Import Profile
   * @param json JSON string
   * @returns Profile ID
   */
  async importProfile(json: string): Promise<string> {
    try {
      const profile = JSON.parse(json) as LLMProfile;

      // Verify Profile
      const validation = await this.validateProfile(profile);
      if (!validation.valid) {
        throw new ConfigurationValidationError(`Invalid profile: ${validation.errors.join(", ")}`, {
          configType: "llm",
          context: { validationErrors: validation.errors },
        });
      }

      // Check if the API Key has been hidden.
      if (profile.apiKey === "***HIDDEN***") {
        throw new ConfigurationValidationError("Cannot import profile with hidden API key", {
          configType: "llm",
          context: { profileId: profile.id, reason: "security" },
        });
      }

      const result = await this.create(profile);
      if (!isSuccess(result)) {
        const error = getErrorMessage(result);
        throw new ConfigurationValidationError(`Failed to import profile: ${error}`, {
          configType: "llm",
          context: { importError: error },
        });
      }

      return profile.id;
    } catch (error) {
      if (error instanceof ValidationError) {
        throw error;
      }
      throw new ConfigurationValidationError(
        `Failed to import profile: ${getErrorMessage(error)}`,
        {
          configType: "llm",
          configPath: "profile",
          context: { parseError: getErrorMessage(error) },
        },
      );
    }
  }

  /**
   * Batch import of Profiles
   * @param json: A JSON string in array format
   * @returns: An array of Profile IDs
   */
  async importProfiles(json: string): Promise<string[]> {
    try {
      const profiles = JSON.parse(json) as LLMProfile[];
      if (!Array.isArray(profiles)) {
        throw new ConfigurationValidationError("Invalid format: expected array of profiles", {
          configType: "llm",
          configPath: "profiles",
          context: { expectedType: "array", receivedType: typeof profiles },
        });
      }

      const profileIds: string[] = [];
      for (const profile of profiles) {
        const profileId = await this.importProfile(JSON.stringify(profile));
        profileIds.push(profileId);
      }

      return profileIds;
    } catch (error) {
      if (error instanceof ValidationError) {
        throw error;
      }
      throw new ConfigurationValidationError(
        `Failed to import profiles: ${getErrorMessage(error)}`,
        {
          configType: "llm",
          configPath: "profiles",
        },
      );
    }
  }

  /**
   * Export all Profiles
   * @returns JSON string
   */
  async exportAllProfiles(): Promise<string> {
    const result = await this.getAll();
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get profiles for export");
    }
    const profiles = getData(result) || [];
    const exportData = profiles.map((profile: LLMProfile) => ({
      ...profile,
      apiKey: "***HIDDEN***",
    }));

    return JSON.stringify(exportData, null, 2);
  }

  /**
   * Create a Profile from a template
   * @param templateName: Template name
   * @param overrides: Override configurations
   * @returns: Profile ID
   */
  async createFromTemplate(templateName: string, overrides: Partial<LLMProfile>): Promise<string> {
    const template = this.templates.get(templateName);
    if (!template) {
      throw new NodeTemplateNotFoundError(`Template not found: ${templateName}`, templateName, {
        templateName,
      });
    }

    // Merge templates and override configurations.
    const profile: LLMProfile = {
      id: overrides.id || `profile-${now()}`,
      name: overrides.name || template.name,
      provider: overrides.provider || template.profile.provider!,
      model: overrides.model || template.profile.model!,
      apiKey: overrides.apiKey || template.profile.apiKey!,
      baseUrl: overrides.baseUrl || template.profile.baseUrl,
      parameters: overrides.parameters || template.profile.parameters || {},
      headers: overrides.headers || template.profile.headers,
      timeout: overrides.timeout || template.profile.timeout,
      maxRetries: overrides.maxRetries || template.profile.maxRetries,
      retryDelay: overrides.retryDelay || template.profile.retryDelay,
      metadata: overrides.metadata || template.profile.metadata,
    };

    const result = await this.create(profile);
    if (!isSuccess(result)) {
      const error = getErrorMessage(result);
      throw new ConfigurationValidationError(`Failed to create profile from template: ${error}`, {
        configType: "llm",
        configPath: "profile",
      });
    }

    return profile.id;
  }

  /**
   * Get all templates
   * @returns List of templates
   */
  async getTemplates(): Promise<LLMProfileTemplate[]> {
    return Array.from(this.templates.values());
  }

  /**
   * Get the template
   * @param templateName: The name of the template
   * @returns: The template; returns null if it does not exist
   */
  async getTemplate(templateName: string): Promise<LLMProfileTemplate | null> {
    return this.templates.get(templateName) || null;
  }

  /**
   * Add a custom template
   * @param template The template
   */
  async addTemplate(template: LLMProfileTemplate): Promise<void> {
    this.templates.set(template.name, template);
  }

  /**
   * Delete the template
   * @param templateName Template name
   */
  async removeTemplate(templateName: string): Promise<void> {
    this.templates.delete(templateName);
  }

  /**
   * Get the underlying ProfileManager instance
   * @returns ProfileManager instance
   */
  getManager(): ProfileManager {
    return this.profileManager;
  }

  /**
   * Initialize the built-in templates
   */
  private initializeTemplates(): void {
    // OpenAI Chat template
    this.templates.set("openai-chat", {
      name: "OpenAI Chat",
      description: "OpenAI Chat API Configuration Template",
      profile: {
        id: "",
        name: "",
        provider: "OPENAI_CHAT" as const,
        model: "gpt-5",
        apiKey: "",
        parameters: {
          temperature: 0.7,
          maxTokens: 8192,
        },
      },
    });

    // Anthropic template
    this.templates.set("anthropic", {
      name: "Anthropic",
      description: "Anthropic Claude Configuration Template",
      profile: {
        id: "",
        name: "",
        provider: "ANTHROPIC" as const,
        model: "claude-4.5-opus",
        apiKey: "",
        parameters: {
          temperature: 0.7,
          maxTokens: 8192,
        },
      },
    });

    // Gemini template
    this.templates.set("gemini", {
      name: "Gemini",
      description: "Google Gemini Configuration Template",
      profile: {
        id: "",
        name: "",
        provider: "GEMINI_NATIVE" as const,
        model: "gemini-2.5-pro",
        apiKey: "",
        parameters: {
          temperature: 0.7,
          maxOutputTokens: 8192,
        },
      },
    });
  }
}
