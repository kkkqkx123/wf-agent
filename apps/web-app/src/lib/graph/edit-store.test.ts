import { describe, expect, it } from 'vitest';
import { GraphEditStore } from './edit-store.svelte';
import type { DisplayNode } from './display-model';

function node(id: string, kind = 'SCRIPT'): DisplayNode {
	return { id, label: id, kind };
}

function storeWith(ids: Array<[string, string]>): GraphEditStore {
	const store = new GraphEditStore();
	store.load(
		ids.map(([id, kind]) => node(id, kind)),
		[],
	);
	return store;
}

describe('GraphEditStore.connect port validation', () => {
	it('connects ordinary nodes', () => {
		const store = storeWith([
			['a', 'SCRIPT'],
			['b', 'LLM'],
		]);
		expect(store.connect('a', 'b')).toBeNull();
		expect(store.edges).toHaveLength(1);
	});

	it('rejects edges into entry nodes', () => {
		const store = storeWith([
			['a', 'SCRIPT'],
			['start', 'START'],
		]);
		expect(store.connect('a', 'start')).toBe('START node cannot have incoming edges');
		expect(store.edges).toHaveLength(0);
	});

	it('rejects edges out of exit nodes', () => {
		const store = storeWith([
			['end', 'END'],
			['b', 'SCRIPT'],
		]);
		expect(store.connect('end', 'b')).toBe('END node cannot have outgoing edges');
		expect(store.edges).toHaveLength(0);
	});

	it('rejects triggered-subgraph boundaries', () => {
		const store = storeWith([
			['a', 'SCRIPT'],
			['msg', 'START_FROM_MESSAGE'],
		]);
		expect(store.connect('a', 'msg')).toBe('START_FROM_MESSAGE node cannot have incoming edges');
		const store2 = storeWith([
			['cont', 'CONTINUE_FROM_MESSAGE'],
			['b', 'SCRIPT'],
		]);
		expect(store2.connect('cont', 'b')).toBe(
			'CONTINUE_FROM_MESSAGE node cannot have outgoing edges',
		);
	});

	it('keeps duplicate connects idempotent and topology checks first', () => {
		const store = storeWith([
			['a', 'SCRIPT'],
			['b', 'SCRIPT'],
		]);
		expect(store.connect('a', 'b')).toBeNull();
		expect(store.connect('a', 'b')).toBeNull();
		expect(store.edges).toHaveLength(1);
		expect(store.connect('a', 'a')).toBe('Cannot self-connect.');
	});

	it('treats unrecognised kinds as ordinary middle nodes', () => {
		const store = storeWith([
			['a', 'acme.step'],
			['b', 'SCRIPT'],
		]);
		expect(store.connect('a', 'b')).toBeNull();
	});
});
