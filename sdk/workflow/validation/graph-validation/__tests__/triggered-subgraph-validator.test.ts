/**
 * Triggered Subgraph Validator Unit Tests
 * 
 * Tests for:
 * - isTriggeredSubgraph: Checks if graph contains START_FROM_TRIGGER node
 * - validateTriggeredSubgraphConnectivity: Validates connectivity in triggered subgraphs
 */

import { describe, it, expect } from 'vitest';
import { 
  isTriggeredSubgraph, 
  validateTriggeredSubgraphConnectivity 
} from '../triggered-subgraph-validator.js';
import { WorkflowGraphData } from '../../../entities/workflow-graph-data.js';
import type { WorkflowNode, WorkflowEdge } from '@wf-agent/types';

describe('isTriggeredSubgraph', () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }> = []
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

  describe('Triggered subgraph detection', () => {
    it('should return true when graph contains START_FROM_TRIGGER node', () => {
      const graph = createGraph([
        { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
        { id: 'script1', type: 'SCRIPT' },
        { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
      ]);

      const result = isTriggeredSubgraph(graph);
      expect(result).toBe(true);
    });

    it('should return false when graph does not contain START_FROM_TRIGGER node', () => {
      const graph = createGraph([
        { id: 'start', type: 'START' },
        { id: 'script1', type: 'SCRIPT' },
        { id: 'end', type: 'END' },
      ]);

      const result = isTriggeredSubgraph(graph);
      expect(result).toBe(false);
    });

    it('should return false for empty graph', () => {
      const graph = createGraph([]);

      const result = isTriggeredSubgraph(graph);
      expect(result).toBe(false);
    });

    it('should return true even with multiple START_FROM_TRIGGER nodes', () => {
      const graph = createGraph([
        { id: 'start-trigger-1', type: 'START_FROM_TRIGGER' },
        { id: 'start-trigger-2', type: 'START_FROM_TRIGGER' },
        { id: 'script1', type: 'SCRIPT' },
      ]);

      const result = isTriggeredSubgraph(graph);
      expect(result).toBe(true);
    });
  });
});

describe('validateTriggeredSubgraphConnectivity', () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }> = []
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

  describe('Valid triggered subgraph connectivity', () => {
    it('should pass validation for a valid triggered subgraph', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'script2', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'script2' },
          { sourceNodeId: 'script2', targetNodeId: 'continue-trigger' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation for simple linear flow', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'continue-trigger' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation for branched flow where all paths reach end', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'fork', type: 'FORK' },
          { id: 'path-a', type: 'SCRIPT' },
          { id: 'path-b', type: 'SCRIPT' },
          { id: 'join', type: 'JOIN' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'fork' },
          { sourceNodeId: 'fork', targetNodeId: 'path-a' },
          { sourceNodeId: 'fork', targetNodeId: 'path-b' },
          { sourceNodeId: 'path-a', targetNodeId: 'join' },
          { sourceNodeId: 'path-b', targetNodeId: 'join' },
          { sourceNodeId: 'join', targetNodeId: 'continue-trigger' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors).toHaveLength(0);
    });

    it('should pass validation when there are no START_FROM_TRIGGER or CONTINUE_FROM_TRIGGER nodes', () => {
      const graph = createGraph(
        [
          { id: 'start', type: 'START' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'end', type: 'END' },
        ],
        [
          { sourceNodeId: 'start', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'end' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe('Invalid triggered subgraph connectivity', () => {
    it('should fail validation when node is not reachable from START_FROM_TRIGGER', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'script2', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'continue-trigger' },
          // script2 is not connected
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      // After improvement: script2 only gets UNREACHABLE error, not the duplicate CANNOT_REACH error
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toContain('not reachable from START_FROM_TRIGGER');
      expect(errors[0].message).toContain('disconnected from the workflow');
      expect(errors[0].context?.code).toBe('UNREACHABLE_FROM_START_FROM_TRIGGER');
      expect(errors[0].context?.nodeId).toBe('script2');
    });

    it('should fail validation when node cannot reach CONTINUE_FROM_TRIGGER', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'script2', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'script2' },
          // script2 does not connect to continue-trigger
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      // Both script1 and script2 cannot reach end, but now with better messages
      expect(errors.length).toBeGreaterThanOrEqual(2);
      
      const script2Error = errors.find(e => 
        e.context?.code === 'CANNOT_REACH_CONTINUE_FROM_TRIGGER' && 
        e.context?.nodeId === 'script2'
      );
      expect(script2Error).toBeDefined();
      expect(script2Error?.message).toContain('cannot reach CONTINUE_FROM_TRIGGER');
      expect(script2Error?.message).toContain('proper connections');
      
      const script1Error = errors.find(e => 
        e.context?.code === 'CANNOT_REACH_CONTINUE_FROM_TRIGGER' && 
        e.context?.nodeId === 'script1'
      );
      expect(script1Error).toBeDefined();
    });

    it('should report multiple errors for multiple unreachable nodes', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'script2', type: 'SCRIPT' },
          { id: 'script3', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'continue-trigger' },
          // script2 and script3 are not connected
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors.length).toBeGreaterThanOrEqual(2);
      
      const unreachableErrors = errors.filter(e => 
        e.context?.code === 'UNREACHABLE_FROM_START_FROM_TRIGGER'
      );
      expect(unreachableErrors.length).toBeGreaterThanOrEqual(2);
    });

    it('should report multiple errors for multiple nodes that cannot reach end', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'script2', type: 'SCRIPT' },
          { id: 'script3', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'script2' },
          { sourceNodeId: 'script2', targetNodeId: 'script3' },
          // None of the scripts connect to continue-trigger
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      expect(errors.length).toBeGreaterThanOrEqual(3);
      
      const cannotReachErrors = errors.filter(e => 
        e.context?.code === 'CANNOT_REACH_CONTINUE_FROM_TRIGGER'
      );
      expect(cannotReachErrors.length).toBeGreaterThanOrEqual(3);
    });

    it('should skip START_FROM_TRIGGER node when checking reachability from start', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'continue-trigger' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      // Should have no errors - START_FROM_TRIGGER should be skipped
      expect(errors).toHaveLength(0);
    });

    it('should skip CONTINUE_FROM_TRIGGER node when checking reachability to end', () => {
      const graph = createGraph(
        [
          { id: 'start-trigger', type: 'START_FROM_TRIGGER' },
          { id: 'script1', type: 'SCRIPT' },
          { id: 'continue-trigger', type: 'CONTINUE_FROM_TRIGGER' },
        ],
        [
          { sourceNodeId: 'start-trigger', targetNodeId: 'script1' },
          { sourceNodeId: 'script1', targetNodeId: 'continue-trigger' },
        ]
      );

      const errors = validateTriggeredSubgraphConnectivity(graph);
      // Should have no errors - CONTINUE_FROM_TRIGGER should be skipped
      expect(errors).toHaveLength(0);
    });
  });
});
