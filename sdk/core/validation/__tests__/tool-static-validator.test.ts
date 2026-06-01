import { describe, it, expect } from 'vitest';
import { StaticValidator } from '../tool-static-validator.js';
import type { Tool } from '@wf-agent/types';

function createMinimalTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: 'test_tool',
    type: 'STATELESS',
    description: 'A test tool',
    parameters: {
      type: 'object',
      properties: {},
      required: [],
    },
    ...overrides,
  } as Tool;
}

describe('StaticValidator', () => {
  let validator: StaticValidator;

  beforeEach(() => {
    validator = new StaticValidator();
  });

  describe('validateTool', () => {
    it('should validate a valid STATELESS tool', () => {
      const tool = createMinimalTool({
        type: 'STATELESS',
        config: { execute: async () => 'ok' },
      });
      const result = validator.validateTool(tool);
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value.id).toBe('test_tool');
      }
    });

    it('should validate a valid STATEFUL tool', () => {
      const tool = createMinimalTool({
        type: 'STATEFUL',
        config: {
          factory: { create: () => ({ execute: async () => 'ok' }) },
        },
      });
      const result = validator.validateTool(tool);
      expect(result.isOk()).toBe(true);
    });

    it('should validate a valid REST tool without config', () => {
      const tool = createMinimalTool({ type: 'REST' });
      const result = validator.validateTool(tool);
      expect(result.isOk()).toBe(true);
    });

    it('should validate a valid REST tool with config', () => {
      const tool = createMinimalTool({
        type: 'REST',
        config: { baseUrl: 'https://api.example.com' },
      });
      const result = validator.validateTool(tool);
      expect(result.isOk()).toBe(true);
    });

    it('should reject tool with empty id', () => {
      const tool = createMinimalTool({ id: '' });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject tool with invalid id format (uppercase)', () => {
      const tool = createMinimalTool({ id: 'Invalid-ID' });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject tool with empty description', () => {
      const tool = createMinimalTool({ description: '' });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject tool with invalid type', () => {
      const tool = createMinimalTool({ type: 'UNKNOWN' as any });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject STATELESS tool without config.execute', () => {
      const tool = createMinimalTool({ type: 'STATELESS', config: {} as any });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject STATEFUL tool without config.factory', () => {
      const tool = createMinimalTool({ type: 'STATEFUL', config: {} as any });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should reject tool with invalid parameter required field', () => {
      const tool = createMinimalTool({
        parameters: {
          type: 'object',
          properties: { name: { type: 'string' } },
          required: ['nonexistent'],
        },
      });
      const result = validator.validateTool(tool);
      expect(result.isErr()).toBe(true);
    });

    it('should validate BUILTIN tool', () => {
      const tool = createMinimalTool({
        type: 'BUILTIN',
        config: { execute: async (_params: any, _context: any) => 'ok' },
      });
      const result = validator.validateTool(tool);
      expect(result.isOk()).toBe(true);
    });
  });

  describe('validateParameters', () => {
    it('should validate valid parameters', () => {
      const params = {
        type: 'object' as const,
        properties: {
          name: { type: 'string' as const, description: 'Name' },
        },
        required: ['name'],
      };
      const result = validator.validateParameters(params);
      expect(result.isOk()).toBe(true);
    });

    it('should reject parameters with invalid type', () => {
      const params = {
        type: 'array' as const,
        properties: {},
        required: [],
      };
      const result = validator.validateParameters(params);
      expect(result.isErr()).toBe(true);
    });

    it('should reject parameters with undefined required field', () => {
      const params = {
        type: 'object' as const,
        properties: {
          name: { type: 'string' as const },
        },
        required: ['nonexistent'],
      };
      const result = validator.validateParameters(params);
      expect(result.isErr()).toBe(true);
    });

    it('should accept parameters with empty properties', () => {
      const params = {
        type: 'object' as const,
        properties: {},
        required: [],
      };
      const result = validator.validateParameters(params);
      expect(result.isOk()).toBe(true);
    });
  });

  describe('validateToolConfig', () => {
    it('should validate STATELESS config', () => {
      const config = { execute: async () => 'ok' };
      const result = validator.validateToolConfig('STATELESS', config);
      expect(result.isOk()).toBe(true);
    });

    it('should validate STATEFUL config', () => {
      const config = { factory: { create: () => ({ execute: async () => 'ok' }) } };
      const result = validator.validateToolConfig('STATEFUL', config);
      expect(result.isOk()).toBe(true);
    });

    it('should validate REST config', () => {
      const config = { baseUrl: 'https://api.example.com', timeout: 5000 };
      const result = validator.validateToolConfig('REST', config);
      expect(result.isOk()).toBe(true);
    });

    it('should validate REST config with empty object', () => {
      const result = validator.validateToolConfig('REST', {});
      expect(result.isOk()).toBe(true);
    });

    it('should reject invalid STATELESS config (missing execute)', () => {
      const config = { version: '1.0' };
      const result = validator.validateToolConfig('STATELESS', config);
      expect(result.isErr()).toBe(true);
    });

    it('should reject invalid STATEFUL config (missing factory)', () => {
      const config = { version: '1.0' };
      const result = validator.validateToolConfig('STATEFUL', config);
      expect(result.isErr()).toBe(true);
    });

    it('should reject invalid URL in REST config', () => {
      const config = { baseUrl: 'not-a-url' };
      const result = validator.validateToolConfig('REST', config);
      expect(result.isErr()).toBe(true);
    });

    it('should return err for unknown tool type', () => {
      const result = validator.validateToolConfig('UNKNOWN' as any, {});
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0].message).toContain('Unknown tool type');
      }
    });

    it('should reject BUILTIN config without execute', () => {
      const config = {};
      const result = validator.validateToolConfig('BUILTIN', config);
      expect(result.isErr()).toBe(true);
    });

    it('should validate BUILTIN config with execute', () => {
      const config = { execute: async (_params: any, _ctx: any) => 'ok' };
      const result = validator.validateToolConfig('BUILTIN', config);
      expect(result.isOk()).toBe(true);
    });

    it('should reject negative timeout in REST config', () => {
      const config = { timeout: -100 };
      const result = validator.validateToolConfig('REST', config);
      expect(result.isErr()).toBe(true);
    });
  });
});