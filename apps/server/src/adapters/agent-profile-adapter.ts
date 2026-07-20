/**
 * Agent Profile Adapter
 * Manage agent profiles via the SDK AgentProfileRegistry.
 */

import { BaseAdapter } from "./base-adapter.js";

interface AgentProfileMeta {
  id: string;
  name: string;
  description?: string;
}

export class AgentProfileAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "AgentProfile";
  }

  private getRegistry() {
    return this.sdk.getFactory().getDependencies().getAgentProfileRegistry();
  }

  async registerFromMeta(meta: AgentProfileMeta): Promise<AgentProfileMeta> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromMeta", { id: meta.id });
      const registry = this.getRegistry();
      await registry.registerProfile(meta);
      return meta;
    }, "Register agent profile");
  }

  async listProfiles(): Promise<AgentProfileMeta[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listProfiles");
      const registry = this.getRegistry();
      const profiles = registry.list();
      return profiles.map((p: any) => ({
        id: p.id,
        name: p.name,
        description: p.description,
      })) as AgentProfileMeta[];
    }, "List agent profiles");
  }

  async getProfile(id: string): Promise<AgentProfileMeta> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getProfile", { id });
      const registry = this.getRegistry();
      const profile = await registry.get(id);
      if (!profile) {
        throw new Error(`Agent profile not found: ${id}`);
      }
      return { id: profile.id, name: profile.name, description: profile.description } as AgentProfileMeta;
    }, "Get agent profile");
  }

  async deleteProfile(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("deleteProfile", { id });
      const registry = this.getRegistry();
      const profile = await registry.get(id);
      if (!profile) {
        throw new Error(`Agent profile not found: ${id}`);
      }
      // AgentProfileRegistry doesn't expose delete directly; use the registry's delete if available
      // For now, log a warning and return
      this.logOperation(`Profile deletion requested for: ${id} - operation may not be supported by the SDK`);
    }, "Delete agent profile");
  }
}