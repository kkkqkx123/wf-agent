/**
 * Resource API - High-level interface for workflow resource management
 */

import { ResourceManager } from '../managers/resource.manager.js';
import type { WorkflowResource, ResourceAPI, ResourceFilter } from '../types/resource.types.js';
import type { WorkflowTemplate } from '../types/workflow.types.js';

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

  create(template: WorkflowTemplate): Promise<string> {
    return this.manager.createWorkflow(template);
  }

  read(id: string): Promise<WorkflowTemplate> {
    return this.manager.readWorkflow(id);
  }

  update(id: string, template: Partial<WorkflowTemplate>): Promise<void> {
    return this.manager.updateWorkflow(id, template);
  }

  delete(id: string): Promise<void> {
    return this.manager.deleteWorkflow(id);
  }

  list(filter?: ResourceFilter): Promise<WorkflowTemplate[]> {
    return this.manager.listWorkflows(filter);
  }

  clone(sourceId: string, targetId: string): Promise<string> {
    return this.manager.cloneWorkflow(sourceId, targetId);
  }

  exists(id: string): Promise<boolean> {
    return this.manager.workflowExists(id);
  }

  getVersion(id: string): Promise<string> {
    return this.manager.getWorkflowVersion(id);
  }

  listVersions(id: string) {
    return this.manager.listWorkflowVersions(id);
  }

  rollback(id: string, version: string): Promise<void> {
    return this.manager.rollbackWorkflow(id, version);
  }

  getMetadata(id: string) {
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
