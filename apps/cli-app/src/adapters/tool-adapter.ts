/**
 * Tool Adapter
 * Encapsulates SDK API calls related to tools.
 * Uses runtime utils for batch registration and error handling.
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import type { Tool, ToolExecutionOptions } from "@wf-agent/types";
import { StaticValidatorAPI, parseToml, parseJson } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import type { ConfigurationValidationError } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";
import { ToolRegistry, type ToolRegistryConfig } from "../tools/index.js";
import type { SkillHandlerConfig } from "@wf-agent/sdk/resources";

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
      const { content, format } = await loadConfigFile(fullPath);
      const tool = (format === "toml" ? parseToml(content) : parseJson(content)) as unknown as Tool;

      // Using inherited sdk instances
      const api = this.sdk.tools;
      await api.create(tool);

      this.output.infoLog(`Tool registered: ${tool.id}`);
      return tool;
    }, "Register tool");
  }

  /**
   * Batch register tools from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
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
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/tools",
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return (format === "toml" ? parseToml(content) : parseJson(content)) as unknown as Tool;
        },
        register: async (tool) => {
          await this.sdk.tools.create(tool);
        },
        onSuccess: (tool) => {
          this.output.infoLog(`Tool registered: ${tool.id}`);
        },
        onFailure: (file) => {
          this.output.errorLog(`Failed to register tool: ${file}`);
        },
      });
    }, "Batch register tools");
  }

  /**
   * List all tools
   */
  async listTools(filter?: Record<string, unknown>): Promise<Tool[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      return await api.getAll(filter);
    }, "List tools");
  }

  /**
   * Get tool details
   */
  async getTool(id: string): Promise<Tool> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.tools;
      const tool = await api.get(id);

      if (!tool) {
        throw new CLINotFoundError(`Tool not found: ${id}`, "Tool", id);
      }

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

      const tool = await api.get(id);

      if (!tool) {
        throw new CLINotFoundError(`Tool not found: ${id}`, "Tool", id);
      }

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
      const { content, format } = await loadConfigFile(fullPath);
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
    parameters: Record<string, unknown>,
    options?: ToolExecutionOptions,
  ): Promise<unknown> {
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
    parameters: Record<string, unknown>,
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
  async registerBuiltinTools(skillConfig?: SkillHandlerConfig): Promise<{
    success: Tool[];
    failures: Array<{ toolId: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      // Register all built-in tools to the registry
      await this.toolRegistry.registerAll(skillConfig);

      const success: Tool[] = [];
      const failures: Array<{ toolId: string; error: string }> = [];

      const api = this.sdk.tools;
      const tools = this.toolRegistry.getAllSdkTools();

      for (const tool of tools) {
        try {
          await api.create(tool);
          success.push(tool);
          this.output.infoLog(`Built-in tool registered: ${tool.id}`);
        } catch (error) {
          failures.push({
            toolId: tool.id,
            error: error instanceof Error ? error.message : String(error),
          });
          this.output.errorLog(`Failed to register built-in tool: ${tool.id}`);
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
