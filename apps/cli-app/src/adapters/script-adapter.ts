/**
 * Script Adapter
 * Encapsulates SDK API calls related to scripts
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import type { Script, ScriptExecutionOptions } from "@wf-agent/types";
import { parseScript } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Script Adapter
 */
export class ScriptAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Register script from file
   * @param filePath Configuration file path
   * @returns Script definition
   */
  async registerFromFile(filePath: string): Promise<Script> {
    return this.executeWithErrorHandling(async () => {
      // Use SDK to load the configuration.
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const script = parseScript(content, format);

      // Using an instance of the inherited SDK
      const api = this.sdk.scripts;
      await api.create(script);

      this.output.infoLog(`Script registered: ${script.name}`);
      return script;
    }, "Register script");
  }

  /**
   * Batch register scripts from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
   * @param options Load options
   * @returns Registration result
   */
  async registerFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: Script[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/scripts",
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return parseScript(content, format);
        },
        register: async (script) => {
          await this.sdk.scripts.create(script);
        },
        onSuccess: (script) => {
          this.output.infoLog(`Script registered: ${script.name}`);
        },
        onFailure: (file) => {
          this.output.errorLog(`Failed to register script: ${file}`);
        },
      });
    }, "Batch register scripts");
  }

  /**
   * List all scripts
   */
  async listScripts(filter?: Record<string, unknown>): Promise<Script[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      return await api.getAll(filter);
    }, "List scripts");
  }

  /**
   * Get script details
   */
  async getScript(id: string): Promise<Script> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      const script = await api.get(id);

      if (!script) {
        throw new CLINotFoundError(`Script not found: ${id}`, "Script", id);
      }

      return script as Script;
    }, "Get script");
  }

  /**
   * Delete script
   */
  async deleteScript(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      await api.delete(id);
      this.output.infoLog(`Script deleted: ${id}`);
    }, "Delete script");
  }

  /**
   * Update script
   */
  async updateScript(id: string, updates: Partial<Script>): Promise<Script> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      await api.update(id, updates);

      const script = await api.get(id);

      if (!script) {
        throw new CLINotFoundError(`Script not found: ${id}`, "Script", id);
      }

      this.output.infoLog(`Script updated: ${id}`);
      return script as Script;
    }, "Update script");
  }

  /**
   * Validate script configuration
   */
  async validateScript(filePath: string): Promise<{ valid: boolean; errors: string[] }> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const script = parseScript(content, format);

      // Using the validation feature of the SDK
      const api = this.sdk.scripts;
      const result = await api.validateScript(script.name);

      if (result.valid) {
        this.output.infoLog(`Script validated: ${filePath}`);
      } else {
        this.output.errorLog(`Script validation failed: ${filePath}`);
      }

      return result;
    }, "Validate script");
  }

  /**
   * Execute script
   */
  async executeScript(scriptName: string, options?: ScriptExecutionOptions): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.sdk.scripts.getService();
      const executor = this.sdk.getScriptExecutor();
      const result = await executor.execute(scriptName, options, registry);

      // Handling the Result type
      if (result.isErr()) {
        throw result.error;
      }

      this.output.infoLog(`Script executed: ${scriptName}`);
      return result.value;
    }, "Execute script");
  }
}
