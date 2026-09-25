import { openGetStream, openPostStream } from '$lib/api/stream';
import type { StreamFailure } from '$lib/api/stream';
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

/** One node lifecycle frame forwarded from the engine event bus. */
export interface StreamNodeUpdate {
	id: string;
	name: string;
	status: 'running' | 'completed' | 'failed' | 'skipped';
	durationMs: number | null;
	error: string | null;
	at: string;
}

/** One error of an execution chain, root cause included. */
export interface ErrorRecord {
	id: string;
	error: string;
	errorType: string | null;
	nodeId: string | null;
	at: string;
	isRecoverable: boolean;
	recoveryAction: string | null;
	rootCauseId: string;
}

export interface StreamCallbacks {
	/** Id the execution was created under, from the handshake frame. */
	onExecution?: (executionId: string) => void;
	onDelta?: (text: string) => void;
	onReasoning?: (text: string) => void;
	onIterationStart?: (iteration: number) => void;
	onIterationEnd?: (iteration: number) => void;
	onToolStart?: (toolCallId: string, toolName: string) => void;
	onToolEnd?: (tool: ToolLifecycle) => void;
	onUsage?: (usage: UsageSnapshot) => void;
	onSubAgent?: (id: string, name: string, success: boolean | null) => void;
	onNode?: (node: StreamNodeUpdate) => void;
	onCompleted?: (iterations: number) => void;
	onFailed?: (message: string) => void;
	onInterrupted?: (reason: string) => void;
	onError?: (failure: StreamFailure) => void;
}

interface Frame {
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

function frameTimestamp(frame: Frame): string {
	return typeof frame.timestamp === 'number'
		? new Date(frame.timestamp).toISOString()
		: '';
}

function asOptionalNumber(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function asOptionalString(value: unknown): string | null {
	return typeof value === 'string' && value ? value : null;
}

/** Engine frames carry their node payload under `metadata`. */
function frameMetadata(frame: Frame): Record<string, unknown> {
	const metadata = frame.metadata;
	return metadata && typeof metadata === 'object'
		? (metadata as Record<string, unknown>)
		: {};
}

/**
 * Node lifecycle as reported by the engine bus. Other bus events (workflow,
 * checkpoint, trigger lifecycle) are not modelled by a run timeline.
 */
const NODE_STATUS: Record<string, StreamNodeUpdate['status']> = {
	NODE_STARTED: 'running',
	NODE_COMPLETED: 'completed',
	NODE_FAILED: 'failed',
	NODE_SKIPPED: 'skipped',
};

function toNodeUpdate(
	frame: Frame,
	status: StreamNodeUpdate['status'],
): StreamNodeUpdate {
	const metadata = frameMetadata(frame);
	const nodeId = asString(metadata.node_id);
	return {
		id: nodeId,
		name: asString(metadata.node_name) || nodeId,
		status,
		durationMs: asOptionalNumber(metadata.duration_ms),
		error: asOptionalString(metadata.error),
		at: frameTimestamp(frame),
	};
}

/**
 * Analysis frames are whole error records rather than protocol events, so
 * they are read straight from the payload instead of routed by frame kind.
 */
function toErrorRecord(data: unknown): ErrorRecord | null {
	const frame = asRecord(data);
	const id = asString(frame?.id);
	if (!frame || !id) return null;
	return {
		id,
		error: asString(frame.error),
		errorType: asOptionalString(frame.error_type),
		nodeId: asOptionalString(frame.node_id),
		at: frameTimestamp(frame),
		isRecoverable: frame.is_recoverable === true,
		recoveryAction: asOptionalString(frame.recovery_action),
		rootCauseId: asString(frame.root_cause_id),
	};
}

/** The wire `Message` the LLM endpoints require, identity and time included. */
function createUserMessage(text: string): RunLoopMessage {
	return {
		id: crypto.randomUUID(),
		role: 'user',
		content: text,
		timestamp: Date.now(),
	};
}

/**
 * Route one frame of the execution protocol, the stream behind agent loop runs
 * and workflow executions. Kinds the timeline does not model are ignored, so
 * newer server events never break a run.
 */
export function handleExecutionFrame(
	event: string,
	data: unknown,
	callbacks: StreamCallbacks,
): void {
	const frame = asRecord(data);
	if (!frame) return;
	if (event === 'metadata') {
		const executionId = asString(frame.execution_id);
		if (executionId) callbacks.onExecution?.(executionId);
		return;
	}
	if (event === 'engine') {
		// Engine bus events keep their bus event type as the frame kind and
		// carry the node payload under `metadata`.
		const status = NODE_STATUS[asString(frame.type)];
		if (status) callbacks.onNode?.(toNodeUpdate(frame, status));
		return;
	}
	switch (asString(frame.type)) {
		case 'llm_delta': {
			const text = asString(frame.content);
			if (text) callbacks.onDelta?.(text);
			break;
		}
		case 'reasoning_delta': {
			const text = asString(frame.content);
			if (text) callbacks.onReasoning?.(text);
			break;
		}
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
				error: asOptionalString(frame.error),
			});
			break;
		case 'usage':
			callbacks.onUsage?.({
				promptTokens: asNumber(frame.prompt_tokens),
				completionTokens: asNumber(frame.completion_tokens),
				cost: asOptionalNumber(frame.cost),
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
	}
}

/**
 * Route one frame of the single-generation protocol, the `event_type`-tagged
 * stream. The terminal `final_message` repeats the accumulated answer and the
 * `end` frame only closes the stream, so neither is modelled here.
 */
export function handleGenerationFrame(
	data: unknown,
	callbacks: StreamCallbacks,
): void {
	const frame = asRecord(data);
	if (!frame) return;
	switch (asString(frame.event_type)) {
		case 'text': {
			// The increment arrives alongside the snapshot of the whole streamed
			// text; only the increment is appended.
			const text = asString(frame.text);
			if (text) callbacks.onDelta?.(text);
			break;
		}
		case 'error':
			callbacks.onError?.({
				message: asString(frame.error) || 'Stream error',
				status: null,
				retryAfterMs: null,
			});
			break;
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
		onFrame: (event, data) => handleExecutionFrame(event, data, callbacks),
		onError: (failure) => callbacks.onError?.(failure),
	});
}

/**
 * Body of the generate stream. The server endpoint takes the whole `LlmRequest`,
 * but only these two are sent: everything model-shaped is left for the profile
 * to decide.
 */
export interface GenerationBody {
	profileId: string;
	prompt: string;
}

/** Stream a single generation request; frames report progress. */
export function streamGeneration(
	body: GenerationBody,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openPostStream({
		path: '/api/v1/llm/generate-stream',
		body: {
			profile_id: body.profileId,
			messages: [createUserMessage(body.prompt)],
		},
		signal,
		onFrame: (_event, data) => handleGenerationFrame(data, callbacks),
		onError: (failure) => callbacks.onError?.(failure),
	});
}

/** Stream a workflow execution, handshake metadata included. */
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
		onFrame: (event, data) => handleExecutionFrame(event, data, callbacks),
		onError: (failure) => callbacks.onError?.(failure),
	});
}

/** Follow the read-only error analysis stream for one execution. */
export interface ErrorAnalysisCallbacks {
	/** Called once per record, root cause first. */
	onRecord: (record: ErrorRecord) => void;
	onError: (failure: StreamFailure) => void;
}

export function streamErrorAnalysis(
	id: string,
	callbacks: ErrorAnalysisCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openGetStream({
		path: `/api/v1/executions/${id}/error-analysis/stream`,
		signal,
		onFrame: (_event, data) => {
			const record = toErrorRecord(data);
			if (record) callbacks.onRecord(record);
		},
		onError: (failure) => callbacks.onError(failure),
	});
}
