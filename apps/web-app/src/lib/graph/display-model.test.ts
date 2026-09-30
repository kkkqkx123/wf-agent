import { describe, expect, it } from 'vitest';
import {
	connectedComponents,
	columnPositions,
	groupAwareLayeredPositions,
	layeredPositions,
	pushOverlapped,
	snapToGrid,
	sortIdsByCanvasPosition,
} from './layout';
import {
	capGraph,
	connectByPort,
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
	buildVersionDiffView,
	diffTopology,
	issueNodeIds,
	matchDecisionNodeId,
	nodesForIteration,
	projectEdgeOverlay,
	projectEdgeTone,
	projectExecutionOverlay,
	slowHeatTier,
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
		expect(nodeShape('START_FROM_MESSAGE', 'workflow')).toBe('ellipse');
		expect(nodeShape('CONTINUE_FROM_MESSAGE', 'workflow')).toBe('ellipse');
	});
	it('maps route nodes to diamonds', () => {
		expect(nodeShape('route', 'decision')).toBe('diamond');
		expect(nodeShape('ROUTE', 'workflow')).toBe('diamond');
	});
	it('maps model calls to hexagons outside workflows', () => {
		expect(nodeShape('LLM', 'decision')).toBe('hexagon');
		expect(nodeShape('LLM', 'workflow')).toBe('round-rectangle');
	});
	it('renders unknown and plugin kinds as ordinary steps', () => {
		expect(nodeShape('error', 'decision')).toBe('round-rectangle');
		expect(nodeShape('tool_call', 'decision')).toBe('round-rectangle');
		expect(nodeShape('acme.step', 'workflow')).toBe('round-rectangle');
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
	it('retains priority ids before sampling leftovers', () => {
		const nodes = [
			node('a0', 'a'),
			node('a1', 'a'),
			node('a2', 'a'),
			node('b0', 'b'),
		];
		const result = capGraph(nodes, [], 3, ['b0', 'a2']);
		expect(result.truncated).toBe(true);
		expect(result.nodes.map((entry) => entry.id)).toEqual(['b0', 'a2', 'a0']);
	});
	it('keeps retained ids within the budget when they exceed it', () => {
		const nodes = Array.from({ length: 5 }, (_, i) => node(`n${i}`));
		const result = capGraph(nodes, [], 2, ['n4', 'n3', 'n2']);
		expect(result.truncated).toBe(true);
		expect(result.nodes.map((entry) => entry.id)).toEqual(['n4', 'n3']);
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
	it('classifies builtin entry and exit types as terminals', () => {
		expect(renderKind('START')).toBe('terminal');
		expect(renderKind('END')).toBe('terminal');
		expect(renderKind('START_FROM_MESSAGE')).toBe('terminal');
		expect(renderKind('CONTINUE_FROM_MESSAGE')).toBe('terminal');
	});
	it('classifies route nodes as decisions and model calls as tools', () => {
		expect(renderKind('ROUTE')).toBe('decision');
		expect(renderKind('llm')).toBe('tool');
	});
	it('treats every other builtin type as a step', () => {
		expect(renderKind('SCRIPT')).toBe('step');
		expect(renderKind('AGENT_LOOP')).toBe('step');
		expect(renderKind('LOOP_START')).toBe('step');
		expect(renderKind('USER_INTERACTION')).toBe('step');
	});
	it('never invents a role for kinds the backend does not define', () => {
		for (const kind of [
			'webhook',
			'cron',
			'trigger',
			'note',
			'comment',
			'subagent',
			'start_node',
			'error',
			'branch',
		]) {
			expect(renderKind(kind)).toBe('step');
		}
	});
	it('renders plugin-contributed types as steps', () => {
		expect(renderKind('acme.step')).toBe('step');
		expect(renderKind('')).toBe('step');
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
				node('z-tool', 'LLM'),
				node('m-tool', 'LLM'),
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
	it('positions edge-free nodes instead of dropping them', () => {
		const positions = layeredPositions(
			[node('a', 'SCRIPT'), node('b', 'LLM'), node('c', 'acme.step')],
			[],
		);
		expect(positions.size).toBe(3);
		for (const position of positions.values()) {
			expect(Number.isFinite(position.x)).toBe(true);
			expect(Number.isFinite(position.y)).toBe(true);
		}
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

describe('pushOverlapped', () => {
	it('pushes stationary nodes out of moved boxes', () => {
		const pushed = pushOverlapped([{ id: 'a', position: { x: 100, y: 100 } }], {
			a: { x: 0, y: 0 },
			b: { x: 110, y: 100 },
		});
		expect(pushed.map((entry) => entry.id)).toEqual(['b']);
		expect(pushed[0]?.position.x % 20).toBe(0);
	});
	it('leaves distant nodes alone', () => {
		expect(
			pushOverlapped([{ id: 'a', position: { x: 0, y: 0 } }], {
				a: { x: 0, y: 0 },
				b: { x: 500, y: 500 },
			}),
		).toEqual([]);
	});
});

describe('groupAwareLayeredPositions', () => {
	it('keeps grouped members compact', () => {
		const nodes = [
			node('a', 'step', { groupId: 'g' }),
			node('b', 'step', { groupId: 'g' }),
			node('c', 'step'),
		];
		const edges = [edge('e1', 'a', 'b'), edge('e2', 'b', 'c')];
		const positions = groupAwareLayeredPositions(nodes, edges, {
			a: 'g',
			b: 'g',
		});
		expect(positions.size).toBe(3);
		const ax = positions.get('a')?.x ?? 0;
		const bx = positions.get('b')?.x ?? 0;
		expect(Math.abs(bx - ax)).toBeLessThan(600);
	});
	it('falls back to layered layout without groups', () => {
		const nodes = [node('a'), node('b')];
		const positions = groupAwareLayeredPositions(
			nodes,
			[edge('e1', 'a', 'b')],
			{},
		);
		expect(positions.get('b')?.x ?? 0).toBeGreaterThan(
			positions.get('a')?.x ?? 0,
		);
	});
});

describe('sortIdsByCanvasPosition', () => {
	it('orders left to right then top to bottom', () => {
		const ids = ['c', 'a', 'b'];
		const positions = new Map([
			['a', { x: 0, y: 100 }],
			['b', { x: 0, y: 20 }],
			['c', { x: 200, y: 0 }],
		]);
		expect(sortIdsByCanvasPosition(ids, positions)).toEqual(['b', 'a', 'c']);
	});
	it('keeps input order for missing positions', () => {
		const ids = ['b', 'a', 'c'];
		expect(
			sortIdsByCanvasPosition(ids, new Map([['a', { x: 0, y: 0 }]])),
		).toEqual(['b', 'a', 'c']);
	});
	it('accepts record positions and passes through without positions', () => {
		const ids = ['b', 'a'];
		expect(
			sortIdsByCanvasPosition(ids, {
				a: { x: 0, y: 0 },
				b: { x: 100, y: 0 },
			}),
		).toEqual(['a', 'b']);
		expect(sortIdsByCanvasPosition(ids, undefined)).toEqual(['b', 'a']);
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
	it('tiers slow nodes by duration share', () => {
		const nodes = [node('a'), node('b'), node('c')];
		const overlay = projectExecutionOverlay(nodes, {
			slowNodes: [
				{ node: 'a', durationMs: 100 },
				{ node: 'b', durationMs: 500 },
				{ node: 'c', durationMs: 900 },
			],
		});
		expect(overlay.marks.get('a')?.heatTier).toBe(1);
		expect(overlay.marks.get('b')?.heatTier).toBe(2);
		expect(overlay.marks.get('c')?.heatTier).toBe(3);
		expect(slowHeatTier(0, 900)).toBe(0);
		expect(slowHeatTier(900, 900)).toBe(3);
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
		expect(diff.addedEdges).toEqual([{ source: 'b', target: 'c' }]);
		expect(diff.removedEdges).toEqual([{ source: 'a', target: 'b' }]);
	});
});

describe('matchDecisionNodeId', () => {
	it('prefers exact id or label matches', () => {
		const nodes = [node('a'), node('b')];
		expect(matchDecisionNodeId(nodes, 'A')).toBe('a');
		expect(matchDecisionNodeId(nodes, 'b')).toBe('b');
		expect(matchDecisionNodeId(nodes, '  ')).toBeNull();
	});
	it('falls back to substring matches', () => {
		const nodes = [{ id: 'tool-search', label: 'Web Search' }];
		expect(matchDecisionNodeId(nodes, 'search')).toBe('tool-search');
		expect(matchDecisionNodeId(nodes, 'unknown-tool')).toBeNull();
	});
});

describe('nodesForIteration', () => {
	it('collects ids of one iteration column', () => {
		const nodes = [
			node('a', 'decision', { iteration: 0 }),
			node('b', 'decision', { iteration: 1 }),
		];
		expect(nodesForIteration(nodes, 1)).toEqual(['b']);
		expect(nodesForIteration(nodes, null)).toEqual([]);
	});
});

describe('buildVersionDiffView', () => {
	it('returns empty passthrough without a diff', () => {
		const nodes = [node('a')];
		const edges = [edge('e', 'a', 'a')];
		const view = buildVersionDiffView(nodes, edges, null);
		expect(view.empty).toBe(true);
		expect(view.nodes).toBe(nodes);
	});
	it('styles added edges and placeholders removed ones', () => {
		const view = buildVersionDiffView([node('a'), node('b')], [], {
			addedNodes: [],
			removedNodes: ['gone'],
			addedEdges: [],
			removedEdges: [{ source: 'a', target: 'gone' }],
		});
		expect(view.removedNodes).toBe(1);
		expect(view.nodes.some((entry) => entry.id === 'gone')).toBe(true);
		expect(view.edges[0]?.label).toContain('−');
		expect(view.empty).toBe(false);
	});
	it('merges parallel removed edges with a count', () => {
		const view = buildVersionDiffView([node('a'), node('b')], [], {
			addedNodes: [],
			removedNodes: [],
			addedEdges: [],
			removedEdges: [
				{ source: 'a', target: 'b' },
				{ source: 'a', target: 'b' },
			],
		});
		expect(view.edges).toHaveLength(1);
		expect(view.edges[0]?.label).toContain('2 links');
		expect(view.mergedGroups).toBe(1);
	});
	it('reports an empty diff with zero counts', () => {
		const view = buildVersionDiffView([node('a')], [], {
			addedNodes: [],
			removedNodes: [],
			addedEdges: [],
			removedEdges: [],
		});
		expect(view.empty).toBe(true);
		expect(view.addedEdges).toBe(0);
	});
});

describe('connectByPort', () => {
	it('rejects incoming edges into graph entries', () => {
		expect(connectByPort({ sourceKind: 'LLM', targetKind: 'START' })).toBe(
			'START node cannot have incoming edges',
		);
		expect(
			connectByPort({ sourceKind: 'SCRIPT', targetKind: 'start_from_message' }),
		).toBe('START_FROM_MESSAGE node cannot have incoming edges');
	});
	it('rejects outgoing edges from graph exits', () => {
		expect(connectByPort({ sourceKind: 'END', targetKind: 'SCRIPT' })).toBe(
			'END node cannot have outgoing edges',
		);
		expect(
			connectByPort({
				sourceKind: 'continue_from_message',
				targetKind: 'SCRIPT',
			}),
		).toBe('CONTINUE_FROM_MESSAGE node cannot have outgoing edges');
	});
	it('allows edges between ordinary kinds', () => {
		expect(
			connectByPort({ sourceKind: 'LLM', targetKind: 'SCRIPT' }),
		).toBeNull();
		expect(
			connectByPort({ sourceKind: 'SCRIPT', targetKind: 'END' }),
		).toBeNull();
		expect(
			connectByPort({ sourceKind: 'START', targetKind: 'LLM' }),
		).toBeNull();
	});
	it('treats unrecognised kinds as ordinary nodes', () => {
		expect(
			connectByPort({ sourceKind: 'plugin-step', targetKind: 'SCRIPT' }),
		).toBeNull();
		expect(connectByPort({ sourceKind: '', targetKind: 'START' })).toBe(
			'START node cannot have incoming edges',
		);
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
