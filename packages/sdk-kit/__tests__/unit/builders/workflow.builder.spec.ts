/**
 * Workflow Builder Tests
 */

import { describe, it, expect } from 'vitest';
import { WorkflowBuilder } from '@/builders/workflow.builder.js';
import { KitError, KitErrorCode } from '@/converters/error.converter.js';

describe('WorkflowBuilder', () => {
  it('should create a basic workflow', () => {
    const builder = new WorkflowBuilder('test-workflow');

    const template = builder
      .node('start', { type: 'START' })
      .node('task', { type: 'LLM' })
      .edge('start', 'task')
      .build();

    expect(template.id).toBe('test-workflow');
    expect(template.nodes).toHaveLength(2);
    expect(template.edges).toHaveLength(1);
  });

  it('should validate duplicate node IDs', () => {
    const builder = new WorkflowBuilder('test-workflow');

    builder.node('start', { type: 'START' });

    expect(() => {
      builder.node('start', { type: 'LLM' });
    }).toThrow(KitError);
  });

  it('should validate node type is required', () => {
    const builder = new WorkflowBuilder('test-workflow');

    expect(() => {
      builder.node('start', { type: '' });
    }).toThrow(KitError);
  });

  it('should validate node exists for edge', () => {
    const builder = new WorkflowBuilder('test-workflow');

    builder.node('start', { type: 'START' });

    expect(() => {
      builder.edge('start', 'nonexistent');
    }).toThrow(KitError);
  });

  it('should prevent duplicate edges', () => {
    const builder = new WorkflowBuilder('test-workflow');

    builder
      .node('start', { type: 'START' })
      .node('task', { type: 'LLM' })
      .edge('start', 'task');

    expect(() => {
      builder.edge('start', 'task');
    }).toThrow(KitError);
  });

  it('should require at least one node', () => {
    const builder = new WorkflowBuilder('test-workflow');

    expect(() => {
      builder.build();
    }).toThrow(KitError);
  });

  it('should support metadata', () => {
    const builder = new WorkflowBuilder('test-workflow');

    const template = builder
      .node('start', { type: 'START' })
      .metadata({ version: '1.0', author: 'test' })
      .build();

    expect(template.metadata).toEqual({ version: '1.0', author: 'test' });
  });

  it('should support name and description', () => {
    const builder = new WorkflowBuilder('test-workflow');

    const template = builder
      .node('start', { type: 'START' })
      .name('My Workflow')
      .description('Test workflow')
      .build();

    expect(template.name).toBe('My Workflow');
    expect(template.description).toBe('Test workflow');
  });

  it('should support node metadata', () => {
    const builder = new WorkflowBuilder('test-workflow');

    const template = builder
      .node('start', {
        type: 'START',
        name: 'Start Node',
        description: 'Entry point',
      })
      .build();

    expect(template.nodes[0].name).toBe('Start Node');
    expect(template.nodes[0].description).toBe('Entry point');
  });

  it('should chain methods fluently', () => {
    const builder = new WorkflowBuilder('test-workflow');

    const template = builder
      .node('n1', { type: 'START' })
      .node('n2', { type: 'LLM' })
      .node('n3', { type: 'END' })
      .edge('n1', 'n2')
      .edge('n2', 'n3')
      .name('Test')
      .metadata({ key: 'value' })
      .build();

    expect(template.nodes).toHaveLength(3);
    expect(template.edges).toHaveLength(2);
  });

  it('should return a copy of template on build', () => {
    const builder = new WorkflowBuilder('test-workflow');

    builder.node('n1', { type: 'START' });

    const template1 = builder.build();
    const template2 = builder.build();

    expect(template1).not.toBe(template2);
    expect(template1).toEqual(template2);
  });
});
