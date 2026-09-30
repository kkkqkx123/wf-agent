import { describe, expect, it } from 'vitest';
import {
	filterNodeTraces,
	isFailedNodeTrace,
	summarizeNodeTraces,
	toLlmReasoningStep,
	toNodeInputContext,
	toNodeTrace,
} from '$lib/services/node-trace';
import type { NodeTrace } from '$lib/types/models';

function trace(overrides: Partial<NodeTrace> = {}): NodeTrace {
	return {
		executionId: 'exec-1',
		nodeId: 'n1',
		nodeName: 'Fetch',
		nodeType: 'SCRIPT',
		status: 'completed',
		startedAt: '2026-01-01T00:00:00.000Z',
		endedAt: '2026-01-01T00:00:01.000Z',
		durationMs: 1000,
		input: { a: 1 },
		output: { b: 2 },
		retryCount: 0,
		error: null,
		toolDependencies: [],
		...overrides,
	};
}

describe('toNodeTrace', () => {
	it('maps the backend record onto the view model', () => {
		const mapped = toNodeTrace({
			execution_id: 'exec-1',
			node_id: 'n1',
			node_name: 'Fetch',
			node_type: 'SCRIPT',
			status: 'completed',
			start_time: 0,
			end_time: 1000,
			duration: 1000,
			retry_count: 2,
			tool_dependencies: [{ tool_name: 'bash', call_count: 3 }],
		});
		expect(mapped).toEqual({
			executionId: 'exec-1',
			nodeId: 'n1',
			nodeName: 'Fetch',
			nodeType: 'SCRIPT',
			status: 'completed',
			startedAt: '1970-01-01T00:00:00.000Z',
			endedAt: '1970-01-01T00:00:01.000Z',
			durationMs: 1000,
			input: null,
			output: null,
			retryCount: 2,
			error: null,
			toolDependencies: [{ toolName: 'bash', callCount: 3 }],
		});
	});

	it('drops rows that carry no node id', () => {
		expect(toNodeTrace({ node_name: 'orphan' })).toBeNull();
		expect(toNodeTrace({ node_id: '   ' })).toBeNull();
	});

	it('keeps absent optional fields absent instead of inventing values', () => {
		const mapped = toNodeTrace({ node_id: 'n1' });
		expect(mapped?.endedAt).toBeNull();
		expect(mapped?.durationMs).toBeNull();
		expect(mapped?.error).toBeNull();
		expect(mapped?.startedAt).toBe('');
	});
});

describe('toNodeInputContext', () => {
	it('flattens parameters and variables into renderable pairs', () => {
		const mapped = toNodeInputContext({
			node_id: 'n1',
			node_name: 'Summarize',
			node_type: 'LLM',
			input_parameters: { prompt: 'hi', limit: 2 },
			available_variables: [{ name: 'x', value: { a: 1 }, type: 'json' }],
			timestamp: 1000,
		});
		expect(mapped?.inputParameters).toEqual([
			{ key: 'prompt', value: 'hi' },
			{ key: 'limit', value: '2' },
		]);
		expect(mapped?.availableVariables).toEqual([
			{ name: 'x', value: '{"a":1}', type: 'json', source: null },
		]);
		expect(mapped?.recordedAt).toBe('1970-01-01T00:00:01.000Z');
	});

	it('rejects a payload without a node id', () => {
		expect(toNodeInputContext({ node_name: 'x' })).toBeNull();
	});
});

describe('toLlmReasoningStep', () => {
	it('falls back to a positional id so keyed blocks stay stable', () => {
		const step = toLlmReasoningStep({ reasoning_type: 'planning' }, 3);
		expect(step.stepId).toBe('reasoning-3');
		expect(step.confidence).toBeNull();
		expect(step.conclusions).toEqual([]);
	});
});

describe('filterNodeTraces', () => {
	const rows = [
		trace({ nodeId: 'a', nodeName: 'Fetch', status: 'completed' }),
		trace({
			nodeId: 'b',
			nodeName: 'Summarize',
			status: 'failed',
			retryCount: 2,
		}),
		trace({ nodeId: 'c', nodeName: 'Decide', status: 'skipped' }),
	];

	it('matches every status vocabulary the backend uses for failures', () => {
		expect(filterNodeTraces(rows, 'failed', '').map((r) => r.nodeId)).toEqual([
			'b',
		]);
		expect(isFailedNodeTrace(trace({ status: 'Errored' }))).toBe(true);
		expect(isFailedNodeTrace(trace({ status: 'completed' }))).toBe(false);
	});

	it('filters on exact status for non-failure buckets', () => {
		expect(filterNodeTraces(rows, 'skipped', '').map((r) => r.nodeId)).toEqual([
			'c',
		]);
	});

	it('searches id, name and type case-insensitively', () => {
		expect(
			filterNodeTraces(rows, 'all', 'SUMMAR').map((r) => r.nodeId),
		).toEqual(['b']);
		expect(
			filterNodeTraces(rows, 'all', 'script').map((r) => r.nodeId),
		).toHaveLength(3);
	});
});

describe('summarizeNodeTraces', () => {
	it('totals rows, failures and retries', () => {
		expect(
			summarizeNodeTraces([
				trace({ nodeId: 'a' }),
				trace({ nodeId: 'b', status: 'failed', retryCount: 2 }),
				trace({ nodeId: 'c', status: 'timeout', retryCount: 1 }),
			]),
		).toEqual({ total: 3, failed: 2, retries: 3 });
	});
});
