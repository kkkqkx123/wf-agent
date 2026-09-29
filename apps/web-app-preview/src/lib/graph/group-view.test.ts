import { describe, expect, it } from 'vitest';
import {
	aggregateGroupStatus,
	aggregateGroupTone,
	buildGroupView,
	deriveGroups,
	foldForCap,
	groupIdFromTitle,
	groupTitleId,
	isGroupTitleId,
	titlePosition,
	type GroupTitle,
} from './group-view';
import type { DisplayEdge, DisplayNode } from './display-model';

function node(
	id: string,
	groupId?: string,
	extra?: Partial<DisplayNode>,
): DisplayNode {
	return groupId
		? { id, label: id, kind: 'step', groupId, ...extra }
		: { id, label: id, kind: 'step', ...extra };
}

function edge(id: string, source: string, target: string): DisplayEdge {
	return { id, source, target };
}

describe('deriveGroups', () => {
	it('lists grouped ids in stable order', () => {
		expect(deriveGroups([node('b', 'g2'), node('a'), node('c', 'g1')])).toEqual(
			[
				{ id: 'g1', label: 'g1' },
				{ id: 'g2', label: 'g2' },
			],
		);
	});
	it('returns empty when nothing is grouped', () => {
		expect(deriveGroups([node('a')])).toEqual([]);
	});
});

describe('group title ids', () => {
	it('marks title ids', () => {
		expect(isGroupTitleId(groupTitleId('g'))).toBe(true);
		expect(isGroupTitleId('g')).toBe(false);
	});
});

describe('buildGroupView', () => {
	it('passes through when nothing is collapsed', () => {
		const nodes = [node('a', 'g'), node('b')];
		const edges = [edge('e', 'a', 'b')];
		const view = buildGroupView(nodes, edges, new Set());
		expect(view.nodes).toEqual(nodes);
		expect(view.edges).toEqual(edges);
		expect(view.hiddenIds.size).toBe(0);
	});
	it('hides members behind a title node and drops internal edges', () => {
		const nodes = [node('a', 'g'), node('b', 'g'), node('c')];
		const edges = [edge('inner', 'a', 'b'), edge('out', 'a', 'c')];
		const view = buildGroupView(nodes, edges, new Set(['g']));
		expect(view.hiddenIds).toEqual(new Set(['a', 'b']));
		expect(view.titleIds).toEqual(new Set(['group:g']));
		expect(view.nodes.map((entry) => entry.id).sort()).toEqual([
			'c',
			'group:g',
		]);
		expect(
			view.edges.map((entry) => `${entry.source}->${entry.target}`),
		).toEqual(['group:g->c']);
		expect(view.canonicals.get('group:g->c')).toEqual([
			{ source: 'a', target: 'c' },
		]);
		const titles: Record<string, GroupTitle> = view.titles;
		expect(titles['group:g']?.memberIds).toEqual(['a', 'b']);
	});
	it('merges parallel boundary edges and keeps the highest tone', () => {
		const nodes = [node('a', 'g'), node('b', 'g'), node('c')];
		const edges = [
			{ ...edge('e1', 'a', 'c'), status: 'completed' },
			{ ...edge('e2', 'b', 'c'), status: 'failed' },
		];
		const view = buildGroupView(nodes, edges, new Set(['g']));
		expect(view.edges).toHaveLength(1);
		expect(view.edges[0]?.label).toBe('2 links');
		expect(view.edges[0]?.status).toBe('failed');
		expect(view.canonicals.get(view.edges[0]?.id ?? ''))?.toHaveLength(2);
	});
	it('keeps edges between two collapsed groups on the boundary', () => {
		const nodes = [node('a', 'g1'), node('b', 'g2')];
		const view = buildGroupView(
			nodes,
			[edge('e', 'a', 'b')],
			new Set(['g1', 'g2']),
		);
		expect(
			view.edges.map((entry) => `${entry.source}->${entry.target}`),
		).toEqual(['group:g1->group:g2']);
	});
});

describe('aggregateGroupTone', () => {
	it('takes the highest member tone', () => {
		expect(aggregateGroupTone(['success', 'warning', 'error'])).toBe('error');
		expect(aggregateGroupTone([])).toBe('neutral');
	});
});

describe('group labels and status', () => {
	it('prefers real group labels over ids', () => {
		const nodes = [node('a', 'g', { groupLabel: 'Payments' }), node('b', 'g')];
		expect(deriveGroups(nodes)).toEqual([{ id: 'g', label: 'Payments' }]);
		const view = buildGroupView(nodes, [], new Set(['g']));
		expect(view.titles['group:g']?.label).toBe('Payments');
	});
	it('aggregates member failure onto the title', () => {
		const nodes = [
			{ ...node('a', 'g'), status: 'completed' },
			{ ...node('b', 'g'), status: 'failed' },
		];
		const view = buildGroupView(nodes, [], new Set(['g']));
		expect(view.titles['group:g']?.status).toBe('failed');
		expect(view.nodes.find((entry) => entry.id === 'group:g')?.status).toBe(
			'failed',
		);
		expect(aggregateGroupStatus(['completed', 'failed'])).toBe('failed');
		expect(aggregateGroupStatus([])).toBeUndefined();
	});
	it('honors status overrides for live execution', () => {
		const nodes = [node('a', 'g'), node('b', 'g')];
		const view = buildGroupView(nodes, [], new Set(['g']), {
			statusById: { a: 'running' },
		});
		expect(view.titles['group:g']?.status).toBe('running');
	});
});

describe('titlePosition', () => {
	it('averages known member positions', () => {
		expect(
			titlePosition(['a', 'b', 'c'], {
				a: { x: 0, y: 0 },
				b: { x: 10, y: 20 },
			}),
		).toEqual({ x: 5, y: 10 });
		expect(titlePosition(['a'], {})).toBeUndefined();
	});
});

describe('groupIdFromTitle', () => {
	it('strips the title prefix', () => {
		expect(groupIdFromTitle(groupTitleId('g'))).toBe('g');
	});
});

describe('foldForCap', () => {
	it('folds the largest unprotected group first', () => {
		const nodes = [
			node('a', 'big'),
			node('b', 'big'),
			node('c', 'big'),
			node('d', 'small'),
			node('e'),
		];
		const folded = foldForCap(nodes, [], new Set(), new Set(), 3);
		expect(folded.auto).toEqual(['big']);
		expect(folded.view.hiddenIds.has('a')).toBe(true);
		expect(folded.view.nodes.length).toBeLessThanOrEqual(3);
	});
	it('never auto-folds protected groups', () => {
		const nodes = [node('a', 'g'), node('b', 'g'), node('c')];
		const folded = foldForCap(nodes, [], new Set(), new Set(['g']), 2);
		expect(folded.auto).toEqual([]);
		expect(folded.view.nodes.length).toBe(3);
	});
	it('keeps manual folds without auto additions', () => {
		const nodes = [node('a', 'g'), node('b')];
		const folded = foldForCap(nodes, [], new Set(['g']), new Set(), 10);
		expect(folded.auto).toEqual([]);
		expect(folded.view.titleIds.has('group:g')).toBe(true);
	});
});
