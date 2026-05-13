/**
 * Workflow Graph Analyzer Tests
 * Tests for analyzeWorkflowGraph and collectForkJoinPairs functions
 */

import { describe, it, expect } from 'vitest';
import { analyzeWorkflowGraph, collectForkJoinPairs } from '../workflow-graph-analyzer.js';
import { WorkflowGraphData } from '../../../entities/workflow-graph-data.js';
import type { WorkflowNode, WorkflowEdge } from '@wf-agent/types';

// Helper function to create a simple node
function createNode(id: string, type: string = 'TASK', config: any = {}): WorkflowNode {
  return {
    id,
    type,
    config,
    workflowId: 'test-workflow',
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as WorkflowNode;
}

// Helper function to create a FORK node with forkId
function createForkNode(id: string, forkId: string): WorkflowNode {
  return {
    id,
    type: 'FORK',
    config: { forkPaths: [], forkStrategy: 'parallel' as const },
    workflowId: 'test-workflow',
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    originalNode: {
      config: { forkId }
    },
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as any as WorkflowNode;
}

// Helper function to create a JOIN node with joinId
function createJoinNode(id: string, joinId: string): WorkflowNode {
  return {
    id,
    type: 'JOIN',
    config: { forkPathIds: [], joinStrategy: 'all' as const, mainPathId: '' },
    workflowId: 'test-workflow',
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    originalNode: {
      config: { joinId }
    },
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as any as WorkflowNode;
}

// Helper function to create a simple edge
function createEdge(id: string, sourceNodeId: string, targetNodeId: string): WorkflowEdge {
  return {
    id,
    sourceNodeId,
    targetNodeId,
    type: 'DEFAULT',
    condition: undefined,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as WorkflowEdge;
}

describe('analyzeWorkflowGraph', () => {
  it('should analyze a simple linear graph', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('start', 'START'));
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createNode('end', 'END'));
    
    graph.addEdge(createEdge('e1', 'start', 'a'));
    graph.addEdge(createEdge('e2', 'a', 'end'));
    
    graph.startNodeId = 'start';
    graph.endNodeIds.add('end');
    
    const result = analyzeWorkflowGraph(graph);
    
    // Check cycle detection
    expect(result.cycleDetection.hasCycle).toBe(false);
    
    // Check reachability
    expect(result.reachability.unreachableNodes.size).toBe(0);
    expect(result.reachability.deadEndNodes.size).toBe(0);
    
    // Check topological sort
    expect(result.topologicalSort.success).toBe(true);
    expect(result.topologicalSort.sortedNodes.length).toBe(3);
    
    // Check node stats
    expect(result.nodeStats.total).toBe(3);
    expect(result.nodeStats.byType.get('START')).toBe(1);
    expect(result.nodeStats.byType.get('TASK')).toBe(1);
    expect(result.nodeStats.byType.get('END')).toBe(1);
    
    // Check edge stats
    expect(result.edgeStats.total).toBe(2);
    expect(result.edgeStats.byType.get('DEFAULT')).toBe(2);
  });

  it('should detect cycles in graph', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createNode('b', 'TASK'));
    graph.addNode(createNode('c', 'TASK'));
    graph.addEdge(createEdge('e1', 'a', 'b'));
    graph.addEdge(createEdge('e2', 'b', 'c'));
    graph.addEdge(createEdge('e3', 'c', 'a')); // Cycle
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.cycleDetection.hasCycle).toBe(true);
    expect(result.topologicalSort.success).toBe(false);
  });

  it('should handle graph with FORK/JOIN nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('start', 'START'));
    graph.addNode(createForkNode('fork1', 'fork-1'));
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createNode('b', 'TASK'));
    graph.addNode(createJoinNode('join1', 'fork-1'));
    graph.addNode(createNode('end', 'END'));
    
    graph.addEdge(createEdge('e1', 'start', 'fork1'));
    graph.addEdge(createEdge('e2', 'fork1', 'a'));
    graph.addEdge(createEdge('e3', 'fork1', 'b'));
    graph.addEdge(createEdge('e4', 'a', 'join1'));
    graph.addEdge(createEdge('e5', 'b', 'join1'));
    graph.addEdge(createEdge('e6', 'join1', 'end'));
    
    graph.startNodeId = 'start';
    graph.endNodeIds.add('end');
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.forkJoinValidation.isValid).toBe(true);
    expect(result.forkJoinValidation.unpairedForks.length).toBe(0);
    expect(result.forkJoinValidation.unpairedJoins.length).toBe(0);
    expect(result.forkJoinValidation.pairs.size).toBe(1);
  });

  it('should detect unpaired FORK/JOIN nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('start', 'START'));
    graph.addNode(createForkNode('fork1', 'fork-1')); // No matching JOIN
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createJoinNode('join1', 'fork-2')); // No matching FORK
    graph.addNode(createNode('end', 'END'));
    
    graph.addEdge(createEdge('e1', 'start', 'fork1'));
    graph.addEdge(createEdge('e2', 'fork1', 'a'));
    graph.addEdge(createEdge('e3', 'a', 'join1'));
    graph.addEdge(createEdge('e4', 'join1', 'end'));
    
    graph.startNodeId = 'start';
    graph.endNodeIds.add('end');
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.forkJoinValidation.isValid).toBe(false);
    expect(result.forkJoinValidation.unpairedForks.length).toBe(1);
    expect(result.forkJoinValidation.unpairedJoins.length).toBe(1);
  });

  it('should count nodes by type correctly', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('start', 'START'));
    graph.addNode(createNode('task1', 'TASK'));
    graph.addNode(createNode('task2', 'TASK'));
    graph.addNode(createNode('task3', 'LLM'));
    graph.addNode(createNode('end', 'END'));
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.nodeStats.total).toBe(5);
    expect(result.nodeStats.byType.get('START')).toBe(1);
    expect(result.nodeStats.byType.get('TASK')).toBe(2);
    expect(result.nodeStats.byType.get('LLM')).toBe(1);
    expect(result.nodeStats.byType.get('END')).toBe(1);
  });

  it('should count edges by type correctly', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createNode('b', 'TASK'));
    graph.addNode(createNode('c', 'TASK'));
    
    graph.addEdge(createEdge('e1', 'a', 'b'));
    graph.addEdge({ ...createEdge('e2', 'b', 'c'), type: 'CONDITIONAL' });
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.edgeStats.total).toBe(2);
    expect(result.edgeStats.byType.get('DEFAULT')).toBe(1);
    expect(result.edgeStats.byType.get('CONDITIONAL')).toBe(1);
  });

  it('should handle empty graph', () => {
    const graph = new WorkflowGraphData();
    
    const result = analyzeWorkflowGraph(graph);
    
    expect(result.nodeStats.total).toBe(0);
    expect(result.edgeStats.total).toBe(0);
    expect(result.cycleDetection.hasCycle).toBe(false);
    expect(result.topologicalSort.success).toBe(true);
  });
});

describe('collectForkJoinPairs', () => {
  it('should collect valid FORK/JOIN pairs', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createForkNode('fork1', 'fork-1'));
    graph.addNode(createJoinNode('join1', 'fork-1'));
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(true);
    expect(result.unpairedForks.length).toBe(0);
    expect(result.unpairedJoins.length).toBe(0);
    expect(result.pairs.size).toBe(1);
    expect(result.pairs.has('fork1')).toBe(true);
    expect(result.pairs.get('fork1')).toBe('join1');
  });

  it('should detect unpaired FORK nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createForkNode('fork1', 'fork-1'));
    graph.addNode(createForkNode('fork2', 'fork-2'));
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(false);
    expect(result.unpairedForks.length).toBe(2);
    expect(result.unpairedJoins.length).toBe(0);
  });

  it('should detect unpaired JOIN nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createJoinNode('join1', 'fork-1'));
    graph.addNode(createJoinNode('join2', 'fork-2'));
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(false);
    expect(result.unpairedForks.length).toBe(0);
    expect(result.unpairedJoins.length).toBe(2);
  });

  it('should handle mixed paired and unpaired nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createForkNode('fork1', 'fork-1'));
    graph.addNode(createJoinNode('join1', 'fork-1')); // Paired with fork1
    graph.addNode(createForkNode('fork2', 'fork-2')); // Unpaired
    graph.addNode(createJoinNode('join2', 'fork-3')); // Unpaired (no fork-3)
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(false);
    expect(result.unpairedForks.length).toBe(1);
    expect(result.unpairedJoins.length).toBe(1);
    expect(result.pairs.size).toBe(1);
  });

  it('should handle graph without FORK/JOIN nodes', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createNode('a', 'TASK'));
    graph.addNode(createNode('b', 'TASK'));
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(true);
    expect(result.unpairedForks.length).toBe(0);
    expect(result.unpairedJoins.length).toBe(0);
    expect(result.pairs.size).toBe(0);
  });

  it('should handle multiple valid pairs', () => {
    const graph = new WorkflowGraphData();
    graph.addNode(createForkNode('fork1', 'fork-1'));
    graph.addNode(createJoinNode('join1', 'fork-1'));
    graph.addNode(createForkNode('fork2', 'fork-2'));
    graph.addNode(createJoinNode('join2', 'fork-2'));
    
    const result = collectForkJoinPairs(graph);
    
    expect(result.isValid).toBe(true);
    expect(result.pairs.size).toBe(2);
    expect(result.pairs.has('fork1')).toBe(true);
    expect(result.pairs.has('fork2')).toBe(true);
  });
});
