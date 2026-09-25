import { openGetStream, openPostStream } from '$lib/api/stream';
import type { RunLoopMessage } from '$lib/services/agent-loops';

export interface ToolLifecycle {
	toolCallId: string;
	toolName: string;
	success: boolean;
	result: string;
	error: string | null;
}

export interface UsageSnapshot {
	promptTokens: number;
	completionTokens: number;
	cost: number | null;
}

export interface StreamCallbacks {
	onDelta?: (text: string) => void;
	onReasoning?: (text: string) => void;
	onIterationStart?: (iteration: number) => void;
	onIterationEnd?: (iteration: number) => void;
	onToolStart?: (toolCallId: string, toolName: string) => void;
	onToolEnd?: (tool: ToolLifecycle) => void;
	onUsage?: (usage: UsageSnapshot) => void;
	onSubAgent?: (id: string, name: string, success: boolean | null) => void;
	onCompleted?: (iterations: number) => void;
	onFailed?: (message: string) => void;
	onInterrupted?: (reason: string) => void;
	onError?: (message: string) => void;
}

interface Frame {
	type?: unknown;
	event_type?: unknown;
	[key: string]: unknown;
}

function asRecord(data: unknown): Frame | null {
	if (!data || typeof data !== 'object') return null;
	return data as Frame;
}

function asString(value: unknown): string {
	return typeof value === 'string' ? value : '';
}

function asNumber(value: unknown, fallback = 0): number {
	return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

/** Both the protocol tag and the legacy payload field name frames. */
function frameKind(frame: Frame): string {
	if (typeof frame.type === 'string') return frame.type;
	if (typeof frame.event_type === 'string') return frame.event_type;
	return 'unknown';
}

/** Generation frames carry bare text under several field names. */
function frameText(frame: Frame): string {
	for (const key of ['content', 'delta', 'text']) {
		const text = asString(frame[key]);
		if (text) return text;
	}
	return '';
}

/**
 * Route one parsed frame to the matching callback. Unknown frames are
 * ignored so newer server event kinds never break the timeline.
 */
export function handleStreamFrame(
	event: string,
	data: unknown,
	callbacks: StreamCallbacks,
): void {
	const frame = asRecord(data);
	if (!frame) {
		if (typeof data === 'string' && data) callbacks.onDelta?.(data);
		return;
	}
	if (event === 'metadata') return;
	const kind = frameKind(frame);
	switch (kind) {
		case 'llm_delta':
			if (frameText(frame)) callbacks.onDelta?.(frameText(frame));
			break;
		case 'reasoning_delta':
			if (frameText(frame)) callbacks.onReasoning?.(frameText(frame));
			break;
		case 'iteration_start':
			callbacks.onIterationStart?.(asNumber(frame.iteration, 1));
			break;
		case 'iteration_end':
			callbacks.onIterationEnd?.(asNumber(frame.iteration, 0));
			break;
		case 'tool_start':
			callbacks.onToolStart?.(
				asString(frame.tool_call_id),
				asString(frame.tool_name),
			);
			break;
		case 'tool_end':
			callbacks.onToolEnd?.({
				toolCallId: asString(frame.tool_call_id),
				toolName: asString(frame.tool_name),
				success: frame.success !== false,
				result: asString(frame.result),
				error:
					typeof frame.error === 'string' && frame.error ? frame.error : null,
			});
			break;
		case 'usage':
			callbacks.onUsage?.({
				promptTokens: asNumber(frame.prompt_tokens),
				completionTokens: asNumber(frame.completion_tokens),
				cost: typeof frame.cost === 'number' ? frame.cost : null,
			});
			break;
		case 'sub_agent_started':
			callbacks.onSubAgent?.(asString(frame.id), asString(frame.name), null);
			break;
		case 'sub_agent_ended':
			callbacks.onSubAgent?.(
				asString(frame.id),
				asString(frame.name),
				frame.success !== false,
			);
			break;
		case 'completed':
			callbacks.onCompleted?.(asNumber(frame.iterations));
			break;
		case 'failed':
			callbacks.onFailed?.(asString(frame.error) || 'Run failed');
			break;
		case 'interrupted':
			callbacks.onInterrupted?.(asString(frame.reason) || 'Interrupted');
			break;
		case 'error':
			callbacks.onError?.(asString(frame.error) || 'Stream error');
			break;
		case 'engine':
			break;
		default: {
			const text = frameText(frame);
			if (text) callbacks.onDelta?.(text);
			break;
		}
	}
}

export interface LoopStreamBody {
	model: string;
	message: string;
	conversation?: RunLoopMessage[];
}

/**
 * Stream an agent loop run. Resolves once the server closes the stream;
 * terminal outcomes arrive through the matching callbacks.
 */
export function streamLoopRun(
	id: string,
	body: LoopStreamBody,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openPostStream({
		path: `/api/v1/agent-loops/${id}/stream`,
		body: {
			model: body.model,
			message: body.message,
			tool_call_protocol: { format: 'json' },
			conversation: body.conversation ?? [],
		},
		signal,
		onFrame: (event, data) => handleStreamFrame(event, data, callbacks),
		onError: (message) => callbacks.onError?.(message),
	});
}

/** Stream a single generation request for the compose box preview lane. */
export function streamGeneration(
	body: Record<string, unknown>,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openPostStream({
		path: '/api/v1/llm/generate-stream',
		body,
		signal,
		onFrame: (event, data) => handleStreamFrame(event, data, callbacks),
		onError: (message) => callbacks.onError?.(message),
	});
}

/** Stream a workflow execution; the leading metadata frame is skipped. */
export function streamWorkflowExecution(
	id: string,
	body: Record<string, unknown>,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openPostStream({
		path: `/api/v1/workflows/${id}/execute/stream`,
		body,
		signal,
		onFrame: (event, data) => handleStreamFrame(event, data, callbacks),
		onError: (message) => callbacks.onError?.(message),
	});
}

/** Follow the read-only error analysis stream for one execution. */
export function streamErrorAnalysis(
	id: string,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openGetStream({
		path: `/api/v1/executions/${id}/error-analysis/stream`,
		signal,
		onFrame: (event, data) => handleStreamFrame(event, data, callbacks),
		onError: (message) => callbacks.onError?.(message),
	});
}
