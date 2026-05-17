/**
 * SUBGRAPH Handler Unit Tests
 * Tests for subgraph-handler.ts functionality
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { subgraphHandler } from '../subgraph-handler.js';
import type { GlobalContext } from '../../../../../core/global-context.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, SubgraphNodeConfig } from '@wf-agent/types';
import { createContextualLogger } from '../../../../../utils/contextual-logger.js';

// Mock dependencies
const mockLogger = createContextualLogger({ component: 'test' });

const mockGlobalContext = {
  container: {
    get: vi.fn()
  }
} as unknown as GlobalContext;

const mockWorkflowExecutionEntity = {
  id: 'parent-execution-123',
  getWorkflowId: vi.fn().mockReturnValue('parent-workflow'),
  variableStateManager: {
    exportVariables: vi.fn()
  },
  unregisterChild: vi.fn(),
  getHierarchyMetadata: vi.fn().mockReturnValue({ depth: 0 }),
  getRootExecutionId: vi.fn().mockReturnValue('root-execution-123'),
  getRootExecutionType: vi.fn().mockReturnValue('WORKFLOW')
} as unknown as WorkflowExecutionEntity;

const mockSubgraphNode: StaticNodeOfType<'SUBGRAPH'> = {
  id: 'subgraph-node-1',
  type: 'SUBGRAPH',
  config: {
    subgraphId: 'child-workflow',
    variableInputs: [
      { externalName: 'parentVar1', internalName: 'childVar1', required: true },
      { externalName: 'parentVar2', internalName: 'childVar2', required: false, defaultValue: 'default' }
    ],
    variableOutputs: [
      { internalName: 'childResult', externalName: 'parentResult' }
    ]
  },
  originalNode: {
    id: 'subgraph-node-1',
    type: 'SUBGRAPH',
    config: {
      subgraphId: 'child-workflow',
      variableInputs: [
        { externalName: 'parentVar1', internalName: 'childVar1', required: true },
        { externalName: 'parentVar2', internalName: 'childVar2', required: false, defaultValue: 'default' }
      ],
      variableOutputs: [
        { internalName: 'childResult', externalName: 'parentResult' }
      ]
    }
  }
};

// Mock execution builder
const mockExecutionBuilder = {
  createSubgraph: vi.fn()
};

// Mock workflow executor
const mockWorkflowExecutor = {
  executeWorkflow: vi.fn()
};

// Mock execution hierarchy registry
const mockRegistry = {
  unregister: vi.fn()
};

// Mock subgraph entity
const mockSubgraphEntity = {
  id: 'subgraph-execution-456',
  variableStateManager: {
    importVariables: vi.fn()
  },
  stop: vi.fn(),
  getOutput: vi.fn().mockReturnValue({ result: 'success' }),
  getStatus: vi.fn().mockReturnValue('COMPLETED')
};

describe('subgraphHandler', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    
    // Setup mock returns
    (mockGlobalContext.container.get as any).mockImplementation((identifier: string) => {
      if (identifier === 'WorkflowExecutionBuilder') {
        return mockExecutionBuilder;
      }
      if (identifier === 'WorkflowExecutor') {
        return mockWorkflowExecutor;
      }
      if (identifier === 'ExecutionHierarchyRegistry') {
        return mockRegistry;
      }
      return null;
    });
    
    mockExecutionBuilder.createSubgraph.mockResolvedValue({
      workflowExecutionEntity: mockSubgraphEntity
    });
    
    mockWorkflowExecutor.executeWorkflow.mockResolvedValue({
      success: true,
      result: 'subgraph-executed'
    });
  });
  
  afterEach(() => {
    vi.resetAllMocks();
  });
  
  describe('successful execution', () => {
    it('should execute SUBGRAPH node successfully with variable mapping', async () => {
      // Act
      const result = await subgraphHandler(
        mockGlobalContext,
        mockWorkflowExecutionEntity,
        mockSubgraphNode
      );
      
      // Assert
      expect(mockExecutionBuilder.createSubgraph).toHaveBeenCalledWith(
        mockWorkflowExecutionEntity,
        {
          subgraphId: 'child-workflow',
          nodeId: 'subgraph-node-1',
          variableMapping: {
            inputs: mockSubgraphNode.config.variableInputs,
            outputs: mockSubgraphNode.config.variableOutputs
          },
          async: false
        }
      );
      
      expect(mockSubgraphEntity.variableStateManager.importVariables).toHaveBeenCalled();
      expect(mockWorkflowExecutor.executeWorkflow).toHaveBeenCalledWith(mockSubgraphEntity);
      expect(mockWorkflowExecutionEntity.variableStateManager.exportVariables).toHaveBeenCalledWith(
        mockSubgraphEntity.variableStateManager,
        mockSubgraphNode.config.variableOutputs
      );
      
      expect(result).toEqual({
        executionId: 'subgraph-execution-456',
        output: { result: 'success' },
        status: 'COMPLETED',
        executionResult: { success: true, result: 'subgraph-executed' }
      });
    });
    
    it('should handle SUBGRAPH node without variable mappings', async () => {
      // Arrange
      const nodeWithoutMappings: StaticNodeOfType<'SUBGRAPH'> = {
        ...mockSubgraphNode,
        config: {
          subgraphId: 'child-workflow'
        },
        originalNode: {
          ...mockSubgraphNode.originalNode!,
          config: {
            subgraphId: 'child-workflow'
          }
        }
      };
      
      // Act
      await subgraphHandler(
        mockGlobalContext,
        mockWorkflowExecutionEntity,
        nodeWithoutMappings
      );
      
      // Assert
      expect(mockExecutionBuilder.createSubgraph).toHaveBeenCalledWith(
        mockWorkflowExecutionEntity,
        {
          subgraphId: 'child-workflow',
          nodeId: 'subgraph-node-1',
          variableMapping: {
            inputs: [],
            outputs: []
          },
          async: false
        }
      );
      expect(mockWorkflowExecutionEntity.variableStateManager.exportVariables).not.toHaveBeenCalled();
    });
  });
  
  describe('error handling', () => {
    it('should throw error when subgraphId is missing', async () => {
      // Arrange
      const invalidNode: StaticNodeOfType<'SUBGRAPH'> = {
        ...mockSubgraphNode,
        config: {}
      };
      
      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockWorkflowExecutionEntity, invalidNode)
      ).rejects.toThrow("SUBGRAPH node 'subgraph-node-1' missing subgraphId configuration");
    });
    
    it('should throw error when WorkflowExecutionBuilder is not available', async () => {
      // Arrange
      (mockGlobalContext.container.get as any).mockReturnValue(null);
      
      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockWorkflowExecutionEntity, mockSubgraphNode)
      ).rejects.toThrow('WorkflowExecutionBuilder not available in DI container');
    });
    
    it('should throw error when WorkflowExecutor is not available', async () => {
      // Arrange
      (mockGlobalContext.container.get as any).mockImplementation((identifier: string) => {
        if (identifier === 'WorkflowExecutionBuilder') {
          return mockExecutionBuilder;
        }
        return null;
      });
      
      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockWorkflowExecutionEntity, mockSubgraphNode)
      ).rejects.toThrow('WorkflowExecutor not available in DI container');
    });
    
    it('should cleanup resources when subgraph execution fails', async () => {
      // Arrange
      const executionError = new Error('Subgraph execution failed');
      mockWorkflowExecutor.executeWorkflow.mockRejectedValue(executionError);
      
      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockWorkflowExecutionEntity, mockSubgraphNode)
      ).rejects.toThrow(`Subgraph execution failed for node 'subgraph-node-1': ${executionError.message}`);
      
      // Verify cleanup was called
      expect(mockSubgraphEntity.stop).toHaveBeenCalled();
      expect(mockRegistry.unregister).toHaveBeenCalledWith('subgraph-execution-456');
      expect(mockWorkflowExecutionEntity.unregisterChild).toHaveBeenCalledWith('subgraph-execution-456', 'WORKFLOW');
    });
    
    it('should handle cleanup errors gracefully', async () => {
      // Arrange
      const executionError = new Error('Subgraph execution failed');
      const cleanupError = new Error('Cleanup failed');
      mockWorkflowExecutor.executeWorkflow.mockRejectedValue(executionError);
      mockRegistry.unregister.mockImplementation(() => {
        throw cleanupError;
      });
      
      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockWorkflowExecutionEntity, mockSubgraphNode)
      ).rejects.toThrow(`Subgraph execution failed for node 'subgraph-node-1': ${executionError.message}`);
      
      // Verify cleanup was attempted even if it failed
      expect(mockSubgraphEntity.stop).toHaveBeenCalled();
      expect(mockRegistry.unregister).toHaveBeenCalled();
    });
  });
  
  describe('variable mapping edge cases', () => {
    it('should handle required input variable missing at runtime (error will be thrown by importVariables)', async () => {
      // This test verifies that the handler passes the variable mapping correctly
      // The actual validation happens in VariableManager.importVariables
      
      // Arrange - nothing special needed
      
      // Act
      await subgraphHandler(
        mockGlobalContext,
        mockWorkflowExecutionEntity,
        mockSubgraphNode
      );
      
      // Assert - just verify the mapping was passed
      expect(mockExecutionBuilder.createSubgraph).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({
          variableMapping: {
            inputs: mockSubgraphNode.config.variableInputs,
            outputs: mockSubgraphNode.config.variableOutputs
          }
        })
      );
    });
    
    it('should handle optional output variable that is undefined', async () => {
      // Arrange
      const nodeWithOptionalOutput: StaticNodeOfType<'SUBGRAPH'> = {
        ...mockSubgraphNode,
        config: {
          ...mockSubgraphNode.config,
          variableOutputs: [
            { internalName: 'optionalResult', externalName: 'parentOptionalResult' }
          ]
        }
      };
      
      // Act
      await subgraphHandler(
        mockGlobalContext,
        mockWorkflowExecutionEntity,
        nodeWithOptionalOutput
      );
      
      // Assert - exportVariables should be called even if variable might be undefined
      expect(mockWorkflowExecutionEntity.variableStateManager.exportVariables).toHaveBeenCalledWith(
        mockSubgraphEntity.variableStateManager,
        nodeWithOptionalOutput.config.variableOutputs
      );
    });
  });
});
