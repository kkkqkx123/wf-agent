/**
 * Integration Tests for Phase 4 - Unified Hierarchy API
 * 
 * Tests the complete Agent → Agent delegation scenario and mixed hierarchies.
 * Validates backward compatibility and new unified API functionality.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ExecutionHierarchyRegistry } from '../../registry/execution-hierarchy-registry.js';
import type { AnyExecutionEntity } from '../../registry/execution-hierarchy-registry.js';

// Mock execution entities for integration testing
class MockWorkflowEntity {
  id: string;
  private parentContext?: any;
  private children: any[] = [];
  stopped: boolean = false;
  cleanedUp: boolean = false;

  constructor(id: string) {
    this.id = id;
  }

  getWorkflowId() {
    return this.id;
  }

  conversationManager: undefined = undefined;

  getParentContext() {
    return this.parentContext;
  }

  setParentContext(context: any) {
    this.parentContext = context;
  }

  getChildren() {
    return this.children;
  }

  addChild(childRef: any) {
    this.children.push(childRef);
  }

  stop() {
    this.stopped = true;
  }

  cleanup() {
    this.cleanedUp = true;
  }
}

class MockAgentEntity {
  id: string;
  private parentContext?: any;
  private children: any[] = [];
  stopped: boolean = false;
  cleanedUp: boolean = false;

  constructor(id: string) {
    this.id = id;
  }

  getWorkflowId(): undefined {
    return undefined;
  }

  conversationManager: any = {};

  getParentContext() {
    return this.parentContext;
  }

  setParentContext(context: any) {
    this.parentContext = context;
  }

  getChildren() {
    return this.children;
  }

  addChild(childRef: any) {
    this.children.push(childRef);
  }

  stop() {
    this.stopped = true;
  }

  cleanup() {
    this.cleanedUp = true;
  }
}

describe('Phase 4 Integration Tests - Unified Hierarchy', () => {
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = new ExecutionHierarchyRegistry();
  });

  describe('Agent → Agent Delegation Scenario', () => {
    it('should support Agent spawning sub-agents for task delegation', () => {
      // Main agent delegates to specialized sub-agents
      const mainAgent = new MockAgentEntity('main-agent');
      const codeReviewAgent = new MockAgentEntity('code-review-agent');
      const securityAgent = new MockAgentEntity('security-agent');
      const performanceAgent = new MockAgentEntity('performance-agent');

      // Set up parent-child relationships using new unified API
      codeReviewAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Code quality review',
      });
      
      securityAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Security vulnerability scan',
      });
      
      performanceAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Performance optimization analysis',
      });

      // Register children in main agent
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'code-review-agent',
        createdAt: Date.now(),
      });
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'security-agent',
        createdAt: Date.now(),
      });
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'performance-agent',
        createdAt: Date.now(),
      });

      // Register all in hierarchy registry
      registry.register(mainAgent as any);
      registry.register(codeReviewAgent as any);
      registry.register(securityAgent as any);
      registry.register(performanceAgent as any);

      // Verify hierarchy queries
      const descendants = registry.getAllDescendants('main-agent', false);
      expect(descendants).toHaveLength(3);
      expect(descendants).toContain(codeReviewAgent);
      expect(descendants).toContain(securityAgent);
      expect(descendants).toContain(performanceAgent);

      // Verify direct children
      const children = registry.getDirectChildren('main-agent');
      expect(children).toHaveLength(3);

      // Verify each sub-agent knows its parent
      expect(codeReviewAgent.getParentContext()).toEqual({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Code quality review',
      });
    });

    it('should support multi-level Agent delegation chains', () => {
      // Create deep delegation chain: main → coordinator → specialist
      const mainAgent = new MockAgentEntity('main-agent');
      const coordinatorAgent = new MockAgentEntity('coordinator-agent');
      const specialistAgent = new MockAgentEntity('specialist-agent');

      // Level 1: main delegates to coordinator
      coordinatorAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Task coordination',
      });
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'coordinator-agent',
        createdAt: Date.now(),
      });

      // Level 2: coordinator delegates to specialist
      specialistAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'coordinator-agent',
        delegationPurpose: 'Specialized analysis',
      });
      coordinatorAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'specialist-agent',
        createdAt: Date.now(),
      });

      // Register all
      registry.register(mainAgent as any);
      registry.register(coordinatorAgent as any);
      registry.register(specialistAgent as any);

      // Verify full chain is queryable
      const descendants = registry.getAllDescendants('main-agent', false);
      expect(descendants).toHaveLength(2);
      expect(descendants).toContain(coordinatorAgent);
      expect(descendants).toContain(specialistAgent);

      // Verify coordinator can query its own children
      const coordinatorChildren = registry.getDirectChildren('coordinator-agent');
      expect(coordinatorChildren).toHaveLength(1);
      expect(coordinatorChildren).toContain(specialistAgent);
    });
  });

  describe('Mixed Hierarchy Scenarios', () => {
    it('should support Workflow → Agent → Agent hierarchy', () => {
      // Complex scenario: workflow spawns agent, which delegates to sub-agents
      const workflow = new MockWorkflowEntity('main-workflow');
      const mainAgent = new MockAgentEntity('main-agent');
      const subAgent1 = new MockAgentEntity('sub-agent-1');
      const subAgent2 = new MockAgentEntity('sub-agent-2');

      // Workflow → Agent
      mainAgent.setParentContext({
        parentType: 'WORKFLOW',
        parentId: 'main-workflow',
        nodeId: 'agent-node-1',
      });
      workflow.addChild({
        childType: 'AGENT_LOOP',
        childId: 'main-agent',
        createdAt: Date.now(),
      });

      // Agent → Agent (delegation)
      subAgent1.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Data processing',
      });
      subAgent2.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Report generation',
      });
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'sub-agent-1',
        createdAt: Date.now(),
      });
      mainAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'sub-agent-2',
        createdAt: Date.now(),
      });

      // Register all
      registry.register(workflow as any);
      registry.register(mainAgent as any);
      registry.register(subAgent1 as any);
      registry.register(subAgent2 as any);

      // Verify complete hierarchy from workflow root
      const allDescendants = registry.getAllDescendants('main-workflow', false);
      expect(allDescendants).toHaveLength(3);
      expect(allDescendants).toContain(mainAgent);
      expect(allDescendants).toContain(subAgent1);
      expect(allDescendants).toContain(subAgent2);

      // Verify type grouping
      const { workflows, agents } = registry.getExecutionsByRoot('main-workflow');
      expect(workflows).toHaveLength(1);
      expect(agents).toHaveLength(3);
    });

    it('should support Agent → Workflow invocation', () => {
      // Agent triggers workflow execution
      const agent = new MockAgentEntity('triggering-agent');
      const workflow = new MockWorkflowEntity('triggered-workflow');

      // Agent → Workflow
      workflow.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'triggering-agent',
      });
      agent.addChild({
        childType: 'WORKFLOW',
        childId: 'triggered-workflow',
        createdAt: Date.now(),
      });

      // Register
      registry.register(agent as any);
      registry.register(workflow as any);

      // Verify hierarchy
      const descendants = registry.getAllDescendants('triggering-agent', false);
      expect(descendants).toHaveLength(1);
      expect(descendants).toContain(workflow);

      // Verify workflow knows its agent parent
      expect(workflow.getParentContext()).toEqual({
        parentType: 'AGENT_LOOP',
        parentId: 'triggering-agent',
      });
    });

    it('should handle complex real-world scenario', () => {
      // Real-world scenario: Customer service system
      // Root workflow orchestrates everything
      const customerServiceWorkflow = new MockWorkflowEntity('customer-service-wf');
      
      // Intent classification agent
      const intentAgent = new MockAgentEntity('intent-classifier');
      intentAgent.setParentContext({
        parentType: 'WORKFLOW',
        parentId: 'customer-service-wf',
        nodeId: 'intent-node',
      });
      customerServiceWorkflow.addChild({
        childType: 'AGENT_LOOP',
        childId: 'intent-classifier',
        createdAt: Date.now(),
      });

      // Response generation agent (spawned by workflow based on intent)
      const responseAgent = new MockAgentEntity('response-generator');
      responseAgent.setParentContext({
        parentType: 'WORKFLOW',
        parentId: 'customer-service-wf',
        nodeId: 'response-node',
      });
      customerServiceWorkflow.addChild({
        childType: 'AGENT_LOOP',
        childId: 'response-generator',
        createdAt: Date.now(),
      });

      // Knowledge base agent (delegated by response agent)
      const kbAgent = new MockAgentEntity('knowledge-base-agent');
      kbAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'response-generator',
        delegationPurpose: 'Retrieve relevant knowledge',
      });
      responseAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'knowledge-base-agent',
        createdAt: Date.now(),
      });

      // Sentiment analysis agent (delegated by response agent)
      const sentimentAgent = new MockAgentEntity('sentiment-analyzer');
      sentimentAgent.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'response-generator',
        delegationPurpose: 'Analyze customer sentiment',
      });
      responseAgent.addChild({
        childType: 'AGENT_LOOP',
        childId: 'sentiment-analyzer',
        createdAt: Date.now(),
      });

      // Escalation workflow (triggered if needed)
      const escalationWorkflow = new MockWorkflowEntity('escalation-wf');
      escalationWorkflow.setParentContext({
        parentType: 'AGENT_LOOP',
        parentId: 'response-generator',
      });
      responseAgent.addChild({
        childType: 'WORKFLOW',
        childId: 'escalation-wf',
        createdAt: Date.now(),
      });

      // Register all
      registry.register(customerServiceWorkflow as any);
      registry.register(intentAgent as any);
      registry.register(responseAgent as any);
      registry.register(kbAgent as any);
      registry.register(sentimentAgent as any);
      registry.register(escalationWorkflow as any);

      // Verify complete hierarchy
      const allExecutions = registry.getAllDescendants('customer-service-wf', true);
      expect(allExecutions).toHaveLength(6);

      // Verify type breakdown
      const { workflows, agents } = registry.getExecutionsByRoot('customer-service-wf');
      expect(workflows).toHaveLength(2); // root + escalation
      expect(agents).toHaveLength(4); // intent + response + kb + sentiment

      // Verify response agent's delegation tree
      const responseDescendants = registry.getAllDescendants('response-generator', false);
      expect(responseDescendants).toHaveLength(3); // kb + sentiment + escalation
    });
  });

  describe('Cleanup Operations in Mixed Hierarchies', () => {
    it('should cleanup entire mixed hierarchy tree', () => {
      const workflow = new MockWorkflowEntity('root-workflow');
      const agent1 = new MockAgentEntity('agent-1');
      const agent2 = new MockAgentEntity('agent-2');
      const subWorkflow = new MockWorkflowEntity('sub-workflow');

      // Build hierarchy
      agent1.setParentContext({ parentType: 'WORKFLOW', parentId: 'root-workflow' });
      agent2.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'agent-1' });
      subWorkflow.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'agent-2' });

      workflow.addChild({ childType: 'AGENT_LOOP', childId: 'agent-1', createdAt: Date.now() });
      agent1.addChild({ childType: 'AGENT_LOOP', childId: 'agent-2', createdAt: Date.now() });
      agent2.addChild({ childType: 'WORKFLOW', childId: 'sub-workflow', createdAt: Date.now() });

      registry.register(workflow as any);
      registry.register(agent1 as any);
      registry.register(agent2 as any);
      registry.register(subWorkflow as any);

      // Cleanup from root
      const cleanedCount = registry.cleanupHierarchy('root-workflow');
      
      expect(cleanedCount).toBe(4);
      expect(registry.size()).toBe(0);
      
      // Verify all were stopped and cleaned up
      expect(workflow.stopped).toBe(true);
      expect(workflow.cleanedUp).toBe(true);
      expect(agent1.stopped).toBe(true);
      expect(agent2.stopped).toBe(true);
      expect(subWorkflow.stopped).toBe(true);
    });
  });

  describe('Unified API Usage', () => {
    it('should use unified API for parent context management', () => {
      const agent = new MockAgentEntity('test-agent');
      
      // Use new unified API
      agent.setParentContext({
        parentType: 'WORKFLOW',
        parentId: 'parent-workflow',
        nodeId: 'node-id',
      });

      const parentContext = agent.getParentContext();
      expect(parentContext).toBeDefined();
      expect(parentContext?.parentType).toBe('WORKFLOW');
      expect(parentContext?.parentId).toBe('parent-workflow');
      if (parentContext?.parentType === 'WORKFLOW') {
        expect(parentContext.nodeId).toBe('node-id');
      }
    });
  });
});
