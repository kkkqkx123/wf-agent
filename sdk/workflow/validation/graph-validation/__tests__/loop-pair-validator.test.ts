/**
 * Loop Pair Validator Unit Tests
 *
 * Tests for validateLoopPairs function which validates:
 * - LOOP_START/LOOP_END pairing by loopId
 * - loopId uniqueness (no duplicate LOOP_START or LOOP_END with same loopId)
 * - LOOP_END's loopStartNodeId cross-references
 * - Graph topology: LOOP_START must have outgoing/incoming edges
 * - Graph topology: LOOP_END must have outgoing edges
 */

import { describe, it, expect } from 'vitest';
import { validateLoopPairs } from '../loop-pair-validator.js';
import { WorkflowGraphData } from '../../../entities/workflow-graph-data.js';
import type { WorkflowNode, WorkflowEdge } from '@wf-agent/types';

describe('validateLoopPairs', () => {
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

  describe('Valid LOOP_START/LOOP_END pairs', () => {
    it('should pass validation for a valid loop pair', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'loop-1' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation with loopStartNodeId set', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: {
              loopId: 'loop-1',
              loopStartNodeId: 'loop-start-1',
            },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      expect(errors).toHaveLength(0);
    });

    it('should validate multiple independent loop pairs', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-a', maxIterations: 5 },
          },
          { id: 'node-a', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'loop-a' },
          },
          {
            id: 'loop-start-2',
            type: 'LOOP_START',
            config: { loopId: 'loop-b', maxIterations: 3 },
          },
          { id: 'node-b', type: 'SCRIPT' },
          {
            id: 'loop-end-2',
            type: 'LOOP_END',
            config: { loopId: 'loop-b' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'loop-start-2' },
          { sourceNodeId: 'loop-start-2', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-b', targetNodeId: 'loop-end-2' },
          { sourceNodeId: 'loop-end-2', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe('Missing loopId errors', () => {
    it('should error when LOOP_START has no loopId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { maxIterations: 10 },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
        ]
      );

      const errors = validateLoopPairs(graph);
      expect(errors.length).toBeGreaterThanOrEqual(1);
      const missingError = errors.find(e => e.context?.['code'] === 'LOOP_START_MISSING_LOOP_ID');
      expect(missingError).toBeDefined();
      expect(missingError!.message).toContain('loopId');
    });

    it('should error when LOOP_END has no loopId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: {},
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const missingError = errors.find(e => e.context?.['code'] === 'LOOP_END_MISSING_LOOP_ID');
      expect(missingError).toBeDefined();
      expect(missingError!.message).toContain('loopId');
    });
  });

  describe('Duplicate loopId errors', () => {
    it('should error when two LOOP_START nodes share the same loopId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'dup-loop', maxIterations: 5 },
          },
          { id: 'node-a', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'dup-loop' },
          },
          {
            id: 'loop-start-2',
            type: 'LOOP_START',
            config: { loopId: 'dup-loop', maxIterations: 3 },
          },
          { id: 'node-b', type: 'SCRIPT' },
          {
            id: 'loop-end-2',
            type: 'LOOP_END',
            config: { loopId: 'dup-loop' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'loop-start-2' },
          { sourceNodeId: 'loop-start-2', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-b', targetNodeId: 'loop-end-2' },
          { sourceNodeId: 'loop-end-2', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const duplicateError = errors.find(e => e.context?.['code'] === 'DUPLICATE_LOOP_START_LOOP_ID');
      expect(duplicateError).toBeDefined();
      expect(duplicateError!.message).toContain('dup-loop');
    });

    it('should error when two LOOP_END nodes share the same loopId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'dup-loop', maxIterations: 5 },
          },
          { id: 'node-a', type: 'SCRIPT' },
          { id: 'node-b', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'dup-loop' },
          },
          {
            id: 'loop-end-2',
            type: 'LOOP_END',
            config: { loopId: 'dup-loop' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'node-a' },
          { sourceNodeId: 'node-a', targetNodeId: 'node-b' },
          { sourceNodeId: 'node-b', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'loop-end-2' },
          { sourceNodeId: 'loop-end-2', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const duplicateError = errors.find(e => e.context?.['code'] === 'DUPLICATE_LOOP_END_LOOP_ID');
      expect(duplicateError).toBeDefined();
      expect(duplicateError!.message).toContain('dup-loop');
    });
  });

  describe('Unpaired loop errors', () => {
    it('should error when LOOP_START has no matching LOOP_END', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'orphan-loop', maxIterations: 5 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const unpairedError = errors.find(e => e.context?.['code'] === 'UNPAIRED_LOOP_START');
      expect(unpairedError).toBeDefined();
      expect(unpairedError!.message).toContain('has no matching LOOP_END');
    });

    it('should error when LOOP_END has no matching LOOP_START', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'ghost-loop' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const unpairedError = errors.find(e => e.context?.['code'] === 'UNPAIRED_LOOP_END');
      expect(unpairedError).toBeDefined();
      expect(unpairedError!.message).toContain('has no matching LOOP_START');
    });
  });

  describe('loopStartNodeId cross-reference errors', () => {
    it('should error when loopStartNodeId references a non-existent node', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: {
              loopId: 'loop-1',
              loopStartNodeId: 'non-existent-node',
            },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const refError = errors.find(e => e.context?.['code'] === 'INVALID_LOOP_START_NODE_REFERENCE');
      expect(refError).toBeDefined();
      expect(refError!.message).toContain('non-existent');
    });

    it('should error when loopStartNodeId references a LOOP_START with mismatched loopId', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-a', maxIterations: 10 },
          },
          {
            id: 'loop-start-2',
            type: 'LOOP_START',
            config: { loopId: 'loop-b', maxIterations: 5 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: {
              loopId: 'loop-a',
              loopStartNodeId: 'loop-start-2', // points to loop-b
            },
          },
          // Need matching LOOP_END for loop-b too
          {
            id: 'loop-end-2',
            type: 'LOOP_END',
            config: { loopId: 'loop-b' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'loop-start-2' },
          { sourceNodeId: 'loop-start-2', targetNodeId: 'loop-end-2' },
          { sourceNodeId: 'loop-end-2', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const mismatchError = errors.find(e => e.context?.['code'] === 'LOOP_ID_MISMATCH');
      expect(mismatchError).toBeDefined();
      expect(mismatchError!.message).toContain('does not match');
    });
  });

  describe('Topology validation errors', () => {
    it('should error when LOOP_START has no outgoing edges', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'loop-1' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          // LOOP_START has no outgoing edge!
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const topoError = errors.find(e => e.context?.['code'] === 'LOOP_START_NO_OUTGOING_EDGES');
      expect(topoError).toBeDefined();
      expect(topoError!.message).toContain('outgoing edge');
    });

    it('should error when LOOP_END has no outgoing edges', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'loop-1' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'loop-start-1' },
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          // LOOP_END has no outgoing edge!
        ]
      );

      const errors = validateLoopPairs(graph);
      const topoError = errors.find(e => e.context?.['code'] === 'LOOP_END_NO_OUTGOING_EDGES');
      expect(topoError).toBeDefined();
      expect(topoError!.message).toContain('outgoing edge');
    });

    it('should error when LOOP_START has no incoming edges', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          {
            id: 'loop-start-1',
            type: 'LOOP_START',
            config: { loopId: 'loop-1', maxIterations: 10 },
          },
          { id: 'body-node', type: 'SCRIPT' },
          {
            id: 'loop-end-1',
            type: 'LOOP_END',
            config: { loopId: 'loop-1' },
          },
          { id: 'end', type: 'END' },
        ],
        [
          // LOOP_START has no incoming edge!
          { sourceNodeId: 'loop-start-1', targetNodeId: 'body-node' },
          { sourceNodeId: 'body-node', targetNodeId: 'loop-end-1' },
          { sourceNodeId: 'loop-end-1', targetNodeId: 'end' },
        ]
      );

      const errors = validateLoopPairs(graph);
      const topoError = errors.find(e => e.context?.['code'] === 'LOOP_START_NO_INCOMING_EDGES');
      expect(topoError).toBeDefined();
      expect(topoError!.message).toContain('incoming edge');
    });
  });
});
