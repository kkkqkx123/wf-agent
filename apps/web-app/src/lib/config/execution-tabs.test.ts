import { describe, expect, it } from 'vitest';
import {
	EXECUTION_TABS,
	isExecutionTab,
	type ExecutionTab,
} from './execution-tabs';

describe('execution tabs', () => {
	it('gives every tab a unique id', () => {
		const ids = EXECUTION_TABS.map((tab) => tab.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it('accepts every declared tab', () => {
		for (const tab of EXECUTION_TABS) {
			expect(isExecutionTab(tab.id)).toBe(true);
		}
	});

	it('rejects unknown and absent tabs', () => {
		expect(isExecutionTab('nope')).toBe(false);
		expect(isExecutionTab(null)).toBe(false);
		expect(isExecutionTab('')).toBe(false);
	});

	it('narrows a valid tab id', () => {
		const value: string = 'hierarchy';
		if (!isExecutionTab(value)) throw new Error('expected a real tab');
		const tab: ExecutionTab = value;
		expect(tab).toBe('hierarchy');
	});
});
