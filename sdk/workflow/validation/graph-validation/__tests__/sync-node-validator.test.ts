/**
 * Sync Node Validator Unit Tests
 * 
 * Tests for validateSyncNodes function which validates:
 * - SYNC nodes must be within a FORK-JOIN branch structure
 * - sourcePathId and targetPathId must exist in parent FORK node's forkPaths
 * - variableMappings format is valid and consistent
 * - SYNC nodes are not isolated (have incoming/outgoing edges)
 * - Paired SYNC nodes have matching variable mappings (bidirectional validation)
 * - Data flow direction is unidirectional (no circular dependencies)
 */

import { describe, it, expect } from 'vitest';
import { validateSyncNodes } from '../sync-node-validator.js';
import { WorkflowGraphData } from '../../../entities/workflow-graph-data.js';
import type { WorkflowNode, WorkflowEdge } from '@wf-agent/types';

describe('validateSyncNodes', () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }>
  ): WorkflowGraphData {
    const graph = new WorkflowGraphData();
    
    // Add nodes
    for (const nodeData of nodes) {
      const node: WorkflowNode = {
        id: nodeData.id,
        type: nodeData.type as any,
        config: nodeData.config || {},
        workflowId: 'test-workflow',
        outgoingEdgeIds: [],
        incomingEdgeIds: [],
        originalNode: {
          id: nodeData.id,
          type: nodeData.type as any,
          name: `${nodeData.type}-${nodeData.id}`,
          config: nodeData.config || {},
        } as any,
      };
      graph.addNode(node);
    }
    
    // Add edges
    let edgeIndex = 0;
    for (const edgeData of edges) {
      const edge: WorkflowEdge = {
        id: `edge-${edgeIndex++}`,
        sourceNodeId: edgeData.sourceNodeId,
        targetNodeId: edgeData.targetNodeId,
        type: 'DEFAULT',
      };
      graph.addEdge(edge);
    }
    
    return graph;
  }

  describe('Valid SYNC node configurations', () => {
    it('should pass validation for a valid SYNC node', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b',
              variableMappings: [
                { externalName: 'var1', internalName: 'var2' }
              ]
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation without targetPathId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              variableMappings: []
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation without variableMappings', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b'
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe('SYNC node configuration errors', () => {
    it('should error when sourcePathId is missing', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              targetPathId: 'path-b'
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      // Now reports config error (and continues to check other validations)
      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(errors[0].message).toContain('missing required sourcePathId');
      expect(errors[0].context?.code).toBe('MISSING_SYNC_SOURCE_PATH_ID');
      // Note: sync1 has edges so it's not isolated, but other validations still run
    });

    it('should error when sourcePathId does not exist in any FORK', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'non-existent-path'
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('does not exist in any FORK node');
      expect(errors[0].context?.code).toBe('INVALID_SYNC_SOURCE_PATH_ID');
    });

    it('should error when targetPathId does not exist in any FORK', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'non-existent-path'
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('does not exist in any FORK node');
      expect(errors[0].context?.code).toBe('INVALID_SYNC_TARGET_PATH_ID');
    });

    it('should error when variableMapping has missing externalName', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              variableMappings: [
                { internalName: 'var1' }
              ]
            }
          },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('missing externalName');
      expect(errors[0].context?.code).toBe('MISSING_SYNC_MAPPING_EXTERNAL_NAME');
    });

    it('should error when variableMapping has missing internalName', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              variableMappings: [
                { externalName: 'var1' }
              ]
            }
          },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('missing internalName');
      expect(errors[0].context?.code).toBe('MISSING_SYNC_MAPPING_INTERNAL_NAME');
    });

    it('should error when variableMapping has duplicate externalName', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              variableMappings: [
                { externalName: 'var1', internalName: 'var2' },
                { externalName: 'var1', internalName: 'var3' }
              ]
            }
          },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('duplicate externalName');
      expect(errors[0].context?.code).toBe('DUPLICATE_SYNC_MAPPING_EXTERNAL_NAME');
    });

    it('should error when variableMapping has duplicate internalName', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              variableMappings: [
                { externalName: 'var1', internalName: 'var2' },
                { externalName: 'var3', internalName: 'var2' }
              ]
            }
          },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('duplicate internalName');
      expect(errors[0].context?.code).toBe('DUPLICATE_SYNC_MAPPING_INTERNAL_NAME');
    });

    it('should error when SYNC node is isolated', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a'
            }
          },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
          // sync1 has no edges
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('is isolated');
      expect(errors[0].context?.code).toBe('ISOLATED_SYNC_NODE');
    });
  });

  describe('SYNC node pairing validation', () => {
    it('should error when multiple SYNC nodes map same internalName from same source', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b',
              variableMappings: [
                { externalName: 'var1', internalName: 'shared' }
              ]
            }
          },
          { 
            id: 'sync2', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b',
              variableMappings: [
                { externalName: 'var2', internalName: 'shared' }
              ]
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync2' },
          { sourceNodeId: 'sync2', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('map to the same internalName');
      expect(errors[0].context?.code).toBe('CONFLICTING_SYNC_MAPPINGS');
    });

    it('should error when circular variable dependency detected', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b',
              variableMappings: [
                { externalName: 'varA', internalName: 'varB' }
              ]
            }
          },
          { 
            id: 'sync2', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-b',
              targetPathId: 'path-a',
              variableMappings: [
                { externalName: 'varB', internalName: 'varA' }
              ]
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-b', targetNodeId: 'sync2' },
          { sourceNodeId: 'sync2', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors.length).toBeGreaterThan(0);
      const circularError = errors.find(e => e.context?.code === 'CIRCULAR_SYNC_DEPENDENCY');
      expect(circularError).toBeDefined();
      expect(circularError?.message).toContain('Circular variable dependency');
    });
  });

  describe('Data flow direction validation', () => {
    it('should error when circular data flow detected', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { 
            id: 'fork1', 
            type: 'FORK',
            config: {
              forkPaths: [
                { pathId: 'path-a', childNodeId: 'node-a' },
                { pathId: 'path-b', childNodeId: 'node-b' },
                { pathId: 'path-c', childNodeId: 'node-c' },
              ]
            }
          },
          { id: 'node-a', type: 'SCRIPT' },
          { 
            id: 'sync1', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-a',
              targetPathId: 'path-b'
            }
          },
          { 
            id: 'sync2', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-b',
              targetPathId: 'path-c'
            }
          },
          { 
            id: 'sync3', 
            type: 'SYNC',
            config: {
              sourcePathId: 'path-c',
              targetPathId: 'path-a'
            }
          },
          { id: 'node-b', type: 'SCRIPT' },
          { id: 'node-c', type: 'SCRIPT' },
          { 
            id: 'join1', 
            type: 'JOIN',
            config: {
              forkPathIds: ['path-a', 'path-b', 'path-c']
            }
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'fork1' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-a' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-b' },
          { sourceNodeId: 'fork1', targetNodeId: 'node-c' },
          { sourceNodeId: 'node-a', targetNodeId: 'sync1' },
          { sourceNodeId: 'sync1', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-b', targetNodeId: 'sync2' },
          { sourceNodeId: 'sync2', targetNodeId: 'node-c' },
          { sourceNodeId: 'node-c', targetNodeId: 'sync3' },
          { sourceNodeId: 'sync3', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'join1' },
          { sourceNodeId: 'node-b', targetNodeId: 'join1' },
          { sourceNodeId: 'node-c', targetNodeId: 'join1' },
          { sourceNodeId: 'join1', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors.length).toBeGreaterThan(0);
      const cycleError = errors.find(e => e.context?.code === 'CIRCULAR_DATA_FLOW');
      expect(cycleError).toBeDefined();
      expect(cycleError?.message).toContain('Circular data flow');
    });
  });

  describe('No SYNC nodes', () => {
    it('should return empty errors when no SYNC nodes present', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { id: 'node-a', type: 'SCRIPT' },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'end' },
        ]
      );

      const errors = validateSyncNodes(graph);
      expect(errors).toHaveLength(0);
    });
  });
});
