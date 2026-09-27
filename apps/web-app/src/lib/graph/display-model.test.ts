import { describe, expect, it } from 'vitest';
import {
	capGraph,
	columnPositions,
	distinctKinds,
	isDashedEdge,
	legendFor,
	nodeShape,
	shortLabel,
	statusHex,
	type DisplayEdge,
	type DisplayNode,
} from './display-model';

function node(id: string, kind = 'llm', extra?: Partial<DisplayNode>): DisplayNode {
	return { id, label: id, kind, ...extra };
}

function edge(id: string, source: string, target: string): DisplayEdge {
	return { id, source, target };
}

describe('nodeShape', () => {
	it('maps terminals to ellipses', () => {
		expect(nodeShape('START', 'workflow')).toBe('ellipse');
		expect(nodeShape('end', 'decision')).toBe('ellipse');
	});
	it('maps error kinds to diamonds in decision graphs', () => {
		expect(nodeShape('error', 'decision')).toBe('diamond');
		expect(nodeShape('error', 'workflow')).toBe('round-rectangle');
	});
	it('maps tools to hexagons outside workflows', () => {
		expect(nodeShape('tool_call', 'decision')).toBe('hexagon');
		expect(nodeShape('LLM', 'workflow')).toBe('round-rectangle');
	});
});

describe('isDashedEdge', () => {
	it('dashes conditional and error routes', () => {
		expect(isDashedEdge('conditional')).toBe(true);
		expect(isDashedEdge('error_route')).toBe(true);
		expect(isDashedEdge('default')).toBe(false);
		expect(isDashedEdge(undefined, false)).toBe(true);
	});
});

describe('statusHex', () => {
	it('maps known statuses and falls back to neutral', () => {
		expect(statusHex('completed')).toBe('#16a34a');
		expect(statusHex('failed')).toBe('#dc2626');
		expect(statusHex('bogus-status')).toBe('#71717a');
		expect(statusHex(null)).toBe('#71717a');
	});
});

describe('capGraph', () => {
	it('passes small graphs through untouched', () => {
		const result = capGraph([node('a'), node('b')], [edge('e', 'a', 'b')]);
		expect(result.truncated).toBe(false);
		expect(result.nodes).toHaveLength(2);
	});
	it('truncates nodes and drops dangling edges', () => {
		const nodes = Array.from({ length: 5 }, (_, i) => node(`n${i}`));
		const edges = [edge('e1', 'n0', 'n1'), edge('e2', 'n3', 'n4')];
		const result = capGraph(nodes, edges, 3);
		expect(result.truncated).toBe(true);
		expect(result.total).toBe(5);
		expect(result.nodes).toHaveLength(3);
		expect(result.edges).toEqual([edges[0]]);
	});
});

describe('columnPositions', () => {
	it('groups nodes by iteration column', () => {
		const positions = columnPositions([
			node('a', 'decision', { iteration: 0 }),
			node('b', 'decision', { iteration: 0 }),
			node('c', 'decision', { iteration: 1 }),
		]);
		expect(positions.get('a')).toEqual({ x: 40, y: 40 });
		expect(positions.get('b')).toEqual({ x: 40, y: 112 });
		expect(positions.get('c')).toEqual({ x: 240, y: 40 });
	});
});

describe('distinctKinds and legend', () => {
	it('lists sorted kinds', () => {
		expect(distinctKinds([node('a', 'b'), node('b', 'a')])).toEqual([
			'a',
			'b',
		]);
	});
	it('adds error legend entries for decision graphs', () => {
		expect(legendFor('decision').some((entry) => entry.label === 'Error')).toBe(
			true,
		);
		expect(legendFor('workflow').some((entry) => entry.label === 'Error')).toBe(
			false,
		);
	});
});

describe('shortLabel', () => {
	it('truncates long labels', () => {
		expect(shortLabel('a'.repeat(30))).toHaveLength(18);
		expect(shortLabel('ok')).toBe('ok');
		expect(shortLabel('')).toBe('unnamed');
	});
});
