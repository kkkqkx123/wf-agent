/**
 * Workflow Adapter
 *
 * Provides unified interface for workflow operations.
 * Handles listing, creating, updating, deleting, and analyzing workflows.
 *
 * Uses runtime's executeWithErrorHandling via BaseAdapter for consistent
 * error handling across all operations.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";
import { isSuccess, getError } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { parseWorkflow } from "@wf-agent/sdk/api";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { resolve } from "path";
import type { WorkflowTemplate } from "@wf-agent/types";

/**
 * Workflow input for creation/update
 */
export interface WorkflowInput {
  name: string;
  description?: string;
  config?: Record<string, any>;
  [key: string]: any;
}

/**
 * Workflow import parameters
 */
export interface WorkflowImportParams {
  /** Path to the config file */
  filePath: string;
  /** Optional runtime parameters for template substitution */
  parameters?: Record<string, unknown>;
}

/**
 * Workflow batch registration parameters
 */
export interface WorkflowBatchParams {
  /** Directory to scan */
  configDir: string;
  /** Whether to scan subdirectories (default: true) */
  recursive?: boolean;
  /** Optional file name pattern filter */
  filePattern?: RegExp;
  /** Optional runtime parameters for template substitution */
  parameters?: Record<string, unknown>;
}

/**
 * Workflow Adapter
 * Manages all workflow-related operations
 */
export class WorkflowAdapter extends BaseAdapter {
  /**
   * Get resource name for logging
   */
  override getResourceName(): string {
    return "Workflow";
  }

  /**
   * List all workflows
   */
  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const api = this.sdk.workflows;
      const workflows = await api.getAll();
      const items = workflows.map((wf: WorkflowTemplate) => ({
        id: wf.id,
        name: wf.name,
        type: wf.type || "unknown",
        description: wf.description,
        nodeCount: wf.nodes?.length || 0,
        edgeCount: wf.edges?.length || 0,
        createdAt: wf.createdAt,
        updatedAt: wf.updatedAt,
      }));
      return this.applyPagination(items, query);
    }, "list workflows");
  }

  /**
   * Get a single workflow
   */
  async get(id: string): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Workflow ID is required");
      }
      return await findByIdOrThrow(this.sdk.workflows, id, "Workflow");
    }, `get workflow ${id}`);
  }

  /**
   * Create a new workflow
   */
  async create(data: WorkflowInput): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("create", { name: data.name });
      if (!data.name || data.name.trim().length === 0) {
        throw new Error("Workflow name is required");
      }

      const workflow = {
        id: data.name.toLowerCase().replace(/\s+/g, "-"),
        name: data.name,
        description: data.description,
        nodes: [],
        edges: [],
        ...data.config,
      } as unknown as WorkflowTemplate;

      const result = await this.sdk.workflows.create(workflow);
      if (!isSuccess(result)) {
        throw getError(result) || new Error("Failed to create workflow");
      }

      return workflow;
    }, "create workflow");
  }

  /**
   * Update an existing workflow
   */
  async update(id: string, data: Partial<WorkflowInput>): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("update", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Workflow ID is required");
      }

      // Verify workflow exists
      const existing = await findByIdOrThrow(this.sdk.workflows, id, "Workflow");

      const updates: Partial<WorkflowTemplate> = {};
      if (data.name) updates.name = data.name;
      if (data.description !== undefined) updates.description = data.description;
      if (data.config) Object.assign(updates, data.config);

      const result = await this.sdk.workflows.update(id, updates);
      if (!isSuccess(result)) {
        throw getError(result) || new Error("Failed to update workflow");
      }

      return { ...existing, ...updates } as WorkflowTemplate;
    }, `update workflow ${id}`);
  }

  /**
   * Delete a workflow
   */
  async delete(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("delete", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Workflow ID is required");
      }

      // Verify workflow exists before deleting
      await findByIdOrThrow(this.sdk.workflows, id, "Workflow");

      const result = await this.sdk.workflows.delete(id);
      if (!isSuccess(result)) {
        throw getError(result) || new Error("Failed to delete workflow");
      }
    }, `delete workflow ${id}`);
  }

  /**
   * Register workflow from file
   */
  async registerFromFile(params: WorkflowImportParams): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromFile", { filePath: params.filePath });
      const fullPath = resolve(process.cwd(), params.filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const workflow = await parseWorkflow(content, format, params.parameters);

      const result = await this.sdk.workflows.create(workflow);
      if (!isSuccess(result)) {
        throw getError(result) || new Error("Failed to register workflow");
      }

      this.logOperation(`Workflow registered: ${workflow.name} (${workflow.id})`);
      return workflow;
    }, "register workflow from file");
  }

  /**
   * Batch register workflows from directory
   */
  async registerFromDirectory(params: WorkflowBatchParams): Promise<{
    success: WorkflowTemplate[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromDirectory", { configDir: params.configDir });
      return await batchRegisterFromDir({
        configDir: params.configDir,
        recursive: params.recursive,
        filePattern: params.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return await parseWorkflow(content, format, params.parameters);
        },
        register: async (workflow) => {
          const result = await this.sdk.workflows.create(workflow);
          if (!isSuccess(result)) {
            throw getError(result) || new Error("Registration failed");
          }
        },
        onSuccess: (workflow) => {
          this.logOperation(`Workflow registered: ${workflow.name} (${workflow.id})`);
        },
        onFailure: (file) => {
          this.logOperation(`Failed to register workflow: ${file}`);
        },
      });
    }, "batch register workflows");
  }

  /**
   * Get workflow graph
   */
  async getGraph(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getGraph", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Workflow ID is required");
      }

      // Verify workflow exists
      await findByIdOrThrow(this.sdk.workflows, id, "Workflow");

      const graph = await this.sdk.workflows.getWorkflowGraph(id);
      if (!graph) {
        throw new Error(`Graph not found for workflow: ${id}`);
      }
      return graph as Record<string, any>;
    }, `get workflow graph ${id}`);
  }
}