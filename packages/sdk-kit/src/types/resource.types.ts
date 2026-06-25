/**
 * Resource Management Types - Defines interfaces for workflow resource operations
 */

import type { WorkflowTemplate } from './workflow.types.js';
import type { Result } from '@wf-agent/types';
import type { KitError } from '../converters/error.converter.js';

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
  create(template: WorkflowTemplate): Promise<Result<string, KitError>>;

  read(id: string): Promise<Result<WorkflowTemplate, KitError>>;

  update(id: string, template: Partial<WorkflowTemplate>): Promise<Result<void, KitError>>;

  delete(id: string): Promise<Result<void, KitError>>;

  list(filter?: ResourceFilter): Promise<Result<WorkflowTemplate[], KitError>>;

  clone(sourceId: string, targetId: string): Promise<Result<string, KitError>>;

  exists(id: string): Promise<Result<boolean, KitError>>;

  getVersion(id: string): Promise<Result<string, KitError>>;

  listVersions(id: string): Promise<Result<WorkflowVersion[], KitError>>;

  rollback(id: string, version: string): Promise<Result<void, KitError>>;

  getMetadata(id: string): Promise<Result<WorkflowMetadata, KitError>>;
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
