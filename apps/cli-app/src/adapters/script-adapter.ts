/**
 * Script Adapter
 * Encapsulates SDK API calls related to scripts
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve, join, extname } from "path";
import type { Script, ScriptExecutionOptions } from "@wf-agent/types";
import { loadConfigContent, parseScript } from "@wf-agent/sdk";

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
      const { content, format } = await loadConfigContent(fullPath);
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
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/scripts";
      const files: string[] = [];

      const scanDir = async (currentDir: string) => {
        const entries = await readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = join(currentDir, entry.name);
          if (entry.isDirectory() && options.recursive !== false) {
            await scanDir(fullPath);
          } else if (entry.isFile()) {
            const ext = extname(entry.name).toLowerCase();
            if (ext === ".toml" || ext === ".json") {
              if (!options.filePattern || options.filePattern.test(entry.name)) {
                files.push(fullPath);
              }
            }
          }
        }
      };

      await scanDir(dir);

      const success: Script[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.scripts;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigContent(file);
          const script = parseScript(content, format);
          await api.create(script);
          success.push(script);
          this.output.infoLog(`Script registered: ${script.name}`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          this.output.errorLog(`Failed to register script: ${file}`);
        }
      }

      return { success, failures };
    }, "Batch register scripts");
  }

  /**
   * List all scripts
   */
  async listScripts(filter?: any): Promise<Script[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      const result = await api.getAll(filter);
      const scripts = (result as any).data || result;
      return scripts as Script[];
    }, "List scripts");
  }

  /**
   * Get script details
   */
  async getScript(id: string): Promise<Script> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      const result = await api.get(id);
      const script = (result as any).data || result;
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
      const result = await api.get(id);
      const script = (result as any).data || result;
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
      const { content, format } = await loadConfigContent(fullPath);
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
  async executeScript(scriptName: string, options?: ScriptExecutionOptions): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.scripts;
      const service = api.getService();
      const result = await service.execute(scriptName, options);

      // Handling the Result type
      if (result.isErr()) {
        throw result.error;
      }

      this.output.infoLog(`Script executed: ${scriptName}`);
      return result.value;
    }, "Execute script");
  }
}
