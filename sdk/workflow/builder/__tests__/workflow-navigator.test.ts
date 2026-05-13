/**
 * Workflow Navigator Unit Tests
 * Tests for WorkflowNavigator class functionality
 */

import { describe, it, expect } from 'vitest';
import { WorkflowGraphBuilder } from '../workflow-graph-builder.js';
import { WorkflowNavigator } from '../workflow-navigator.js';
import type { WorkflowTemplate } from '@wf-agent/types';

describe('WorkflowNavigator', () => {
  function createNavigator(workflow: WorkflowTemplate): WorkflowNavigator {
    const graph = WorkflowGraphBuilder.build(workflow);
    return new WorkflowNavigator(graph);
  }

  describe('getNextNode', () => {
    it('should return START node when currentNodeId is undefined', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getNextNode();

      expect(result.nextNodeId).toBe('start');
      expect(result.isEnd).toBe(false);
      expect(result.hasMultiplePaths).toBe(false);
    });

    it('should return next node for single outgoing edge', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'node-a', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getNextNode('start');

      expect(result.nextNodeId).toBe('node-a');
      expect(result.isEnd).toBe(false);
      expect(result.hasMultiplePaths).toBe(false);
    });

    it('should detect END node when no outgoing edges', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getNextNode('end');

      expect(result.nextNodeId).toBeUndefined();
      expect(result.isEnd).toBe(true);
      expect(result.hasMultiplePaths).toBe(false);
    });

    it('should detect multiple paths when multiple outgoing edges', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getNextNode('start');

      expect(result.nextNodeId).toBeUndefined();
      expect(result.isEnd).toBe(false);
      expect(result.hasMultiplePaths).toBe(true);
      expect(result.possibleNextNodeIds).toContain('node-a');
      expect(result.possibleNextNodeIds).toContain('node-b');
    });
  });

  describe('routeNextNode', () => {
    it('should select DEFAULT edge when no conditions match', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'CONDITIONAL', condition: { field: 'x', operator: 'eq', value: 1 } as any },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.routeNextNode('start', () => false);

      expect(result).not.toBeNull();
      expect(result?.selectedNodeId).toBe('node-b');
      expect(result?.reason).toBe('DEFAULT_EDGE');
    });

    it('should select CONDITIONAL edge when condition matches', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'CONDITIONAL', condition: { field: 'x', operator: 'eq', value: 1 } as any, weight: 10 },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'CONDITIONAL', condition: { field: 'y', operator: 'eq', value: 2 } as any, weight: 5 },
        ],
      };

      const navigator = createNavigator(workflow);
      // First condition matches
      const result = navigator.routeNextNode('start', (condition) => {
        return (condition as any).field === 'x';
      });

      expect(result).not.toBeNull();
      expect(result?.selectedNodeId).toBe('node-a');
      expect(result?.reason).toBe('CONDITION_MATCHED');
    });

    it('should return null when no edges satisfy conditions', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'CONDITIONAL', condition: { field: 'x', operator: 'eq', value: 1 } as any },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.routeNextNode('start', () => false);

      expect(result).toBeNull();
    });

    it('should prioritize CONDITIONAL edges over DEFAULT edges by weight', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test priority',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
          { id: 'node-c', type: 'LLM', name: 'Node C', config: {} as any },
        ],
        edges: [
          // DEFAULT edge with high weight
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT', weight: 100 },
          // CONDITIONAL edge with lower weight but should be checked first
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'CONDITIONAL', condition: { field: 'x', operator: 'eq', value: 1 } as any, weight: 50 },
          // Another CONDITIONAL edge
          { id: 'e3', sourceNodeId: 'start', targetNodeId: 'node-c', type: 'CONDITIONAL', condition: { field: 'y', operator: 'eq', value: 2 } as any, weight: 30 },
        ],
      };

      const navigator = createNavigator(workflow);
      
      // When a CONDITIONAL edge matches, it should be selected even if DEFAULT has higher weight
      const result1 = navigator.routeNextNode('start', (condition) => {
        return (condition as any).field === 'x';
      });
      expect(result1).not.toBeNull();
      expect(result1?.selectedNodeId).toBe('node-b');
      expect(result1?.reason).toBe('CONDITION_MATCHED');

      // When no CONDITIONAL edge matches, DEFAULT should be used
      const result2 = navigator.routeNextNode('start', () => false);
      expect(result2).not.toBeNull();
      expect(result2?.selectedNodeId).toBe('node-a');
      expect(result2?.reason).toBe('DEFAULT_EDGE');
    });
  });

  describe('getPathTo', () => {
    it('should find path between two nodes', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'node-a', targetNodeId: 'node-b', type: 'DEFAULT' },
          { id: 'e3', sourceNodeId: 'node-b', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const path = navigator.getPathTo('start', 'end');

      expect(path).not.toBeNull();
      expect(path).toEqual(['start', 'node-a', 'node-b', 'end']);
    });

    it('should return null when path is unreachable', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const path = navigator.getPathTo('start', 'node-b');

      expect(path).toBeNull();
    });

    it('should return single node path when from and target are same', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
        ],
        edges: [],
      };

      const navigator = createNavigator(workflow);
      const path = navigator.getPathTo('start', 'start');

      expect(path).toEqual(['start']);
    });
  });

  describe('canReach', () => {
    it('should return true when target is reachable', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'node-a', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const canReach = navigator.canReach('start', 'end');

      expect(canReach).toBe(true);
    });

    it('should return false when target is not reachable', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const canReach = navigator.canReach('start', 'node-b');

      expect(canReach).toBe(false);
    });
  });

  describe('isForkNode/isJoinNode/isRouteNode', () => {
    it('should correctly identify FORK node', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'fork', type: 'FORK', name: 'Fork', config: {} as any },
        ],
        edges: [],
      };

      const navigator = createNavigator(workflow);
      expect(navigator.isForkNode('fork')).toBe(true);
      expect(navigator.isForkNode('non-existent')).toBe(false);
    });

    it('should correctly identify JOIN node', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'join', type: 'JOIN', name: 'Join', config: {} as any },
        ],
        edges: [],
      };

      const navigator = createNavigator(workflow);
      expect(navigator.isJoinNode('join')).toBe(true);
      expect(navigator.isJoinNode('non-existent')).toBe(false);
    });

    it('should correctly identify ROUTE node', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'route', type: 'ROUTE', name: 'Route', config: {} as any },
        ],
        edges: [],
      };

      const navigator = createNavigator(workflow);
      expect(navigator.isRouteNode('route')).toBe(true);
      expect(navigator.isRouteNode('non-existent')).toBe(false);
    });
  });

  describe('isEndNode/isStartNode', () => {
    it('should correctly identify END node', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      expect(navigator.isEndNode('end')).toBe(true);
      expect(navigator.isEndNode('start')).toBe(false);
    });

    it('should correctly identify START node', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      expect(navigator.isStartNode('start')).toBe(true);
      expect(navigator.isStartNode('end')).toBe(false);
    });
  });

  describe('getPredecessors/getSuccessors', () => {
    it('should get predecessor nodes', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const predecessors = navigator.getPredecessors('node-a');

      expect(predecessors).toContain('start');
      expect(predecessors.length).toBe(1);
    });

    it('should get successor nodes', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const successors = navigator.getSuccessors('start');

      expect(successors).toContain('node-a');
      expect(successors).toContain('node-b');
      expect(successors.length).toBe(2);
    });
  });

  describe('getAllExecutionPaths - Safety Limits', () => {
    it('should limit number of paths returned', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test path limits',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'fork', type: 'FORK', name: 'Fork', config: {} as any },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
          { id: 'node-c', type: 'LLM', name: 'Node C', config: {} as any },
          { id: 'join', type: 'JOIN', name: 'Join', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'fork', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'fork', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e3', sourceNodeId: 'fork', targetNodeId: 'node-b', type: 'DEFAULT' },
          { id: 'e4', sourceNodeId: 'fork', targetNodeId: 'node-c', type: 'DEFAULT' },
          { id: 'e5', sourceNodeId: 'node-a', targetNodeId: 'join', type: 'DEFAULT' },
          { id: 'e6', sourceNodeId: 'node-b', targetNodeId: 'join', type: 'DEFAULT' },
          { id: 'e7', sourceNodeId: 'node-c', targetNodeId: 'join', type: 'DEFAULT' },
          { id: 'e8', sourceNodeId: 'join', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getAllExecutionPaths('start', {
        maxPaths: 2
      });
      
      expect(result.paths.length).toBeLessThanOrEqual(2);
      expect(result.truncated).toBe(true);
      expect(result.reason).toBe('MAX_PATHS');
      expect(result.totalCount).toBeGreaterThan(2);
    });

    it('should respect maxDepth limit', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test depth limit',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
          { id: 'node-c', type: 'LLM', name: 'Node C', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'node-a', targetNodeId: 'node-b', type: 'DEFAULT' },
          { id: 'e3', sourceNodeId: 'node-b', targetNodeId: 'node-c', type: 'DEFAULT' },
          { id: 'e4', sourceNodeId: 'node-c', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getAllExecutionPaths('start', {
        maxDepth: 3 // Should stop before reaching END (needs 4 nodes)
      });
      
      expect(result.paths.length).toBe(0); // No complete paths
      expect(result.truncated).toBe(true);
      expect(result.reason).toBe('MAX_DEPTH');
    });

    it('should report progress via callback', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Test callback',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
          { id: 'node-b', type: 'LLM', name: 'Node B', config: {} as any },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
          { id: 'e2', sourceNodeId: 'start', targetNodeId: 'node-b', type: 'DEFAULT' },
          { id: 'e3', sourceNodeId: 'node-a', targetNodeId: 'end', type: 'DEFAULT' },
          { id: 'e4', sourceNodeId: 'node-b', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const foundPaths: string[][] = [];
      
      const result = navigator.getAllExecutionPaths('start', {
        onPathFound: (path, count) => {
          foundPaths.push(path);
          return count < 1; // Stop after 1 path
        }
      });
      
      expect(foundPaths.length).toBe(1);
      expect(result.truncated).toBe(true);
      expect(result.reason).toBe('USER_CANCELLED');
    });

    it('should handle graphs with no END nodes gracefully', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'No END node',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'node-a', type: 'LLM', name: 'Node A', config: {} as any },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'node-a', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getAllExecutionPaths('start', {
        maxDepth: 10
      });
      
      // No paths found because there's no END node
      expect(result.paths.length).toBe(0);
      // Not truncated - completed normally but found no END nodes
      expect(result.truncated).toBe(false);
      expect(result.reason).toBe('COMPLETE');
      expect(result.totalCount).toBe(0);
    });

    it('should enumerate paths with getAllExecutionPaths', () => {
      const workflow: WorkflowTemplate = {
        id: 'test-workflow',
        name: 'Test Workflow',
        description: 'Path enumeration',
        version: '1.0.0',
        type: 'STANDALONE',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: 'start', type: 'START', name: 'Start', config: {} },
          { id: 'end', type: 'END', name: 'End', config: {} },
        ],
        edges: [
          { id: 'e1', sourceNodeId: 'start', targetNodeId: 'end', type: 'DEFAULT' },
        ],
      };

      const navigator = createNavigator(workflow);
      const result = navigator.getAllExecutionPaths('start');
      
      expect(Array.isArray(result.paths)).toBe(true);
      expect(result.paths.length).toBe(1);
      expect(result.paths[0]).toEqual(['start', 'end']);
      expect(result.truncated).toBe(false);
      expect(result.reason).toBe('COMPLETE');
      expect(result.totalCount).toBe(1);
    });
  });
});
