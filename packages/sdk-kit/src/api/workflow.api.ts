/**
 * Workflow API - Programmatic workflow definition
 *
 * Provides fluent API for workflow definition using Result pattern.
 * All operations return Result for composable error handling.
 */

import { WorkflowBuilder } from '../builders/workflow.builder.js';
import type { WorkflowTemplate } from '../types/workflow.types.js';
import type { Result } from '@wf-agent/types';
import { KitError, KitErrorCode } from '../converters/error.converter.js';
import { err, ok } from '@wf-agent/common-utils';

/**
 * Workflow API interface - Chainable workflow definition
 */
export interface WorkflowAPI {
  create(id: string): WorkflowBuilder;
  fromTemplate(template: WorkflowTemplate): Result<WorkflowBuilder, KitError>;
}

/**
 * Workflow API implementation
 */
export class WorkflowAPIImpl implements WorkflowAPI {
  create(id: string): WorkflowBuilder {
    return new WorkflowBuilder(id);
  }

  /**
   * Create builder from template
   *
   * Validates template structure and returns Result
   */
  fromTemplate(template: WorkflowTemplate): Result<WorkflowBuilder, KitError> {
    // Validate template structure
    if (!template || typeof template !== 'object') {
      return err(new KitError(
        'Template must be a valid object',
        KitErrorCode.VALIDATION_ERROR,
        { reason: 'invalid_template' }
      ));
    }

    if (!template.id || typeof template.id !== 'string') {
      return err(new KitError(
        'Template must contain a valid id',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'id' }
      ));
    }

    if (!Array.isArray(template.nodes)) {
      return err(new KitError(
        'Template must contain a nodes array',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'nodes' }
      ));
    }

    if (!Array.isArray(template.edges)) {
      return err(new KitError(
        'Template must contain an edges array',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'edges' }
      ));
    }

    const builder = new WorkflowBuilder(template.id);

    // Populate builder with template data using Result chaining
    for (const node of template.nodes) {
      const nodeResult = builder.node(node.id, {
        type: node.type,
        config: node.config,
        name: node.name,
        description: node.description,
      });

      if (nodeResult.isErr()) {
        return nodeResult as any;
      }
    }

    for (const edge of template.edges) {
      const edgeResult = builder.edge(edge.from, edge.to, edge.condition);

      if (edgeResult.isErr()) {
        return edgeResult as any;
      }
    }

    if (template.metadata) {
      builder.metadata(template.metadata);
    }

    if (template.name) {
      builder.name(template.name);
    }

    if (template.description) {
      builder.description(template.description);
    }

    return ok(builder);
  }
}


