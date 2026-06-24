/**
 * ScriptRegistryAPI - Script Resource Management API
 * Encapsulates ScriptRegistry, providing functionality for script registration and querying.
 */

import {
  validateRequiredFields,
  validateStringLength,
  validateBoolean,
} from "../../validation/validation-strategy.js";

import type { Script } from "@wf-agent/types";
import { NotFoundError } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../generic-resource-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { ScriptFilter } from "../../types/code-types.js";
import type { DeleteCheckResult } from "../../../../shared/registry/types.js";

/**
 * ScriptRegistryAPI - Script Resource Management API
 *
 * Improvements:
 * - Inherits from GenericResourceAPI to reduce duplicate code
 * - Unified cache management
 * - Unified error handling
 * - Unified filtering logic
 * - Maintains backward compatibility
 */
export class ScriptRegistryAPI extends SimplifiedCrudResourceAPI<Script, string, ScriptFilter> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single script
   * @param id The name of the script
   * @returns The script definition; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<Script | null> {
    try {
      return this.dependencies.getScriptService().getScript(id);
    } catch (error) {
      if (error instanceof NotFoundError) {
        return null;
      }
      throw error;
    }
  }

  /**
   * Get all scripts
   * @returns Array of script definitions
   */
  protected async getAllResources(): Promise<Script[]> {
    return this.dependencies.getScriptService().listScripts();
  }

  /**
   * Create/Register Script
   * @param script Script definition
   */
  protected async createResource(script: Script): Promise<void> {
    this.dependencies.getScriptService().registerScript(script);
  }

  /**
   * Update the script
   * @param id: Script name
   * @param updates: Update content
   */
  protected async updateResource(id: string, updates: Partial<Script>): Promise<void> {
    this.dependencies.getScriptService().updateScript(id, updates);
  }

  /**
   * Delete the script
   * @param id Script name
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getScriptService().unregisterScript(id);
  }

  /**
   * Clear all scripts
   */
  protected override async clearResources(): Promise<void> {
    this.dependencies.getScriptService().clearScripts();
  }

  /**
   * Apply filter criteria
   * @param scripts Array of scripts
   * @param filter Filter criteria
   * @returns Array of scripts after filtering
   */
  protected override applyFilter(scripts: Script[], filter: ScriptFilter): Script[] {
    return scripts.filter(script => {
      if (filter.ids && !filter.ids.some(id => script.id.includes(id))) {
        return false;
      }
      if (filter.name && !script.name.includes(filter.name)) {
        return false;
      }
      if (filter.category && script.metadata?.category !== filter.category) {
        return false;
      }
      if (filter.tags && script.metadata?.tags) {
        if (!filter.tags.every(tag => script.metadata?.tags?.includes(tag))) {
          return false;
        }
      }
      if (filter.enabled !== undefined) {
        // Use the `enabled` field for filtering; the default value is `true`.
        const scriptEnabled = script.enabled ?? true;
        if (scriptEnabled !== filter.enabled) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Verify script definition
   * @param script: Script definition
   * @returns: Verification result
   */
  protected override async validateResource(
    script: Script,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Use a simplified validation tool to verify the required fields.
    const requiredResult = validateRequiredFields(script, ["name", "description"], "script");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Please provide at least one validation item or file path.
    if (!script.content && !script.filePath) {
      errors.push("Either the script content or the file path must be provided.");
    }

    // Verify the name length.
    if (script.name) {
      const nameResult = validateStringLength(script.name, "Script Name", 1, 100);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify the description length.
    if (script.description) {
      const descriptionResult = validateStringLength(
        script.description,
        "Script Description",
        1,
        500,
      );
      if (descriptionResult.isErr()) {
        errors.push(...descriptionResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify the `enabled` field (if provided).
    if (script.enabled !== undefined) {
      const enabledResult = validateBoolean(script.enabled, "enabled");
      if (enabledResult.isErr()) {
        errors.push(...enabledResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Search Script
   * @param query Search keyword
   * @returns Array of script definitions
   */
  async searchScripts(query: string): Promise<Script[]> {
    return this.dependencies.getScriptService().searchScripts(query);
  }

  /**
   * Verify Script
   * @param scriptName: Script name
   * @returns: Verification result
   */
  async validateScript(scriptName: string): Promise<{ valid: boolean; errors: string[] }> {
    try {
      const script = this.dependencies.getScriptService().getScript(scriptName);
      const isValid = this.dependencies.getScriptService().validateScript(script);
      return { valid: isValid, errors: [] };
    } catch (error) {
      return {
        valid: false,
        errors: [error instanceof Error ? error.message : "Unknown error"],
      };
    }
  }

  /**
   * Get the underlying ScriptRegistry instance
   * @returns ScriptRegistry instance
   */
  getService() {
    return this.dependencies.getScriptService();
  }

  /**
   * Check for workflow and flow blueprint references to this script before deletion
   * @param id Script name
   * @returns Delete check result
   */
  protected override async checkDeleteReferences(id: string): Promise<DeleteCheckResult> {
    const workflowRegistry = this.dependencies.getWorkflowRegistry();
    const scriptService = this.dependencies.getScriptService();
    const references: DeleteCheckResult["references"] = [];

    const summaries = await workflowRegistry.list();
    for (const summary of summaries) {
      const workflow = workflowRegistry.get(summary.id);
      if (!workflow) continue;

      const found: Array<{ type: string; details: string }> = [];

      const hasScriptRef = workflow.nodes.some(node => {
        if (node.type === "SCRIPT" && node.config.scriptName === id) return true;
        if (node.type === "INTERACTIVE_SCRIPT" && node.config.scriptName === id) return true;
        return false;
      });
      if (hasScriptRef) {
        found.push({
          type: "workflow_node",
          details: "Script is used in workflow node",
        });
      }

      const hasFlowRef = workflow.nodes.some(node => {
        if (node.type !== "SCRIPT") return false;
        return node.config.flowId && node.config.scriptName === id;
      });
      if (hasFlowRef) {
        found.push({
          type: "workflow_node",
          details: "Script is referenced as flow blueprint in workflow node",
        });
      }

      for (const ref of found) {
        references.push({
          type: ref.type,
          sourceId: workflow.id,
          sourceName: workflow.name,
          details: ref.details,
        });
      }
    }

    const flows = scriptService.listFlows();
    for (const flow of flows) {
      const referencesScript = flow.branches.some(branch =>
        branch.modules.some(module => module.key === id),
      );
      if (referencesScript) {
        references.push({
          type: "flow_blueprint",
          sourceId: flow.name,
          sourceName: flow.name,
          details: "Script is referenced as a module in flow blueprint",
        });
      }
    }

    return {
      canDelete: references.length === 0,
      details:
        references.length === 0
          ? "No references found"
          : `Referenced by ${references.length} workflow(s)/flow(s)`,
      references,
    };
  }

  /**
   * Enable the script
   * @param scriptName The name of the script
   */
  async enableScript(scriptName: string): Promise<void> {
    this.dependencies.getScriptService().enableScript(scriptName);
  }

  /**
   * Disable the script
   * @param scriptName The name of the script
   */
  async disableScript(scriptName: string): Promise<void> {
    this.dependencies.getScriptService().disableScript(scriptName);
  }

  /**
   * Check if the script is enabled.
   * @param scriptName The name of the script
   * @returns Whether it is enabled or not
   */
  async isScriptEnabled(scriptName: string): Promise<boolean> {
    return this.dependencies.getScriptService().isScriptEnabled(scriptName);
  }
}
