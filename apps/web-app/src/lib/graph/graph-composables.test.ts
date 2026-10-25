import { describe, expect, it } from 'vitest';
import { GraphFilterState } from './use-graph-filter.svelte';
import { GraphSelectionState } from './use-graph-selection.svelte';
import type { DisplayNode, DisplayEdge } from './display-model';

function node(id: string, kind = 'SCRIPT', extra?: Partial<DisplayNode>): DisplayNode {
	return { id, label: id, kind, ...extra };
}

function edge(id: string, source: string, target: string): DisplayEdge {
	return { id, source, target };
}

describe('GraphFilterState', () => {
	it('passes nodes through without filters', () => {
		const state = new GraphFilterState();
		const result = state.filter([node('a'), node('b')], [edge('e', 'a', 'b')]);
		expect(result.nodes).toHaveLength(2);
		expect(result.edges).toHaveLength(1);
		expect(state.active).toBe(false);
	});

	it('hides whole kinds and drops dangling edges', () => {
		const state = new GraphFilterState();
		state.toggleKind('LLM');
		const result = state.filter(
			[node('a', 'LLM'), node('b', 'SCRIPT')],
			[edge('e', 'a', 'b')],
		);
		expect(result.nodes.map((entry) => entry.id)).toEqual(['b']);
		expect(result.edges).toEqual([]);
		expect(state.active).toBe(true);
	});

	it('matches label, id and group queries', () => {
		const state = new GraphFilterState();
		state.query = 'web';
		const nodes = [
			node('a', 'SCRIPT', { label: 'Web Search' }),
			node('b', 'SCRIPT', { label: 'Other', groupId: 'web-group' }),
			node('c', 'SCRIPT', { label: 'Other' }),
		];
		const result = state.filter(nodes, []);
		expect(result.nodes.map((entry) => entry.id).sort()).toEqual(['a', 'b']);
	});

	it('clears filters', () => {
		const state = new GraphFilterState();
		state.query = 'a';
		state.toggleKind('SCRIPT');
		state.clear();
		expect(state.query).toBe('');
		expect(state.hiddenKinds).toEqual([]);
	});
});

describe('GraphSelectionState', () => {
	it('moves with wraparound', () => {
		const state = new GraphSelectionState();
		expect(state.move(['a', 'b', 'c'], 1)).toBe('a');
		expect(state.move(['a', 'b', 'c'], 1)).toBe('b');
		expect(state.move(['a', 'b', 'c'], -1)).toBe('a');
		expect(state.move(['a', 'b', 'c'], -1)).toBe('c');
	});

	it('clears on empty lists', () => {
		const state = new GraphSelectionState();
		state.select('a');
		expect(state.move([], 1)).toBeNull();
		expect(state.selectedId).toBeNull();
	});
});
