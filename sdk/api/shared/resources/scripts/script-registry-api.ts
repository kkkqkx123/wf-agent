/**
 * ScriptRegistryAPI - Script Resource Management API
 * Encapsulates ScriptRegistry, providing functionality for script registration and querying.
 */

import {
  validateRequiredFields,
  validateStringLength,
  validateBoolean,
  validateEnum
} from '../../validation/validation-strategy.js';

import type { Script } from '@wf-agent/types';
import { NotFoundError } from '@wf-agent/types';
import { CrudResourceAPI } from '../generic-resource-api.js';
import type { APIDependencyManager } from '../../core/sdk-dependencies.js';
import type { ScriptFilter } from '../../types/code-types.js';

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
export class ScriptRegistryAPI extends CrudResourceAPI<Script, string, ScriptFilter> {
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
    const requiredResult = validateRequiredFields(script, ['name', 'type', 'description'], 'script');
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Please provide at least one validation item or file path.
    if (!script.content && !script.filePath) {
      errors.push('Either the script content or the file path must be provided.');
    }

    // Verify the name length.
    if (script.name) {
      const nameResult = validateStringLength(script.name, 'Script Name', 1, 100);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify the description length.
    if (script.description) {
      const descriptionResult = validateStringLength(script.description, 'Script Description', 1, 500);
      if (descriptionResult.isErr()) {
        errors.push(...descriptionResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify the `enabled` field (if provided).
    if (script.enabled !== undefined) {
      const enabledResult = validateBoolean(script.enabled, 'enabled');
      if (enabledResult.isErr()) {
        errors.push(...enabledResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify type enumeration values
    if (script.type) {
      const validTypes = ['javascript', 'typescript', 'python', 'shell'];
      const typeResult = validateEnum(script.type, 'Script Type', validTypes);
      if (typeResult.isErr()) {
        errors.push(...typeResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return {
      valid: errors.length === 0,
      errors
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
