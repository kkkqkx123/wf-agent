/**
 * User Interaction Adapter
 * Encapsulates SDK API calls related to user interaction configurations
 */

import { BaseAdapter } from "./base-adapter.js";
import type { UserInteractionConfig, UserInteractionFilter } from "@wf-agent/sdk";
import { getData, isFailure, getError } from "@wf-agent/sdk";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * User Interaction Adapter
 */
export class UserInteractionAdapter extends BaseAdapter {
  /**
   * List all user interaction configurations
   */
  async listConfigs(filter?: UserInteractionFilter): Promise<UserInteractionConfig[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.getAll(filter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as UserInteractionConfig[];
    }, "List user interaction configurations");
  }

  /**
   * Get user interaction configuration details
   */
  async getConfig(id: string): Promise<UserInteractionConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const config = getData(result);
      if (!config) {
        throw new CLINotFoundError(
          `User interaction configuration not found: ${id}`,
          "UserInteraction",
          id
        );
      }
      
      return config as UserInteractionConfig;
    }, "Get user interaction configuration");
  }

  /**
   * Create user interaction configuration
   */
  async createConfig(config: UserInteractionConfig): Promise<UserInteractionConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.create(config);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`User interaction configuration created: ${config.id}`);
      return config;
    }, "Create user interaction configuration");
  }

  /**
   * Update user interaction configuration
   */
  async updateConfig(
    id: string,
    updates: Partial<UserInteractionConfig>
  ): Promise<UserInteractionConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const updateResult = await api.update(id, updates);
      
      if (isFailure(updateResult)) {
        throw getError(updateResult);
      }
      
      const getResult = await api.get(id);
      
      if (isFailure(getResult)) {
        throw getError(getResult);
      }
      
      const config = getData(getResult);
      if (!config) {
        throw new CLINotFoundError(
          `User interaction configuration not found: ${id}`,
          "UserInteraction",
          id
        );
      }
      
      this.output.infoLog(`User interaction configuration updated: ${id}`);
      return config as UserInteractionConfig;
    }, "Update user interaction configuration");
  }

  /**
   * Delete user interaction configuration
   */
  async deleteConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.delete(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`User interaction configuration deleted: ${id}`);
    }, "Delete user interaction configuration");
  }

  /**
   * Enable user interaction configuration
   */
  async enableConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.update(id, { enabled: true } as Partial<UserInteractionConfig>);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`User interaction configuration enabled: ${id}`);
    }, "Enable user interaction configuration");
  }

  /**
   * Disable user interaction configuration
   */
  async disableConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.userInteractions;
      const result = await api.update(id, { enabled: false } as Partial<UserInteractionConfig>);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`User interaction configuration disabled: ${id}`);
    }, "Disable user interaction configuration");
  }
}
