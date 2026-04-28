/**
 * Tool Adapter
 * Encapsulates SDK API calls related to tools
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve, join, extname } from "path";
import type { Tool, ToolExecutionOptions } from "@wf-agent/types";
import { StaticValidatorAPI, loadConfigContent, parseToml, parseJson } from "@wf-agent/sdk";
import type { ConfigurationValidationError } from "@wf-agent/types";
import { ToolRegistry, type ToolRegistryConfig } from "../tools/index.js";

/**
 * Tool Adapter
 */
export class ToolAdapter extends BaseAdapter {
  private toolRegistry: ToolRegistry;

  constructor(registryConfig?: ToolRegistryConfig) {
    super();
    this.toolRegistry = new ToolRegistry(registryConfig);
  }

  /**
   * Register tool from file
   * @param filePath Configuration file path
   * @returns Tool definition
   */
  async registerFromFile(filePath: string): Promise<Tool> {
    return this.executeWithErrorHandling(async () => {
      // Loading Configuration with SDK
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigContent(fullPath);
      const tool = (format === "toml" ? parseToml(content) : parseJson(content)) as unknown as Tool;

      // Using inherited sdk instances
      const api = this.sdk.tools;
      await api.create(tool);

      this.output.infoLog(`Tool registered: ${tool.name}`);
      return tool;
    }, "Register tool");
  }

  /**
   * Batch register tools from directory
   * @param options Load options
   * @returns Registration result
   */
  async registerFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: Tool[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/tools";
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

      const success: Tool[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.tools;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigContent(file);
          const tool = (format === "toml"
            ? parseToml(content)
            : parseJson(content)) as unknown as Tool;
          await api.create(tool);
          success.push(tool);
          this.output.infoLog(`Tool registered: ${tool.name}`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          this.output.errorLog(`Failed to register tool: ${file}`);
        }
      }

      return { success, failures };
    }, "Batch register tools");
  }

  /**
   * List all tools
   */
  async listTools(filter?: any): Promise<Tool[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      const result = await api.getAll(filter);
      const tools = (result as any).data || result;
      return tools as Tool[];
    }, "List tools");
  }

  /**
   * Get tool details
   */
  async getTool(id: string): Promise<Tool> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      const result = await api.get(id);
      const tool = (result as any).data || result;
      return tool as Tool;
    }, "Get tool");
  }

  /**
   * Delete tool
   */
  async deleteTool(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      await api.delete(id);
      this.output.infoLog(`Tool deleted: ${id}`);
    }, "Delete tool");
  }

  /**
   * Update tool
   */
  async updateTool(id: string, updates: Partial<Tool>): Promise<Tool> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      await api.update(id, updates);
      const result = await api.get(id);
      const tool = (result as any).data || result;
      this.output.infoLog(`Tool updated: ${id}`);
      return tool as Tool;
    }, "Update tool");
  }

  /**
   * Validate tool configuration
   */
  async validateTool(filePath: string): Promise<{ valid: boolean; errors: string[] }> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigContent(fullPath);
      const tool = (format === "toml" ? parseToml(content) : parseJson(content)) as unknown as Tool;

      // Validating Tool Configurations with StaticValidatorAPI
      const validator = new StaticValidatorAPI();
      const result = validator.validateTool(tool);

      if (result.isErr()) {
        const errors = result.error.map((err: ConfigurationValidationError) => err.message);
        this.output.errorLog(`Tool validation failed: ${filePath}`);
        return { valid: false, errors };
      }

      this.output.infoLog(`Tool validated: ${filePath}`);
      return { valid: true, errors: [] };
    }, "Validate tool");
  }

  /**
   * Execute tool
   */
  async executeTool(
    toolId: string,
    parameters: Record<string, any>,
    options?: ToolExecutionOptions,
  ): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      const service = api.getService();
      const result = await service.execute(toolId, parameters, options);

      // Processing Result type
      if (result.isErr()) {
        throw result.error;
      }

      this.output.infoLog(`Tool executed: ${toolId}`);
      return result.value;
    }, "Execute tool");
  }

  /**
   * Validate tool parameters
   */
  async validateParameters(
    toolId: string,
    parameters: Record<string, any>,
  ): Promise<{ valid: boolean; errors: string[] }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      const result = await api.validateToolParameters(toolId, parameters);

      if (result.valid) {
        this.output.infoLog(`Tool parameters validated: ${toolId}`);
      } else {
        this.output.errorLog(`Tool parameters validation failed: ${toolId}`);
      }

      return result;
    }, "Validate tool parameters");
  }

  /**
   * Register all built-in tools
   * Register built-in tools to the SDK
   */
  async registerBuiltinTools(): Promise<{
    success: Tool[];
    failures: Array<{ toolId: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      // Register all built-in tools to the registry
      await this.toolRegistry.registerAll();

      const success: Tool[] = [];
      const failures: Array<{ toolId: string; error: string }> = [];

      const api = this.sdk.tools;
      const tools = this.toolRegistry.getAllSdkTools();

      for (const tool of tools) {
        try {
          await api.create(tool);
          success.push(tool);
          this.output.infoLog(`Built-in tool registered: ${tool.name}`);
        } catch (error) {
          failures.push({
            toolId: tool.id,
            error: error instanceof Error ? error.message : String(error),
          });
          this.output.errorLog(`Failed to register built-in tool: ${tool.name}`);
        }
      }

      return { success, failures };
    }, "Register built-in tools");
  }

  /**
   * Get tool registry
   */
  getToolRegistry(): ToolRegistry {
    return this.toolRegistry;
  }
}
