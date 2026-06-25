/**
 * Resource API - High-level interface for workflow resource management
 */

import { ResourceManager } from '../managers/resource.manager.js';
import type { Result } from '@wf-agent/types';
import type { WorkflowResource, ResourceAPI, ResourceFilter, WorkflowVersion, WorkflowMetadata } from '../types/resource.types.js';
import type { WorkflowTemplate } from '../types/workflow.types.js';
import type { KitError } from '../converters/error.converter.js';

// Re-export types for convenience
export type { ResourceAPI, WorkflowResource, ResourceFilter } from '../types/resource.types.js';

/**
 * Workflow Resource implementation
 */
export class WorkflowResourceImpl implements WorkflowResource {
  private manager: ResourceManager;

  constructor(manager: ResourceManager) {
    this.manager = manager;
  }

  create(template: WorkflowTemplate): Promise<Result<string, KitError>> {
    return this.manager.createWorkflow(template);
  }

  read(id: string): Promise<Result<WorkflowTemplate, KitError>> {
    return this.manager.readWorkflow(id);
  }

  update(id: string, template: Partial<WorkflowTemplate>): Promise<Result<void, KitError>> {
    return this.manager.updateWorkflow(id, template);
  }

  delete(id: string): Promise<Result<void, KitError>> {
    return this.manager.deleteWorkflow(id);
  }

  list(filter?: ResourceFilter): Promise<Result<WorkflowTemplate[], KitError>> {
    return this.manager.listWorkflows(filter);
  }

  clone(sourceId: string, targetId: string): Promise<Result<string, KitError>> {
    return this.manager.cloneWorkflow(sourceId, targetId);
  }

  exists(id: string): Promise<Result<boolean, KitError>> {
    return this.manager.workflowExists(id);
  }

  getVersion(id: string): Promise<Result<string, KitError>> {
    return this.manager.getWorkflowVersion(id);
  }

  listVersions(id: string): Promise<Result<WorkflowVersion[], KitError>> {
    return this.manager.listWorkflowVersions(id);
  }

  rollback(id: string, version: string): Promise<Result<void, KitError>> {
    return this.manager.rollbackWorkflow(id, version);
  }

  getMetadata(id: string): Promise<Result<WorkflowMetadata, KitError>> {
    return this.manager.getWorkflowMetadata(id);
  }
}

/**
 * Resource API implementation
 */
export class ResourceAPIImpl implements ResourceAPI {
  private workflowResource: WorkflowResourceImpl;

  constructor(manager: ResourceManager) {
    this.workflowResource = new WorkflowResourceImpl(manager);
  }

  workflows(): WorkflowResource {
    return this.workflowResource;
  }
}
