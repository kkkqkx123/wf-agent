/**
 * Resource Manager - Handles workflow CRUD operations using Result pattern
 *
 * Design:
 * - All methods return Result<T, KitError>
 * - No exception throwing in normal operations
 * - Errors are values that can be composed
 * - SDK errors are automatically converted to KitError
 */

import { ErrorConverter, KitError, KitErrorCode } from '../converters/error.converter.js';
import type { WorkflowTemplate } from '../types/workflow.types.js';
import type { ResourceFilter, WorkflowVersion, WorkflowMetadata } from '../types/resource.types.js';
import type { Result } from '@wf-agent/common-utils';
import { ok, err } from '@wf-agent/common-utils';

type SDKInstance = any;

/**
 * Resource Manager implementation - All methods return Result
 */
export class ResourceManager {
  private errorConverter: ErrorConverter;
  private sdk: SDKInstance;

  constructor(sdk: SDKInstance) {
    this.sdk = sdk;
    this.errorConverter = new ErrorConverter();
  }

  /**
   * Create a new workflow
   *
   * Returns Result<string, KitError> with workflow ID or error
   */
  async createWorkflow(template: WorkflowTemplate): Promise<Result<string, KitError>> {
    // Validate template first
    const validationResult = this.validateWorkflowTemplate(template);
    if (validationResult.isErr()) {
      // Combine multiple validation errors into single error
      const errors = validationResult.unwrapOrElse(e => e);
      return err(errors.length > 0 ? errors[0] : new KitError(
        'Workflow validation failed',
        KitErrorCode.VALIDATION_ERROR
      ));
    }

    try {
      const registry = this.sdk.getFactory().getWorkflowRegistry();
      if (!registry) {
        return err(new KitError(
          'Workflow registry not available',
          KitErrorCode.INTERNAL_ERROR
        ));
      }

      const result = await registry.create(template);
      return this.errorConverter.convertResult<string>(result);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Get a workflow by ID
   *
   * Returns Result<WorkflowTemplate, KitError>
   */
  async readWorkflow(id: string): Promise<Result<WorkflowTemplate, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id', value: id }
      ));
    }

    try {
      const registry = this.sdk.getFactory().getWorkflowRegistry();
      if (!registry) {
        return err(new KitError(
          'Workflow registry not available',
          KitErrorCode.INTERNAL_ERROR
        ));
      }

      const result = await registry.get(id);
      return this.errorConverter.convertResult<WorkflowTemplate>(result);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Update a workflow
   *
   * Returns Result<void, KitError>
   */
  async updateWorkflow(id: string, template: Partial<WorkflowTemplate>): Promise<Result<void, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    if (!template || typeof template !== 'object') {
      return err(new KitError(
        'Template must be a valid object',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'template' }
      ));
    }

    try {
      const registry = this.sdk.getFactory().getWorkflowRegistry();
      if (!registry) {
        return err(new KitError(
          'Workflow registry not available',
          KitErrorCode.INTERNAL_ERROR
        ));
      }

      const result = await registry.update(id, template);
      return this.errorConverter.convertResult<void>(result);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Delete a workflow
   *
   * Returns Result<void, KitError>
   */
  async deleteWorkflow(id: string): Promise<Result<void, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    try {
      const registry = this.sdk.getFactory().getWorkflowRegistry();
      if (!registry) {
        return err(new KitError(
          'Workflow registry not available',
          KitErrorCode.INTERNAL_ERROR
        ));
      }

      const result = await registry.delete(id);
      return this.errorConverter.convertResult<void>(result);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * List workflows with optional filtering
   *
   * Returns Result<WorkflowTemplate[], KitError>
   */
  async listWorkflows(filter?: ResourceFilter): Promise<Result<WorkflowTemplate[], KitError>> {
    try {
      const registry = this.sdk.getFactory().getWorkflowRegistry();
      if (!registry) {
        return err(new KitError(
          'Workflow registry not available',
          KitErrorCode.INTERNAL_ERROR
        ));
      }

      const result = await registry.list(filter);
      return this.errorConverter.convertResult<WorkflowTemplate[]>(result);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Clone a workflow
   */
  async cloneWorkflow(sourceId: string, targetId: string): Promise<Result<string, KitError>> {
    if (!sourceId || typeof sourceId !== 'string') {
      return err(new KitError(
        'Source workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'sourceId' }
      ));
    }

    if (!targetId || typeof targetId !== 'string') {
      return err(new KitError(
        'Target workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'targetId' }
      ));
    }

    try {
      // Get the source workflow
      const sourceResult = await this.readWorkflow(sourceId);
      if (sourceResult.isErr()) {
        return sourceResult as any;
      }

      const sourceWorkflow = sourceResult.unwrap();

      // Create a new workflow with the target ID
      const clonedTemplate: WorkflowTemplate = {
        ...sourceWorkflow,
        id: targetId,
        name: sourceWorkflow.name ? `${sourceWorkflow.name} (clone)` : `${targetId}`,
        metadata: {
          ...sourceWorkflow.metadata,
          clonedFrom: sourceId,
          clonedAt: Date.now(),
        },
      };

      return this.createWorkflow(clonedTemplate);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Check if a workflow exists
   */
  async workflowExists(id: string): Promise<boolean> {
    const result = await this.readWorkflow(id);
    return result.isOk();
  }

  /**
   * Get current version of a workflow
   */
  async getWorkflowVersion(id: string): Promise<Result<string, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    const result = await this.readWorkflow(id);
    if (result.isErr()) {
      return result as any;
    }

    const workflow = result.unwrap();
    const metadata = workflow.metadata as Record<string, unknown> | undefined;
    const version = (metadata?.['version'] as string | undefined) || '1.0.0';
    return ok(version);
  }

  /**
   * List workflow versions (stub implementation)
   */
  async listWorkflowVersions(id: string): Promise<Result<WorkflowVersion[], KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    try {
      const workflow = await this.readWorkflow(id);
      if (workflow.isErr()) {
        return workflow as any;
      }

      const wf = workflow.unwrap();
      const metadata = wf.metadata as Record<string, unknown> | undefined;
      const version = (metadata?.['version'] as string | undefined) || '1.0.0';

      return ok([
        {
          version,
          createdAt: Date.now(),
          description: 'Current version',
        },
      ]);
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Rollback to a specific version (stub implementation)
   */
  async rollbackWorkflow(id: string, version: string): Promise<Result<void, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    if (!version || typeof version !== 'string') {
      return err(new KitError(
        'Version must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'version' }
      ));
    }

    return err(new KitError(
      'Rollback not yet implemented',
      KitErrorCode.INTERNAL_ERROR
    ));
  }

  /**
   * Get workflow metadata
   */
  async getWorkflowMetadata(id: string): Promise<Result<WorkflowMetadata, KitError>> {
    if (!id || typeof id !== 'string') {
      return err(new KitError(
        'Workflow ID must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    try {
      const result = await this.readWorkflow(id);
      if (result.isErr()) {
        return result as any;
      }

      const workflow = result.unwrap();
      const now = Date.now();
      const metadata = workflow.metadata as Record<string, unknown> | undefined;
      const version = (metadata?.['version'] as string | undefined) || '1.0.0';
      const tags = (metadata?.['tags'] as string[] | undefined);
      const author = (metadata?.['author'] as string | undefined);

      return ok({
        id,
        name: workflow.name || id,
        description: workflow.description,
        createdAt: now,
        updatedAt: now,
        version,
        tags,
        author,
      });
    } catch (error) {
      return err(this.errorConverter.toKitError(error));
    }
  }

  /**
   * Validate workflow template
   *
   * Returns Result<void, KitError[]> to collect all validation errors
   */
  private validateWorkflowTemplate(template: any): Result<void, KitError[]> {
    const errors: KitError[] = [];

    if (!template || typeof template !== 'object') {
      errors.push(new KitError(
        'Template must be a valid object',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'template' }
      ));
    }

    if (!template?.id || typeof template?.id !== 'string') {
      errors.push(new KitError(
        'Template must have a valid id',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id', value: template?.id }
      ));
    }

    if (!Array.isArray(template?.nodes)) {
      errors.push(new KitError(
        'Template must have a nodes array',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'nodes' }
      ));
    }

    if (template?.nodes && template.nodes.length === 0) {
      errors.push(new KitError(
        'Template must have at least one node',
        KitErrorCode.INVALID_WORKFLOW,
        { reason: 'empty_nodes' }
      ));
    }

    if (!Array.isArray(template?.edges)) {
      errors.push(new KitError(
        'Template must have an edges array',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'edges' }
      ));
    }

    return errors.length > 0 ? err(errors) : ok(undefined);
  }
}
