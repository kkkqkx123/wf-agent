import { describe, expect, it, vi } from 'vitest';
import { handleExecutionFrame } from '$lib/utils/stream-parser';
import type { StreamCallbacks } from '$lib/utils/stream-parser';

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
				calls.push(
					`usage:${usage.promptTokens}:${usage.completionTokens}:${usage.cost}`,
				),
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

	it('passes the display payload through on tool end', () => {
		const seen: Array<{ kind: unknown; preview: unknown }> = [];
		handleExecutionFrame(
			'',
			{
				type: 'tool_end',
				tool_call_id: 't1',
				tool_name: 'execute_command',
				success: true,
				result: 'ok',
				display: {
					kind: 'shell_tail',
					preview: 'ok',
					truncated: false,
					hidden_lines: 0,
				},
			},
			{
				onToolEnd: (tool) =>
					seen.push({
						kind: tool.display?.kind,
						preview: tool.display?.preview,
					}),
			},
		);
		expect(seen).toEqual([{ kind: 'shell_tail', preview: 'ok' }]);
	});

	it('falls back to null display on unknown payloads', () => {
		const seen: unknown[] = [];
		handleExecutionFrame(
			'',
			{
				type: 'tool_end',
				tool_call_id: 't1',
				tool_name: 'execute_command',
				success: true,
				result: 'ok',
				display: { kind: 'mystery' },
			},
			{ onToolEnd: (tool) => seen.push(tool.display) },
		);
		expect(seen).toEqual([null]);
	});

	it('parses the background marker on tool end', () => {
		const seen: unknown[] = [];
		handleExecutionFrame(
			'',
			{
				type: 'tool_end',
				tool_call_id: 't1',
				tool_name: 'backend_shell',
				success: true,
				result: '{}',
				background: { session_id: 'sess-1', backgrounded: true },
			},
			{ onToolEnd: (tool) => seen.push(tool.background) },
		);
		expect(seen).toEqual([{ sessionId: 'sess-1', backgrounded: true }]);
	});

	it('reads missing background markers as null', () => {
		const seen: unknown[] = [];
		handleExecutionFrame(
			'',
			{
				type: 'tool_end',
				tool_call_id: 't1',
				tool_name: 'shell_output',
				success: true,
				result: 'ok',
			},
			{ onToolEnd: (tool) => seen.push(tool.background) },
		);
		expect(seen).toEqual([null]);
	});
});
