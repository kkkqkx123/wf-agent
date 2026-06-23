/**
 * Profile Manager
 *
 * Responsible for managing LLM Profiles, providing features such as configuration registration, querying, and deletion.
 * Supports the automatic management of default Profiles.
 */

import type { LLMProfile } from "@wf-agent/types";
import { NotFoundError, ConfigurationValidationError } from "@wf-agent/types";

/**
 * Profile Manager Class
 *
 * Manages the lifecycle of LLM Profiles, providing a unified interface for accessing configuration settings.
 */
export class ProfileManager {
  private profiles: Map<string, LLMProfile> = new Map();
  private defaultProfileId: string | null = null;

  /**
   * Register Profile
   *
   * @param profile LLM Profile configuration
   */
  register(profile: LLMProfile): void {
    // Verify Profile
    this.validateProfile(profile);

    // Store Profile
    this.profiles.set(profile.id, profile);

    // If it's the first Profile, set it as the default Profile.
    if (this.profiles.size === 1) {
      this.defaultProfileId = profile.id;
    }
  }

  /**
   * Get Profile
   *
   * @param profileId Profile ID; if not provided, the default Profile will be returned
   * @returns LLM Profile or undefined
   */
  get(profileId?: string): LLMProfile | undefined {
    if (!profileId) {
      return this.getDefault();
    }
    return this.profiles.get(profileId);
  }

  /**
   * Get the default Profile
   *
   * @returns The default Profile or undefined
   */
  getDefault(): LLMProfile | undefined {
    if (!this.defaultProfileId) {
      return undefined;
    }
    return this.profiles.get(this.defaultProfileId);
  }

  /**
   * Set the default Profile
   *
   * @param profileId Profile ID
   */
  setDefault(profileId: string): void {
    if (!this.profiles.has(profileId)) {
      throw new NotFoundError(`Profile not found: ${profileId}`, "PROFILE", profileId, {
        availableProfiles: Array.from(this.profiles.keys()),
      });
    }
    this.defaultProfileId = profileId;
  }

  /**
   * Delete Profile
   *
   * @param profileId Profile ID
   */
  remove(profileId: string): void {
    this.profiles.delete(profileId);

    // If the default Profile is deleted, reset the default Profile.
    if (this.defaultProfileId === profileId) {
      const firstProfile = this.profiles.values().next().value;
      this.defaultProfileId = firstProfile ? firstProfile.id : null;
    }
  }

  /**
   * List all Profiles
   *
   * @returns List of Profiles
   */
  list(): LLMProfile[] {
    return Array.from(this.profiles.values());
  }

  /**
   * Clear all Profiles
   */
  clear(): void {
    this.profiles.clear();
    this.defaultProfileId = null;
  }

  /**
   * Check if the Profile exists
   *
   * @param profileId Profile ID
   * @returns Whether it exists
   */
  has(profileId: string): boolean {
    return this.profiles.has(profileId);
  }

  /**
   * Get the number of Profiles
   *
   * @returns Number of Profiles
   */
  size(): number {
    return this.profiles.size;
  }

  /**
   * Verify Profile
   *
   * @param profile LLM Profile configuration
   */
  private validateProfile(profile: LLMProfile): void {
    if (!profile.id) {
      throw new ConfigurationValidationError("Profile ID is required", {
        configType: "llm",
        configPath: "profile.id",
      });
    }

    if (!profile.name) {
      throw new ConfigurationValidationError("Profile name is required", {
        configType: "llm",
        configPath: "profile.name",
      });
    }

    if (!profile.provider) {
      throw new ConfigurationValidationError("Profile provider is required", {
        configType: "llm",
        configPath: "profile.provider",
      });
    }

    if (!profile.model) {
      throw new ConfigurationValidationError("Profile model is required", {
        configType: "llm",
        configPath: "profile.model",
      });
    }

    if (!profile.apiKey) {
      throw new ConfigurationValidationError("Profile apiKey is required", {
        configType: "llm",
        configPath: "profile.apiKey",
      });
    }
  }
}
