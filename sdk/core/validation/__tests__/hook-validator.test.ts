import { describe, it, expect, vi, beforeEach } from 'vitest';
import { validateHook, validateHooks } from '../hook-validator.js';
import { ConfigurationValidationError, ExpressionSecurityError } from '@wf-agent/types';
import { validateExpression } from '../../../workflow/evaluation/index.js';

vi.mock('../../../workflow/evaluation/index.js', () => ({
  validateExpression: vi.fn(),
}));

function createValidHook(overrides: Record<string, unknown> = {}) {
  return {
    hookType: 'BEFORE_EXECUTE',
    eventName: 'test_event',
    ...overrides,
  };
}

describe('validateHook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should validate a valid hook', () => {
    const hook = createValidHook();
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should validate a hook with AFTER_EXECUTE type', () => {
    const hook = createValidHook({ hookType: 'AFTER_EXECUTE' });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should reject hook with invalid hookType', () => {
    const hook = createValidHook({ hookType: 'INVALID_TYPE' });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it('should reject hook with empty eventName', () => {
    const hook = createValidHook({ eventName: '' });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isErr()).toBe(true);
  });

  it('should reject hook with missing eventName', () => {
    const hook = createValidHook({ eventName: undefined });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isErr()).toBe(true);
  });

  it('should validate hook with optional fields', () => {
    const hook = createValidHook({
      enabled: true,
      weight: 10,
    });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should validate hook with condition expression', () => {
    const hook = createValidHook({
      condition: { expression: 'data.status == "completed"' },
    });
    vi.mocked(validateExpression).mockReturnValue(undefined);
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should return err when condition expression fails security check', () => {
    const hook = createValidHook({
      condition: { expression: 'unsafe' },
    });
    vi.mocked(validateExpression).mockImplementation(() => {
      throw new ExpressionSecurityError('Expression is too long', {
        operation: 'validateExpression',
        field: 'expression',
      });
    });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it('should propagate non-security errors from validateExpression', () => {
    const hook = createValidHook({
      condition: { expression: 'data.status == "completed"' },
    });
    vi.mocked(validateExpression).mockImplementation(() => {
      throw new Error('Unexpected error');
    });
    expect(() => validateHook(hook as any, 'node-1')).toThrow('Unexpected error');
  });

  it('should validate hook with eventPayload', () => {
    const hook = createValidHook({
      eventPayload: { key: 'value', number: 42 },
    });
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should validate hook with condition metadata', () => {
    const hook = createValidHook({
      condition: { expression: 'true', metadata: { source: 'test' } },
    });
    vi.mocked(validateExpression).mockReturnValue(undefined);
    const result = validateHook(hook as any, 'node-1');

    expect(result.isOk()).toBe(true);
  });

  it('should handle boolean enabled field', () => {
    const hook = createValidHook({ enabled: false });
    const result = validateHook(hook as any, 'node-1');
    expect(result.isOk()).toBe(true);
  });

  it('should handle numeric weight field', () => {
    const hook = createValidHook({ weight: 0 });
    const result = validateHook(hook as any, 'node-1');
    expect(result.isOk()).toBe(true);
  });
});

describe('validateHooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should validate an array of valid hooks', () => {
    const hooks = [
      createValidHook({ hookType: 'BEFORE_EXECUTE', eventName: 'event_1' }),
      createValidHook({ hookType: 'AFTER_EXECUTE', eventName: 'event_2' }),
    ];
    const result = validateHooks(hooks as any[], 'node-1');

    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value.length).toBe(2);
    }
  });

  it('should return err for non-array input', () => {
    const result = validateHooks(null as any, 'node-1');

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]!.message).toContain('must be an array');
    }
  });

  it('should return err for undefined input', () => {
    const result = validateHooks(undefined as any, 'node-1');

    expect(result.isErr()).toBe(true);
  });

  it('should return err when any hook is invalid', () => {
    const hooks = [
      createValidHook({ hookType: 'BEFORE_EXECUTE', eventName: 'event_1' }),
      createValidHook({ hookType: 'INVALID_TYPE', eventName: 'event_2' }),
    ];
    const result = validateHooks(hooks as any[], 'node-1');

    expect(result.isErr()).toBe(true);
  });

  it('should handle empty array', () => {
    const result = validateHooks([], 'node-1');

    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value).toEqual([]);
    }
  });

  it('should reject null/undefined hooks in the array', () => {
    const hooks = [
      createValidHook({ hookType: 'BEFORE_EXECUTE', eventName: 'event_1' }),
      null,
      undefined,
    ];
    const result = validateHooks(hooks as any[], 'node-1');

    expect(result.isErr()).toBe(true);
  });

  it('should validate with condition expressions in all hooks', () => {
    const hooks = [
      createValidHook({
        hookType: 'BEFORE_EXECUTE',
        eventName: 'event_1',
        condition: { expression: 'a > 1' },
      }),
      createValidHook({
        hookType: 'AFTER_EXECUTE',
        eventName: 'event_2',
        condition: { expression: 'b == 2' },
      }),
    ];
    vi.mocked(validateExpression).mockReturnValue(undefined);
    const result = validateHooks(hooks as any[], 'node-1');
    expect(result.isOk()).toBe(true);
  });
});