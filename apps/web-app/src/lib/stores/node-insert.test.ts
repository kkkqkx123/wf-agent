import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NODE_INSERT_MAX_AGE_MS, nodeInsertStore } from './node-insert.svelte';

class MemoryStorage {
	private values = new Map<string, string>();

	getItem(key: string): string | null {
		return this.values.get(key) ?? null;
	}

	setItem(key: string, value: string): void {
		this.values.set(key, value);
	}

	removeItem(key: string): void {
		this.values.delete(key);
	}
}

describe('nodeInsertStore', () => {
	beforeEach(() => {
		vi.stubGlobal('sessionStorage', new MemoryStorage());
		nodeInsertStore.clear();
	});

	afterEach(() => {
		nodeInsertStore.clear();
		vi.unstubAllGlobals();
	});

	it('normalises the node type and restores an unexpired queue', () => {
		nodeInsertStore.set({
			nodeType: ' llm ',
			name: 'Reusable LLM',
			targetWorkflowId: ' wf-1 ',
			sourceTemplateId: 'nt-llm',
		});

		const pending = nodeInsertStore.peek();
		expect(pending?.nodeType).toBe('LLM');
		expect(pending?.targetWorkflowId).toBe('wf-1');
		expect(pending?.sourceTemplateId).toBe('nt-llm');
	});

	it('drops expired entries from storage', () => {
		sessionStorage.setItem(
			'wf.pending-node-insert.v1',
			JSON.stringify({
				nodeType: 'LLM',
				name: 'Old insert',
				queuedAt: Date.now() - NODE_INSERT_MAX_AGE_MS - 1,
			}),
		);

		expect(nodeInsertStore.peek()).toBeNull();
		expect(sessionStorage.getItem('wf.pending-node-insert.v1')).toBeNull();
	});

	it('rejects blank node types', () => {
		expect(() => nodeInsertStore.set({ nodeType: '  ', name: 'Invalid' })).toThrowError(
			/non-blank node type/,
		);
	});

	it('keeps the queue until the editor clears it after applying', () => {
		nodeInsertStore.set({ nodeType: 'SCRIPT', name: 'Step' });
		expect(nodeInsertStore.peek()?.name).toBe('Step');
		expect(nodeInsertStore.peek()?.name).toBe('Step');

		nodeInsertStore.clear();
		expect(nodeInsertStore.peek()).toBeNull();
	});
});
