/**
 * Phase 1 Refactoring Verification Tests
 * 
 * This test file verifies the key changes made in Phase 1:
 * 1. VariableManager.copyFrom() now deep clones global variables
 * 2. SYNC node types are properly defined
 * 3. SYNC handler is registered and accessible
 */

import { describe, it, expect } from 'vitest';
import { VariableManager } from '../../../../state-managers/variable-manager.js';
import { isSyncNodeConfig, SyncNodeConfigSchema } from '@wf-agent/types';

describe('Phase 1: Fork-Join Sync Refactoring', () => {
  describe('Task 1.1: VariableManager.copyFrom() complete isolation', () => {
    it('should deep clone global variables (not share by reference)', () => {
      const source = new VariableManager();
      
      // Register a global variable with an object value
      source.registerVariable({
        name: 'sharedConfig',
        type: 'object',
        value: { timeout: 5000, retries: 3 },
        scope: 'global',
      });

      // Copy to target
      const target = new VariableManager();
      target.copyFrom(source);

      // Verify the value is copied
      const targetValue = target.getVariable('sharedConfig') as any;
      expect(targetValue).toEqual({ timeout: 5000, retries: 3 });

      // Modify the target's value
      target.setVariable('sharedConfig', { timeout: 10000, retries: 5 });

      // Verify source is NOT affected (complete isolation)
      const sourceValue = source.getVariable('sharedConfig');
      expect(sourceValue).toEqual({ timeout: 5000, retries: 3 });
      expect(sourceValue).not.toBe(target.getVariable('sharedConfig'));
    });

    it('should deep clone execution variables', () => {
      const source = new VariableManager();
      
      // Register an execution-scoped variable
      source.registerVariable({
        name: 'executionData',
        type: 'object',
        value: { items: [1, 2, 3] },
        scope: 'workflowExecution',
      });

      // Copy to target
      const target = new VariableManager();
      target.copyFrom(source);

      // Modify target's array
      const targetData = target.getVariable('executionData') as any;
      targetData.items.push(4);

      // Verify source is NOT affected
      const sourceData = source.getVariable('executionData') as any;
      expect(sourceData.items).toEqual([1, 2, 3]);
      expect(sourceData.items.length).toBe(3);
    });

    it('should maintain complete isolation between fork branches', () => {
      // Simulate fork scenario
      const parent = new VariableManager();
      parent.registerVariable({
        name: 'counter',
        type: 'number',
        value: 0,
        scope: 'global',
      });

      // Create two branch copies
      const branch1 = new VariableManager();
      branch1.copyFrom(parent);

      const branch2 = new VariableManager();
      branch2.copyFrom(parent);

      // Modify branch1
      branch1.setVariable('counter', 10);

      // Verify branch2 and parent are unaffected
      expect(branch2.getVariable('counter')).toBe(0);
      expect(parent.getVariable('counter')).toBe(0);

      // Modify branch2
      branch2.setVariable('counter', 20);

      // Verify branch1 and parent are still unaffected
      expect(branch1.getVariable('counter')).toBe(10);
      expect(parent.getVariable('counter')).toBe(0);
    });
  });

  describe('Task 1.2: SYNC node type definitions', () => {
    it('should validate valid SYNC node config', () => {
      const validConfig = {
        sourcePathId: 'branch-a',
        targetPathId: 'branch-b',
        variableMappings: [
          {
            externalName: 'result',
            internalName: 'syncedResult',
            required: true,
          },
        ],
        waitForCompletion: true,
        timeout: 30,
      };

      expect(isSyncNodeConfig(validConfig)).toBe(true);
      
      const parsed = SyncNodeConfigSchema.safeParse(validConfig);
      expect(parsed.success).toBe(true);
    });

    it('should reject invalid SYNC node config without sourcePathId', () => {
      const invalidConfig = {
        targetPathId: 'branch-b',
        variableMappings: [],
      };

      expect(isSyncNodeConfig(invalidConfig)).toBe(false);
    });

    it('should accept minimal SYNC config with only sourcePathId', () => {
      const minimalConfig = {
        sourcePathId: 'branch-a',
      };

      expect(isSyncNodeConfig(minimalConfig)).toBe(true);
    });

    it('should use default values for optional fields', () => {
      const config = {
        sourcePathId: 'branch-a',
      };

      const parsed = SyncNodeConfigSchema.parse(config);
      expect(parsed.waitForCompletion).toBe(true);
      expect(parsed.timeout).toBe(0);
    });
  });

  describe('Task 1.3: SYNC handler integration', () => {
    it('should have syncHandler exported from node-handlers', async () => {
      const { syncHandler } = await import('../index.js');
      expect(syncHandler).toBeDefined();
      expect(typeof syncHandler).toBe('function');
    });

    it('should register SYNC handler in getNodeHandler', async () => {
      const { getNodeHandler } = await import('../index.js');
      
      const handler = getNodeHandler('SYNC');
      expect(handler).toBeDefined();
      expect(typeof handler).toBe('function');
    });
  });
});
