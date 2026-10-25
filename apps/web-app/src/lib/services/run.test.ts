import { describe, expect, it } from 'vitest';
import { assertRuntimeOptions } from '#lib/services/run';

describe('assertRuntimeOptions', () => {
	it('passes through missing options as null', () => {
		expect(assertRuntimeOptions(null)).toBeNull();
		expect(assertRuntimeOptions(undefined)).toBeNull();
	});

	it('passes through an all-empty option set unchanged', () => {
		expect(assertRuntimeOptions({})).toEqual({});
	});

	it('keeps positive budgets', () => {
		const checked = assertRuntimeOptions({ maxSteps: 3, timeoutMs: 1000 });
		expect(checked?.maxSteps).toBe(3);
		expect(checked?.timeoutMs).toBe(1000);
		expect(checked?.nodeTimeoutMs ?? null).toBeNull();
	});

	it('rejects zero budgets', () => {
		expect(() => assertRuntimeOptions({ maxSteps: 0 })).toThrow();
		expect(() => assertRuntimeOptions({ timeoutMs: 0 })).toThrow();
		expect(() => assertRuntimeOptions({ maxExecutionTimeMs: 0 })).toThrow();
	});
});
