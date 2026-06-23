/**
 * Resource Manager Tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ResourceManager } from '@/managers/resource.manager.js';
import { KitErrorCode } from '@/converters/error.converter.js';
import type { WorkflowTemplate } from '@/types/workflow.types.js';

describe('ResourceManager', () => {
  let manager: ResourceManager;
  let mockRegistry: any;
  let sdk: any;

  const mockTemplate: WorkflowTemplate = {
    id: 'test-workflow',
    name: 'Test Workflow',
    description: 'A test workflow',
    nodes: [
      { id: 'start', type: 'START' },
      { id: 'task', type: 'LLM' },
    ],
    edges: [{ from: 'start', to: 'task' }],
  };

  beforeEach(() => {
    mockRegistry = {
      create: vi.fn(),
      get: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
      list: vi.fn(),
    };

    sdk = {
      getFactory: vi.fn(() => ({
        getWorkflowRegistry: vi.fn(() => mockRegistry),
      })),
    };

    manager = new ResourceManager(sdk);
  });

  describe('createWorkflow', () => {
    it('should create a workflow', async () => {
      mockRegistry.create.mockResolvedValue({
        success: true,
        data: 'test-workflow-id',
      });

      const result = await manager.createWorkflow(mockTemplate);

      expect(result).toBe('test-workflow-id');
      expect(mockRegistry.create).toHaveBeenCalledWith(mockTemplate);
    });

    it('should validate template has nodes', async () => {
      const invalidTemplate = { ...mockTemplate, nodes: [] };

      await expect(
        manager.createWorkflow(invalidTemplate as any)
      ).rejects.toThrow('at least one node');
    });

    it('should validate template is an object', async () => {
      await expect(manager.createWorkflow(null as any)).rejects.toThrow(
        'must be a valid object'
      );
    });

    it('should validate template has id', async () => {
      const invalidTemplate = { ...mockTemplate, id: null };

      await expect(manager.createWorkflow(invalidTemplate as any)).rejects.toThrow(
        'valid id'
      );
    });
  });

  describe('readWorkflow', () => {
    it('should read a workflow', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: mockTemplate,
      });

      const result = await manager.readWorkflow('test-workflow');

      expect(result).toEqual(mockTemplate);
      expect(mockRegistry.get).toHaveBeenCalledWith('test-workflow');
    });

    it('should validate workflow ID', async () => {
      await expect(manager.readWorkflow('')).rejects.toThrow(
        'non-empty string'
      );

      await expect(manager.readWorkflow(null as any)).rejects.toThrow(
        'non-empty string'
      );
    });
  });

  describe('updateWorkflow', () => {
    it('should update a workflow', async () => {
      mockRegistry.update.mockResolvedValue({
        success: true,
        data: undefined,
      });

      const updateData = { description: 'Updated description' };
      await manager.updateWorkflow('test-workflow', updateData);

      expect(mockRegistry.update).toHaveBeenCalledWith('test-workflow', updateData);
    });

    it('should validate workflow ID', async () => {
      await expect(
        manager.updateWorkflow('', { description: 'test' })
      ).rejects.toThrow('non-empty string');
    });

    it('should validate template is an object', async () => {
      await expect(
        manager.updateWorkflow('test-workflow', null as any)
      ).rejects.toThrow('valid object');
    });
  });

  describe('deleteWorkflow', () => {
    it('should delete a workflow', async () => {
      mockRegistry.delete.mockResolvedValue({
        success: true,
        data: undefined,
      });

      await manager.deleteWorkflow('test-workflow');

      expect(mockRegistry.delete).toHaveBeenCalledWith('test-workflow');
    });

    it('should validate workflow ID', async () => {
      await expect(manager.deleteWorkflow('')).rejects.toThrow(
        'non-empty string'
      );
    });
  });

  describe('listWorkflows', () => {
    it('should list workflows', async () => {
      const templates = [mockTemplate];
      mockRegistry.list.mockResolvedValue({
        success: true,
        data: templates,
      });

      const result = await manager.listWorkflows();

      expect(result).toEqual(templates);
      expect(mockRegistry.list).toHaveBeenCalledWith(undefined);
    });

    it('should list workflows with filter', async () => {
      const templates = [mockTemplate];
      const filter = { tag: 'production' };
      mockRegistry.list.mockResolvedValue({
        success: true,
        data: templates,
      });

      const result = await manager.listWorkflows(filter);

      expect(result).toEqual(templates);
      expect(mockRegistry.list).toHaveBeenCalledWith(filter);
    });
  });

  describe('cloneWorkflow', () => {
    it('should clone a workflow', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: mockTemplate,
      });

      mockRegistry.create.mockResolvedValue({
        success: true,
        data: 'cloned-workflow-id',
      });

      const result = await manager.cloneWorkflow(
        'test-workflow',
        'cloned-workflow'
      );

      expect(result).toBe('cloned-workflow-id');

      // Verify the cloned template has the correct structure
      const callArgs = mockRegistry.create.mock.calls[0][0];
      expect(callArgs.id).toBe('cloned-workflow');
      expect(callArgs.metadata?.clonedFrom).toBe('test-workflow');
    });

    it('should validate source ID', async () => {
      await expect(
        manager.cloneWorkflow('', 'target')
      ).rejects.toThrow('Source workflow ID');
    });

    it('should validate target ID', async () => {
      await expect(
        manager.cloneWorkflow('source', '')
      ).rejects.toThrow('Target workflow ID');
    });
  });

  describe('workflowExists', () => {
    it('should return true if workflow exists', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: mockTemplate,
      });

      const result = await manager.workflowExists('test-workflow');

      expect(result).toBe(true);
    });

    it('should return false if workflow does not exist', async () => {
      mockRegistry.get.mockRejectedValue(new Error('Not found'));

      const result = await manager.workflowExists('non-existent');

      expect(result).toBe(false);
    });

    it('should return false for invalid ID', async () => {
      const result = await manager.workflowExists('');

      expect(result).toBe(false);
    });
  });

  describe('getWorkflowVersion', () => {
    it('should get workflow version', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: {
          ...mockTemplate,
          metadata: { version: '2.0.0' },
        },
      });

      const result = await manager.getWorkflowVersion('test-workflow');

      expect(result).toBe('2.0.0');
    });

    it('should return default version if not specified', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: mockTemplate,
      });

      const result = await manager.getWorkflowVersion('test-workflow');

      expect(result).toBe('1.0.0');
    });
  });

  describe('listWorkflowVersions', () => {
    it('should list workflow versions', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: {
          ...mockTemplate,
          metadata: { version: '1.0.0' },
        },
      });

      const result = await manager.listWorkflowVersions('test-workflow');

      expect(Array.isArray(result)).toBe(true);
      expect(result.length).toBeGreaterThan(0);
      expect(result[0].version).toBe('1.0.0');
    });
  });

  describe('getWorkflowMetadata', () => {
    it('should get workflow metadata', async () => {
      mockRegistry.get.mockResolvedValue({
        success: true,
        data: mockTemplate,
      });

      const result = await manager.getWorkflowMetadata('test-workflow');

      expect(result).toBeDefined();
      expect(result.id).toBe('test-workflow');
      expect(result.name).toBe('Test Workflow');
      expect(result.description).toBe('A test workflow');
    });

    it('should validate workflow ID', async () => {
      await expect(manager.getWorkflowMetadata('')).rejects.toThrow(
        'non-empty string'
      );
    });
  });

  describe('error handling', () => {
    it('should handle SDK errors', async () => {
      mockRegistry.create.mockResolvedValue({
        success: false,
        error: { code: 'VALIDATION_ERROR', message: 'Invalid template' },
      });

      await expect(manager.createWorkflow(mockTemplate)).rejects.toThrow(
        'Invalid template'
      );
    });

    it('should handle missing registry', async () => {
      sdk.getFactory = vi.fn(() => ({
        getWorkflowRegistry: vi.fn(() => null),
      }));

      const newManager = new ResourceManager(sdk);

      await expect(newManager.createWorkflow(mockTemplate)).rejects.toThrow(
        'not available'
      );
    });
  });
});
