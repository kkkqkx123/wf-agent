/**
 * Agent Profile Adapter
 * Encapsulates calls to the AgentProfileRegistry in the SDK DI container
 * with persistence via SqliteAgentProfileStorage.
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { parseJson, parseToml } from "@wf-agent/sdk/api";
import { AgentLoopDefinitionSchema } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Agent profile metadata stored in the registry
 */
interface AgentProfileMeta {
  id: string;
  name: string;
  description?: string;
}

/**
 * Agent Profile Adapter
 * Uses the SDK's AgentProfileRegistry (from DI container) which has
 * write-through persistence via SqliteAgentProfileStorage.
 */
export class AgentProfileAdapter extends BaseAdapter {
  private getRegistry() {
    return this.sdk.getFactory().getDependencies().getAgentProfileRegistry();
  }

  /**
   * Register an agent profile from a configuration file
   * @param filePath Agent configuration file path
   * @returns Registered agent profile metadata
   */
  async registerFromFile(filePath: string): Promise<AgentProfileMeta> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const raw: unknown =
        format === "toml" ? parseToml(content) : parseJson(content);
      const result = AgentLoopDefinitionSchema.safeParse(raw);
      if (!result.success) {
        const errors = result.error.issues.map((issue) => `${issue.path.join(".")}: ${issue.message}`).join("\n");
        throw new Error(`Agent loop config validation failed:\n${errors}`);
      }
      const config = result.data;
      const meta: AgentProfileMeta = {
        id: config.id,
        name: config.name || config.id,
        description: config.description,
      };

      // Persist via the registry (write-through to SQLite)
      const registry = this.getRegistry();
      await registry.registerProfile(meta);

      this.output.infoLog(`Agent profile registered: ${meta.id}`);
      return meta;
    }, "Register an agent profile");
  }

  /**
   * Register an agent profile directly from metadata
   * @param meta Agent profile metadata
   */
  async registerFromMeta(meta: AgentProfileMeta): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      await registry.registerProfile(meta);
      this.output.infoLog(`Agent profile registered: ${meta.id}`);
    }, "Register agent profile");
  }

  /**
   * List all registered agent profiles
   */
  async listProfiles(): Promise<AgentProfileMeta[]> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      // Registry.list() returns memory-cached profiles; storage is loaded
      // during SDK bootstrap via initializeFromStorage().
      return registry.list() as AgentProfileMeta[];
    }, "List agent profiles");
  }

  /**
   * Get a specific agent profile by ID
   * @param id Agent profile ID
   */
  async getProfile(id: string): Promise<AgentProfileMeta> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      const profile = registry.get(id) as AgentProfileMeta | undefined;
      if (!profile) {
        throw new CLINotFoundError(`Agent profile not found: ${id}`, "AgentProfile", id);
      }
      return profile;
    }, "Get agent profile");
  }

  /**
   * Delete an agent profile by ID
   * @param id Agent profile ID
   */
  async deleteProfile(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      await registry.removeProfile(id);
      this.output.infoLog(`Agent profile deleted: ${id}`);
    }, "Delete agent profile");
  }
}
