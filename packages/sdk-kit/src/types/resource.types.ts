/**
 * Resource Management Types - Defines interfaces for workflow resource operations
 */

import type { WorkflowTemplate } from './workflow.types.js';

/**
 * Filter criteria for resource queries
 */
export interface ResourceFilter {
  tag?: string;
  name?: string;
  description?: string;
  createdAfter?: number;
  createdBefore?: number;
  modifiedAfter?: number;
  modifiedBefore?: number;
  [key: string]: unknown;
}

/**
 * Workflow version information
 */
export interface WorkflowVersion {
  version: string;
  createdAt: number;
  createdBy?: string;
  description?: string;
  changeLog?: string;
}

/**
 * Workflow metadata with timestamps
 */
export interface WorkflowMetadata {
  id: string;
  name: string;
  description?: string;
  createdAt: number;
  updatedAt: number;
  version: string;
  tags?: string[];
  author?: string;
}

/**
 * Workflow resource interface - Core CRUD operations
 */
export interface WorkflowResource {
  /**
   * Create a new workflow
   */
  create(template: WorkflowTemplate): Promise<string>;

  /**
   * Get a workflow by ID
   */
  read(id: string): Promise<WorkflowTemplate>;

  /**
   * Update an existing workflow
   */
  update(id: string, template: Partial<WorkflowTemplate>): Promise<void>;

  /**
   * Delete a workflow
   */
  delete(id: string): Promise<void>;

  /**
   * List all workflows with optional filtering
   */
  list(filter?: ResourceFilter): Promise<WorkflowTemplate[]>;

  /**
   * Clone a workflow with a new ID
   */
  clone(sourceId: string, targetId: string): Promise<string>;

  /**
   * Check if a workflow exists
   */
  exists(id: string): Promise<boolean>;

  /**
   * Get current version of a workflow
   */
  getVersion(id: string): Promise<string>;

  /**
   * List all versions of a workflow
   */
  listVersions(id: string): Promise<WorkflowVersion[]>;

  /**
   * Rollback to a specific version
   */
  rollback(id: string, version: string): Promise<void>;

  /**
   * Get workflow metadata
   */
  getMetadata(id: string): Promise<WorkflowMetadata>;
}

/**
 * Main resource API interface
 */
export interface ResourceAPI {
  workflows(): WorkflowResource;
  // Future extensions:
  // templates(): TemplateResource;
  // configs(): ConfigResource;
}
