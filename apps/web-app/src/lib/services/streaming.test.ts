import { describe, expect, it, vi } from 'vitest';
import { handleExecutionFrame, handleGenerationFrame } from './streaming';
import type { StreamCallbacks } from './streaming';

function collect(): { callbacks: StreamCallbacks; calls: string[] } {
	const calls: string[] = [];
	return {
		calls,
		callbacks: {
			onExecution: (id) => calls.push(`execution:${id}`),
			onDelta: (text) => calls.push(`delta:${text}`),
			onReasoning: (text) => calls.push(`reasoning:${text}`),
			onNode: (node) =>
				calls.push(`node:${node.id}:${node.status}:${node.durationMs}`),
			onToolStart: (id, name) => calls.push(`toolStart:${id}:${name}`),
			onUsage: (usage) =>
				calls.push(`usage:${usage.promptTokens}:${usage.completionTokens}:${usage.cost}`),
			onInterrupted: (reason) => calls.push(`interrupted:${reason}`),
			onError: (failure) => calls.push(`error:${failure.message}`),
		},
	};
}

describe('handleExecutionFrame', () => {
	it('reports the execution id from the metadata handshake', () => {
		const { callbacks, calls } = collect();
		handleExecutionFrame('metadata', { execution_id: 'exec-1' }, callbacks);
		expect(calls).toEqual(['execution:exec-1']);
	});

	it('maps engine bus frames to node updates', () => {
		const { callbacks, calls } = collect();
		handleExecutionFrame(
			'engine',
			{
				type: 'NODE_COMPLETED',
				metadata: { node_id: 'n1', node_name: 'Fetch', duration_ms: 12 },
			},
			callbacks,
		);
		expect(calls).toEqual(['node:n1:completed:12']);
	});

	it('ignores engine bus events the timeline does not model', () => {
		const spy = vi.fn();
		handleExecutionFrame(
			'engine',
			{ type: 'CHECKPOINT_SAVED', metadata: {} },
			{ onNode: spy },
		);
		expect(spy).not.toHaveBeenCalled();
	});

	it('reads the increments of the answer and the reasoning separately', () => {
		const { callbacks, calls } = collect();
		handleExecutionFrame('', { type: 'llm_delta', content: 'hi' }, callbacks);
		handleExecutionFrame(
			'',
			{ type: 'reasoning_delta', content: 'think' },
			callbacks,
		);
		expect(calls).toEqual(['delta:hi', 'reasoning:think']);
	});

	it('ignores frames of the generation protocol', () => {
		const spy = vi.fn();
		handleExecutionFrame(
			'',
			{ event_type: 'text', text: 'not mine' },
			{ onDelta: spy },
		);
		expect(spy).not.toHaveBeenCalled();
	});
});

describe('handleGenerationFrame', () => {
	it('appends only the increment of a text frame', () => {
		const { callbacks, calls } = collect();
		handleGenerationFrame(
			{ event_type: 'text', text: 'wor', snapshot: 'hello wor' },
			callbacks,
		);
		expect(calls).toEqual(['delta:wor']);
	});

	it('keeps an in-stream error distinguishable from a frame', () => {
		const { callbacks, calls } = collect();
		handleGenerationFrame({ event_type: 'error', error: 'boom' }, callbacks);
		expect(calls).toEqual(['error:boom']);
	});

	it('routes reasoning and usage increments', () => {
		const { callbacks, calls } = collect();
		handleGenerationFrame(
			{ event_type: 'reasoning_text', reasoning: 'think', snapshot: 'think' },
			callbacks,
		);
		handleGenerationFrame(
			{
				event_type: 'usage',
				usage: { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 },
			},
			callbacks,
		);
		expect(calls).toEqual(['reasoning:think', 'usage:10:20:null']);
	});

	it('surfaces usage carried by the terminal message', () => {
		const { callbacks, calls } = collect();
		handleGenerationFrame(
			{
				event_type: 'final_message',
				message: { content: [] },
				usage: {
					prompt_tokens: 5,
					completion_tokens: 7,
					total_tokens: 12,
					total_cost: 0.001,
				},
			},
			callbacks,
		);
		expect(calls).toEqual(['usage:5:7:0.001']);
	});

	it('maps an abort frame to an interruption', () => {
		const { callbacks, calls } = collect();
		handleGenerationFrame({ event_type: 'abort', reason: 'stopped' }, callbacks);
		expect(calls).toEqual(['interrupted:stopped']);
	});

	it('ignores frames the timeline does not model', () => {
		const spy = vi.fn();
		handleGenerationFrame({ event_type: 'end' }, { onDelta: spy });
		handleGenerationFrame(
			{ event_type: 'final_message', message: { content: [] } },
			{ onDelta: spy },
		);
		expect(spy).not.toHaveBeenCalled();
	});
});

describe('tool lifecycle', () => {
	it('reports a started tool call by id and name', () => {
		const { callbacks, calls } = collect();
		handleExecutionFrame(
			'',
			{ type: 'tool_start', tool_call_id: 't1', tool_name: 'search' },
			callbacks,
		);
		expect(calls).toEqual(['toolStart:t1:search']);
	});
});
