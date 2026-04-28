/**
 * Human Relay Adapter
 * Encapsulates SDK API calls related to Human Relay
 */

import { BaseAdapter } from "./base-adapter.js";
import type { HumanRelayConfig, HumanRelayFilter } from "@wf-agent/sdk";

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
      const configs = (result as any).data || result;
      return configs as HumanRelayConfig[];
    }, "List Human Relay Configurations");
  }

  /**
   * Get Human Relay configuration details
   */
  async getConfig(id: string): Promise<HumanRelayConfig> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.humanRelay;
      const result = await api.get(id);
      const config = (result as any).data || result;
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
      await api.update(id, updates);
      const result = await api.get(id);
      const config = (result as any).data || result;
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
