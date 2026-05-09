/**
 * Workflow Type Field Integration Test
 * 
 * Tests that workflow type field is properly stored and retrieved
 */

import { describe, it, expect } from 'vitest';
import { WorkflowRegistry } from '../../workflow/stores/workflow-registry.js';
import { WorkflowGraphRegistry } from '../../workflow/stores/workflow-graph-registry.js';
import { GlobalContext } from '../../core/global-context.js';
import { createIsolatedContainer } from '../../core/di/index.js';
import type { WorkflowTemplate } from '@wf-agent/types';
import { now } from '@wf-agent/common-utils';
import * as Identifiers from '../../core/di/service-identifiers.js';

const createNode = (id: string, name: string, type: string): any => {
  return {
    id,
    name,
    type,
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
  };
};

const createEdge = (id: string, source: string, target: string): any => {
  return {
    id,
    sourceNodeId: source,
    targetNodeId: target,
    type: "NORMAL",
  };
};

describe('Workflow Type Field', () => {
  /**
   * Helper to create a minimal valid workflow
   */
  function createWorkflow(type: string): WorkflowTemplate {
    return {
      id: `test-wf-${type.toLowerCase()}-${Date.now()}`,
      name: `Test ${type} Workflow`,
      type: type as any,
      description: `A test workflow of type ${type}`,
      nodes: [
        createNode('start', 'Start', 'START'),
        createNode('end', 'End', 'END'),
      ],
      edges: [
        createEdge('edge-1', 'start', 'end'),
      ],
      version: '1.0.0',
      createdAt: now(),
      updatedAt: now(),
    };
  }

  /**
   * Helper to create a TRIGGERED_SUBWORKFLOW
   */
  function createTriggeredSubworkflow(): WorkflowTemplate {
    return {
      id: `test-triggered-${Date.now()}`,
      name: 'Test Triggered Subworkflow',
      type: 'TRIGGERED_SUBWORKFLOW' as any,
      description: 'A test triggered subworkflow',
      nodes: [
        createNode('start_from_trigger', 'Start From Trigger', 'START_FROM_TRIGGER'),
        createNode('continue_from_trigger', 'Continue From Trigger', 'CONTINUE_FROM_TRIGGER'),
      ],
      edges: [
        createEdge('edge-1', 'start_from_trigger', 'continue_from_trigger'),
      ],
      version: '1.0.0',
      createdAt: now(),
      updatedAt: now(),
    };
  }

  /**
   * Helper to create SDK infrastructure
   */
  function createSDKInfrastructure() {
    const { container } = createIsolatedContainer({});
    const globalContext = new GlobalContext(container);
    
    // Get services from container (already bound by configureContainerBindings)
    const graphRegistry = container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
    
    // Create workflow registry with globalContext so it can access the graphRegistry from container
    const workflowRegistry = new WorkflowRegistry(globalContext);
    
    return { workflowRegistry, graphRegistry, globalContext };
  }

  describe('STANDALONE workflow type', () => {
    it('should preserve STANDALONE type when registering and retrieving', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createWorkflow('STANDALONE');
      
      // Register workflow
      await workflowRegistry.registerAsync(workflow);
      
      // Retrieve workflow
      const retrieved = workflowRegistry.get(workflow.id);
      
      expect(retrieved).toBeDefined();
      expect(retrieved!.type).toBe('STANDALONE');
    });

    it('should include type in workflow summary', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createWorkflow('STANDALONE');
      
      await workflowRegistry.registerAsync(workflow);
      
      const summaries = await workflowRegistry.list();
      const summary = summaries.find((s: any) => s.id === workflow.id);
      
      expect(summary).toBeDefined();
      expect(summary!.type).toBe('STANDALONE');
    });
  });

  describe('DEPENDENT workflow type', () => {
    it('should preserve DEPENDENT type when registering and retrieving', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createWorkflow('DEPENDENT');
      
      await workflowRegistry.registerAsync(workflow);
      
      const retrieved = workflowRegistry.get(workflow.id);
      
      expect(retrieved).toBeDefined();
      expect(retrieved!.type).toBe('DEPENDENT');
    });

    it('should include type in workflow summary', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createWorkflow('DEPENDENT');
      
      await workflowRegistry.registerAsync(workflow);
      
      const summaries = await workflowRegistry.list();
      const summary = summaries.find((s: any) => s.id === workflow.id);
      
      expect(summary).toBeDefined();
      expect(summary!.type).toBe('DEPENDENT');
    });
  });

  describe('TRIGGERED_SUBWORKFLOW type', () => {
    it('should preserve TRIGGERED_SUBWORKFLOW type when registering and retrieving', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createTriggeredSubworkflow();
      
      await workflowRegistry.registerAsync(workflow);
      
      const retrieved = workflowRegistry.get(workflow.id);
      
      expect(retrieved).toBeDefined();
      expect(retrieved!.type).toBe('TRIGGERED_SUBWORKFLOW');
    });

    it('should include type in workflow summary', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const workflow = createTriggeredSubworkflow();
      
      await workflowRegistry.registerAsync(workflow);
      
      const summaries = await workflowRegistry.list();
      const summary = summaries.find((s: any) => s.id === workflow.id);
      
      expect(summary).toBeDefined();
      expect(summary!.type).toBe('TRIGGERED_SUBWORKFLOW');
    });

    it('should successfully register TRIGGERED_SUBWORKFLOW with proper validation', async () => {
      const { workflowRegistry, graphRegistry } = createSDKInfrastructure();
      const workflow = createTriggeredSubworkflow();
      
      // Should not throw during registration
      await expect(workflowRegistry.registerAsync(workflow)).resolves.not.toThrow();
      
      // Verify it's registered
      expect(workflowRegistry.has(workflow.id)).toBe(true);
      
      // Verify the preprocessed graph exists
      const graph = graphRegistry.get(workflow.id);
      expect(graph).toBeDefined();
    });
  });

  describe('Multiple workflows with different types', () => {
    it('should correctly handle multiple workflows with different types', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const standalone = createWorkflow('STANDALONE');
      const dependent = createWorkflow('DEPENDENT');
      const triggered = createTriggeredSubworkflow();
      
      // Register all workflows
      await workflowRegistry.registerAsync(standalone);
      await workflowRegistry.registerAsync(dependent);
      await workflowRegistry.registerAsync(triggered);
      
      // Get all summaries
      const summaries = await workflowRegistry.list();
      
      expect(summaries.length).toBeGreaterThanOrEqual(3);
      
      // Verify each has correct type
      const standaloneSummary = summaries.find((s: any) => s.id === standalone.id);
      const dependentSummary = summaries.find((s: any) => s.id === dependent.id);
      const triggeredSummary = summaries.find((s: any) => s.id === triggered.id);
      
      expect(standaloneSummary!.type).toBe('STANDALONE');
      expect(dependentSummary!.type).toBe('DEPENDENT');
      expect(triggeredSummary!.type).toBe('TRIGGERED_SUBWORKFLOW');
    });

    it('should retrieve individual workflows with correct types', async () => {
      const { workflowRegistry } = createSDKInfrastructure();
      const standalone = createWorkflow('STANDALONE');
      const dependent = createWorkflow('DEPENDENT');
      const triggered = createTriggeredSubworkflow();
      
      await workflowRegistry.registerAsync(standalone);
      await workflowRegistry.registerAsync(dependent);
      await workflowRegistry.registerAsync(triggered);
      
      // Retrieve each individually
      const retrievedStandalone = workflowRegistry.get(standalone.id);
      const retrievedDependent = workflowRegistry.get(dependent.id);
      const retrievedTriggered = workflowRegistry.get(triggered.id);
      
      expect(retrievedStandalone!.type).toBe('STANDALONE');
      expect(retrievedDependent!.type).toBe('DEPENDENT');
      expect(retrievedTriggered!.type).toBe('TRIGGERED_SUBWORKFLOW');
    });
  });
});
