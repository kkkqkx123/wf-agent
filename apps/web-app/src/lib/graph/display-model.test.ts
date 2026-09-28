import { describe, expect, it } from 'vitest';
import {
	connectedComponents,
	columnPositions,
	layeredPositions,
	snapToGrid,
} from './layout';
import {
	capGraph,
	distinctKinds,
	isDashedEdge,
	legendFor,
	nodeShape,
	rankTone,
	renderKind,
	shortLabel,
	statusForTone,
	statusHex,
	toneForStatus,
	type DisplayEdge,
	type DisplayNode,
} from './display-model';
import {
	applyEdgeOverlay,
	applyExecutionOverlay,
	diffTopology,
	issueNodeIds,
	projectEdgeOverlay,
	projectEdgeTone,
	projectExecutionOverlay,
} from './execution-projection';

function node(
	id: string,
	kind = 'llm',
	extra?: Partial<DisplayNode>,
): DisplayNode {
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
	it('samples across kinds instead of cutting the tail', () => {
		const nodes = [
			node('a0', 'a'),
			node('a1', 'a'),
			node('a2', 'a'),
			node('b0', 'b'),
		];
		const result = capGraph(nodes, [], 3);
		expect(result.truncated).toBe(true);
		expect(result.nodes.map((entry) => entry.id)).toEqual(['a0', 'b0', 'a1']);
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
		expect(positions.get('b')).toEqual({ x: 40, y: 120 });
		expect(positions.get('c')).toEqual({ x: 240, y: 40 });
	});
});

describe('distinctKinds and legend', () => {
	it('lists sorted kinds', () => {
		expect(distinctKinds([node('a', 'b'), node('b', 'a')])).toEqual(['a', 'b']);
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

describe('renderKind', () => {
	it('classifies terminals, tools, triggers and agents', () => {
		expect(renderKind('START', 'workflow')).toBe('terminal');
		expect(renderKind('start_node', 'workflow')).toBe('terminal');
		expect(renderKind('tool_call', 'decision')).toBe('tool');
		expect(renderKind('webhook', 'workflow')).toBe('trigger');
		expect(renderKind('subagent', 'decision')).toBe('agent');
		expect(renderKind('note', 'workflow')).toBe('note');
		expect(renderKind('custom', 'workflow')).toBe('step');
	});
	it('keeps decision shapes for branches and decision errors', () => {
		expect(renderKind('branch', 'workflow')).toBe('decision');
		expect(renderKind('error', 'decision')).toBe('decision');
		expect(renderKind('error', 'workflow')).toBe('step');
	});
});

describe('execution tones', () => {
	it('normalizes backend statuses to canonical tones', () => {
		expect(toneForStatus('in_progress')).toBe('running');
		expect(toneForStatus('timeout')).toBe('error');
		expect(toneForStatus('queued')).toBe('warning');
		expect(toneForStatus('succeeded')).toBe('success');
		expect(toneForStatus('bogus')).toBe('neutral');
		expect(toneForStatus(null)).toBe('neutral');
	});
	it('ranks running above error above warning above success', () => {
		expect(rankTone('success', 'warning')).toBe('warning');
		expect(rankTone('warning', 'error')).toBe('error');
		expect(rankTone('error', 'running')).toBe('running');
		expect(rankTone('running', 'success')).toBe('running');
		expect(statusForTone('running')).toBe('running');
		expect(statusForTone('neutral')).toBeUndefined();
	});
});

describe('layeredPositions', () => {
	it('flows chains left to right on the grid', () => {
		const nodes = [node('a'), node('b'), node('c')];
		const positions = layeredPositions(nodes, [
			edge('e1', 'a', 'b'),
			edge('e2', 'b', 'c'),
		]);
		expect(positions.size).toBe(3);
		const ax = positions.get('a')?.x ?? 0;
		const bx = positions.get('b')?.x ?? 0;
		const cx = positions.get('c')?.x ?? 0;
		expect(bx).toBeGreaterThan(ax);
		expect(cx).toBeGreaterThan(bx);
		for (const position of positions.values()) {
			expect(position.x % 20).toBe(0);
			expect(position.y % 20).toBe(0);
		}
	});
	it('places cyclic leftovers instead of dropping them', () => {
		const nodes = [node('a'), node('b')];
		const positions = layeredPositions(nodes, [
			edge('e1', 'a', 'b'),
			edge('e2', 'b', 'a'),
		]);
		expect(positions.size).toBe(2);
	});
	it('lays disconnected components side by side', () => {
		const positions = layeredPositions(
			[node('a', 'step'), node('b', 'step'), node('c', 'step')],
			[edge('e1', 'a', 'b')],
		);
		const ax = positions.get('a')?.x ?? 0;
		const cx = positions.get('c')?.x ?? 0;
		expect(cx).toBeGreaterThan(ax);
		expect((positions.get('c')?.x ?? 0) % 20).toBe(0);
		expect((positions.get('c')?.y ?? 0) % 20).toBe(0);
	});
	it('lanes tool calls under their caller', () => {
		const positions = layeredPositions(
			[
				node('r', 'step'),
				node('a-step', 'step'),
				node('b-step', 'step'),
				node('z-tool', 'tool_call'),
				node('m-tool', 'tool_call'),
			],
			[
				edge('e1', 'r', 'a-step'),
				edge('e2', 'r', 'b-step'),
				edge('e3', 'a-step', 'z-tool'),
				edge('e4', 'b-step', 'm-tool'),
			],
		);
		const callerA = positions.get('a-step');
		const callerB = positions.get('b-step');
		const zed = positions.get('z-tool');
		const em = positions.get('m-tool');
		expect(zed?.x ?? 0).toBeGreaterThan(callerA?.x ?? 0);
		expect(em?.x ?? 0).toBeGreaterThan(callerB?.x ?? 0);
		expect(zed?.x).toBe(em?.x);
		expect(zed?.y ?? 0).toBeLessThan(em?.y ?? 0);
	});
	it('docks notes below their component', () => {
		const positions = layeredPositions(
			[node('a', 'step'), node('n', 'note')],
			[edge('e1', 'a', 'n')],
		);
		expect(positions.get('n')?.x).toBe(positions.get('a')?.x);
		expect(positions.get('n')?.y ?? 0).toBeGreaterThan(
			positions.get('a')?.y ?? 0,
		);
	});
});

describe('connectedComponents', () => {
	it('splits isolated nodes into their own components', () => {
		expect(
			connectedComponents(
				[node('b'), node('a'), node('c')],
				[edge('e', 'a', 'b')],
			),
		).toEqual([['a', 'b'], ['c']]);
	});
	it('ignores dangling and self edges', () => {
		expect(
			connectedComponents(
				[node('a')],
				[edge('e1', 'a', 'a'), edge('e2', 'a', 'ghost')],
			),
		).toEqual([['a']]);
	});
});

describe('snapToGrid', () => {
	it('rounds to the grid', () => {
		expect(snapToGrid(41)).toBe(40);
		expect(snapToGrid(260)).toBe(260);
	});
});

describe('projectExecutionOverlay', () => {
	it('merges signals by priority and pulses the current node', () => {
		const nodes = [node('a'), node('b'), node('c')];
		const overlay = projectExecutionOverlay(nodes, {
			currentNode: 'b',
			failedNodes: ['a', 'b'],
			criticalPath: ['b'],
			executedNodes: ['a'],
		});
		expect(overlay.marks.get('b')?.tone).toBe('running');
		expect(overlay.marks.get('b')?.pulse).toBe(true);
		expect(overlay.marks.get('b')?.critical).toBe(true);
		expect(overlay.marks.get('a')?.tone).toBe('error');
		expect(overlay.marks.get('c')?.tone).toBe('neutral');
		const applied = applyExecutionOverlay(nodes, overlay);
		expect(applied.find((entry) => entry.id === 'b')?.status).toBe('running');
		expect(applied.find((entry) => entry.id === 'c')?.status).toBeUndefined();
	});
	it('derives edge tones from the source node', () => {
		expect(projectEdgeTone('error', true)).toBe('error');
		expect(projectEdgeTone('running', false)).toBe('running');
		expect(projectEdgeTone('success', true)).toBe('success');
		expect(projectEdgeTone('success', false)).toBe('neutral');
	});
	it('projects edge overlays from node marks', () => {
		const nodes = [node('a'), node('b'), node('c')];
		const overlay = projectExecutionOverlay(nodes, {
			currentNode: 'a',
			failedNodes: ['b'],
			executedNodes: ['a', 'c'],
		});
		const tones = projectEdgeOverlay(
			[edge('e1', 'a', 'c'), edge('e2', 'b', 'c'), edge('e3', 'c', 'a')],
			overlay,
		);
		expect(tones.get('e1')).toBe('running');
		expect(tones.get('e2')).toBe('error');
		expect(tones.get('e3')).toBe('success');
		const applied = applyEdgeOverlay(
			[edge('e1', 'a', 'c'), edge('e2', 'b', 'c')],
			tones,
		);
		expect(applied.find((entry) => entry.id === 'e1')?.status).toBe('running');
		expect(applied.find((entry) => entry.id === 'e2')?.status).toBe('failed');
	});
});

describe('diffTopology', () => {
	it('reports added and removed nodes and edges', () => {
		const diff = diffTopology(
			[node('a'), node('b')],
			[edge('e1', 'a', 'b')],
			[node('b'), node('c')],
			[edge('e2', 'b', 'c')],
		);
		expect(diff.addedNodes).toEqual(['c']);
		expect(diff.removedNodes).toEqual(['a']);
		expect(diff.addedEdges).toEqual(['b->c']);
		expect(diff.removedEdges).toEqual(['a->b']);
	});
});

describe('issueNodeIds', () => {
	it('maps dotted field paths to node ids', () => {
		const matched = issueNodeIds(
			[
				{ field: 'nodesbly.name', message: 'required' },
				{ field: 'nodes.b.name', message: 'unknown kind' },
			],
			[node('a'), node('b')],
		);
		expect(matched.get('b')).toEqual(['unknown kind']);
		expect(matched.has('a')).toBe(false);
	});
});
