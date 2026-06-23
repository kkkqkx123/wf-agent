/**
 * SDKKit Main Class Tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SDKKit } from '@/kit.js';
import type { WorkflowAPI } from '@/api/workflow.api.js';
import type { ExecutionAPI } from '@/api/execution.api.js';
import type { QueryAPI } from '@/api/query.api.js';

describe('SDKKit', () => {
  let sdk: any;
  let kit: SDKKit;

  beforeEach(() => {
    // Mock ExecuteWorkflowCommand class
    const MockCommand = class {
      constructor() {}
      execute() {
        return Promise.resolve({ success: true, data: {} });
      }
    };

    sdk = {
      executeCommand: vi.fn(() => ({ success: true, data: {} })),
      getFactory: vi.fn(() => ({
        getDependencies: vi.fn(() => ({})),
        getWorkflowExecutionRegistry: vi.fn(),
      })),
      ExecuteWorkflowCommand: MockCommand,
    };

    kit = new SDKKit(sdk);
  });

  it('should provide workflow API', () => {
    const api = kit.workflow();

    expect(api).toBeDefined();
    expect(api.create).toBeDefined();
    expect(api.fromTemplate).toBeDefined();
  });

  it('should provide execution API', () => {
    const api = kit.execution();

    expect(api).toBeDefined();
    expect(api.workflow).toBeDefined();
  });

  it('should provide query API', () => {
    const api = kit.query();

    expect(api).toBeDefined();
    expect(api.executions).toBeDefined();
  });

  it('should provide access to underlying SDK', () => {
    const underlying = kit.getSDK();

    expect(underlying).toBe(sdk);
  });

  it('should return same API instances on multiple calls', () => {
    const workflow1 = kit.workflow();
    const workflow2 = kit.workflow();

    expect(workflow1).toBe(workflow2);
  });

  describe('workflow API', () => {
    it('should create new workflow builder', () => {
      const api = kit.workflow();
      const builder = api.create('test-wf');

      expect(builder).toBeDefined();
      expect(builder.node).toBeDefined();
      expect(builder.edge).toBeDefined();
      expect(builder.build).toBeDefined();
    });
  });

  describe('execution API', () => {
    it('should create execution builder', () => {
      const api = kit.execution();
      const builder = api.workflow('test-wf');

      expect(builder).toBeDefined();
      expect(builder.input).toBeDefined();
      expect(builder.execute).toBeDefined();
    });
  });

  describe('query API', () => {
    it('should create query builder', () => {
      const api = kit.query();
      const builder = api.executions();

      expect(builder).toBeDefined();
      expect(builder.filter).toBeDefined();
      expect(builder.get).toBeDefined();
    });
  });
});
