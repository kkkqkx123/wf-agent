/**
 * Workflow Builder - Programmatic workflow definition using Result pattern
 *
 * Design:
 * - Builder returns Result<this, KitError> to enable chaining
 * - build() collects and returns all validation errors at once
 * - No exceptions thrown during building
 * - Errors are values that can be handled via Result
 */

import { KitError, KitErrorCode } from '../converters/error.converter.js';
import type { WorkflowTemplate, Node, Edge, NodeConfig, EdgeCondition } from '../types/workflow.types.js';
import type { Result } from '@wf-agent/types';
import { ok, err } from '@wf-agent/common-utils';

/**
 * Workflow Builder implementation
 *
 * Methods return Result for error handling:
 * - Chainable: result.andThen(builder => builder.node(...))
 * - Safe: errors don't interrupt the chain
 * - Flexible: apply conditional operations with map/andThen
 */
export class WorkflowBuilder {
  private template: WorkflowTemplate;
  private nodes: Map<string, Node> = new Map();
  private edges: Edge[] = [];


  constructor(id: string) {
    this.template = {
      id,
      version: '1.0',
      nodes: [],
      edges: [],
    };
  }

  /**
   * Add a node to the workflow
   *
   * Returns Result for error handling without exceptions
   */
  node(id: string, config: NodeConfig): Result<this, KitError> {
    // Validate node ID uniqueness
    if (this.nodes.has(id)) {
      return err(new KitError(
        `Node with ID "${id}" already exists`,
        KitErrorCode.DUPLICATE_NODE_ID,
        { nodeId: id }
      ));
    }

    // Validate node type
    if (!config.type) {
      return err(new KitError(
        `Node type is required for node "${id}"`,
        KitErrorCode.VALIDATION_ERROR,
        { nodeId: id, field: 'type' }
      ));
    }

    const node: Node = {
      id,
      type: config.type,
      config: config.config || {},
      name: config.name,
      description: config.description,
    };

    this.nodes.set(id, node);
    return ok(this);  // ✅ Chain continues on success
  }

  /**
   * Add an edge between two nodes
   *
   * Returns Result for error handling
   */
  edge(from: string, to: string, condition?: EdgeCondition): Result<this, KitError> {
    // Validate source node exists
    if (!this.nodes.has(from)) {
      return err(new KitError(
        `Source node "${from}" not found in workflow`,
        KitErrorCode.NODE_NOT_FOUND,
        { nodeId: from, operation: 'edge_from' }
      ));
    }

    // Validate target node exists
    if (!this.nodes.has(to)) {
      return err(new KitError(
        `Target node "${to}" not found in workflow`,
        KitErrorCode.NODE_NOT_FOUND,
        { nodeId: to, operation: 'edge_to' }
      ));
    }

    // Validate no duplicate edges
    const edgeExists = this.edges.some(
      (e) => e.from === from && e.to === to
    );
    if (edgeExists) {
      return err(new KitError(
        `Edge from "${from}" to "${to}" already exists`,
        KitErrorCode.VALIDATION_ERROR,
        { from, to }
      ));
    }

    const edge: Edge = {
      from,
      to,
      condition: condition || {},
    };

    this.edges.push(edge);
    return ok(this);  // ✅ Chain continues on success
  }

  /**
   * Set workflow metadata
   */
  metadata(data: Record<string, unknown>): Result<this, KitError> {
    this.template.metadata = data;
    return ok(this);
  }

  /**
   * Set workflow name
   */
  name(name: string): Result<this, KitError> {
    if (!name || typeof name !== 'string' || name.trim().length === 0) {
      return err(new KitError(
        'Workflow name must be a non-empty string',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'name', value: name }
      ));
    }
    this.template.name = name;
    return ok(this);
  }

  /**
   * Set workflow description
   */
  description(description: string): Result<this, KitError> {
    this.template.description = description;
    return ok(this);
  }

  /**
   * Build and validate the workflow template
   *
   * Returns Result<WorkflowTemplate, KitError[]> to collect all validation errors
   */
  build(): Result<WorkflowTemplate, KitError[]> {
    const errors: KitError[] = [];

    // Validate workflow has nodes
    if (this.nodes.size === 0) {
      errors.push(new KitError(
        'Workflow must have at least one node',
        KitErrorCode.INVALID_WORKFLOW,
        { reason: 'no_nodes' }
      ));
    }

    // Validate workflow has edges if more than one node
    if (this.nodes.size > 1 && this.edges.length === 0) {
      errors.push(new KitError(
        'Workflow with multiple nodes must have at least one edge',
        KitErrorCode.INVALID_WORKFLOW,
        { reason: 'no_edges', nodeCount: this.nodes.size }
      ));
    }

    // Return all errors at once
    if (errors.length > 0) {
      return err(errors);
    }

    // Populate template
    this.template.nodes = Array.from(this.nodes.values());
    this.template.edges = this.edges;

    return ok(this.getTemplate());
  }

  /**
   * Get a copy of the current template
   */
  private getTemplate(): WorkflowTemplate {
    return {
      id: this.template.id,
      version: this.template.version,
      name: this.template.name,
      description: this.template.description,
      nodes: [...this.template.nodes],
      edges: [...this.template.edges],
      metadata: this.template.metadata ? { ...this.template.metadata } : undefined,
    };
  }
}
