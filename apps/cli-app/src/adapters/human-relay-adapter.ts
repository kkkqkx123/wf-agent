/**
 * Human Relay Adapter
 * Encapsulates SDK API calls related to Human Relay
 */

import { BaseAdapter } from "./base-adapter.js";
import type { HumanRelayConfig, HumanRelayFilter } from "@wf-agent/sdk/api";
import { getData, isFailure, getError } from "@wf-agent/sdk/api";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Human Relay Adapter
 */
export class HumanRelayAdapter extends BaseAdapter {
  /**
   * List all Human Relay configurations
   */
  async listConfigs(filter?: HumanRelayFilter): Promise<HumanRelayConfig[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      const result = await api.getAll(filter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as HumanRelayConfig[];
    }, "List Human Relay Configurations");
  }

  /**
   * Get Human Relay configuration details
   */
  async getConfig(id: string): Promise<HumanRelayConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const config = getData(result);
      if (!config) {
        throw new CLINotFoundError(`Human Relay configuration not found: ${id}`, "HumanRelay", id);
      }
      
      return config as HumanRelayConfig;
    }, "Get Human Relay Configuration");
  }

  /**
   * Create Human Relay configuration
   */
  async createConfig(config: HumanRelayConfig): Promise<HumanRelayConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      await api.create(config);
      this.output.infoLog(`Human Relay configuration created: ${config.id}`);
      return config;
    }, "Create a Human Relay Configuration");
  }

  /**
   * Update Human Relay configuration
   */
  async updateConfig(id: string, updates: Partial<HumanRelayConfig>): Promise<HumanRelayConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
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
        throw new CLINotFoundError(`Human Relay configuration not found: ${id}`, "HumanRelay", id);
      }
      
      this.output.infoLog(`Human Relay configuration updated: ${id}`);
      return config as HumanRelayConfig;
    }, "Update Human Relay Configuration");
  }

  /**
   * Delete Human Relay configuration
   */
  async deleteConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      await api.delete(id);
      this.output.infoLog(`Human Relay configuration deleted: ${id}`);
    }, "Delete the Human Relay Configuration");
  }

  /**
   * Enable Human Relay configuration
   */
  async enableConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      await api.update(id, { enabled: true });
      this.output.infoLog(`Human Relay configuration enabled: ${id}`);
    }, "Enable Human Relay Configuration");
  }

  /**
   * Disable Human Relay configuration
   */
  async disableConfig(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      await api.update(id, { enabled: false });
      this.output.infoLog(`Human Relay configuration disabled: ${id}`);
    }, "Disable Human Relay Configuration");
  }
}
