/**
 * Agent Profile Adapter
 * Encapsulates calls to the AgentProfileRegistry in the SDK DI container
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import type { AgentProfileMeta } from "@wf-agent/sdk/core";
import { AgentProfileRegistry, ServiceIdentifiers } from "@wf-agent/sdk/core";
import { loadConfigFile } from "@wf-agent/config-processor";
import { parseJson, parseToml } from "@wf-agent/sdk/api";
import { AgentLoopDefinitionSchema } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Agent Profile Adapter
 */
export class AgentProfileAdapter extends BaseAdapter {
  /**
   * Get the AgentProfileRegistry from the SDK's DI container
   */
  private getRegistry(): AgentProfileRegistry {
    const globalContext = this.sdk.getGlobalContext();
    const container = globalContext.container;
    const registry = container.get<AgentProfileRegistry>(ServiceIdentifiers.AgentProfileRegistry);
    if (!registry) {
      throw new Error("AgentProfileRegistry not available in SDK container");
    }
    return registry;
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

      const registry = this.getRegistry();
      registry.register(meta);

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
      registry.register(meta);
      this.output.infoLog(`Agent profile registered: ${meta.id}`);
    }, "Register agent profile");
  }

  /**
   * List all registered agent profiles
   */
  async listProfiles(): Promise<AgentProfileMeta[]> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      return registry.list();
    }, "List agent profiles");
  }

  /**
   * Get a specific agent profile by ID
   * @param id Agent profile ID
   */
  async getProfile(id: string): Promise<AgentProfileMeta> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getRegistry();
      const profile = registry.get(id);
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
      if (!registry.has(id)) {
        throw new CLINotFoundError(`Agent profile not found: ${id}`, "AgentProfile", id);
      }
      registry.remove(id);
      this.output.infoLog(`Agent profile deleted: ${id}`);
    }, "Delete agent profile");
  }
}