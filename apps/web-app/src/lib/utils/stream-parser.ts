import type { StreamFailure } from '#lib/api/stream';
import { parseDisplayPayload } from '#lib/utils/tool-output';
import type { DisplayPayload } from '#lib/utils/tool-output';

export interface ToolLifecycle {
	toolCallId: string;
	toolName: string;
	toolKind?: string;
	success: boolean;
	result: string;
	error: string | null;
	/** Frontend-facing summary produced alongside the full result. Null on
	 * older servers; consumers fall back to name-based inference. */
	display: DisplayPayload | null;
	/** Structured background linkage for tools that hand a running session
	 * back instead of blocking. Null for foreground tools and older
	 * servers; views must not sniff the result text. */
	background: BackgroundMarker | null;
}

/** Linkage between one tool call and the background session it produced. */
export interface BackgroundMarker {
	sessionId: string;
	backgrounded: boolean;
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

/** Linkage details of a sub-agent frame. All fields are absent on
 * older servers; views fall back to name-only display. */
export interface SubAgentDetails {
	toolCallId?: string;
	preview?: string | null;
	background?: boolean | null;
	activity?: string | null;
	outputTokens?: number | null;
	toolUses?: number | null;
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
	onToolStart?: (
		toolCallId: string,
		toolName: string,
		toolKind?: string,
	) => void;
	onToolEnd?: (tool: ToolLifecycle) => void;
	onUsage?: (usage: UsageSnapshot) => void;
	onSubAgent?: (
		id: string,
		name: string,
		success: boolean | null,
		details?: SubAgentDetails,
	) => void;
	onNode?: (node: StreamNodeUpdate) => void;
	onCompleted?: (iterations: number) => void;
	onFailed?: (message: string) => void;
	onInterrupted?: (reason: string) => void;
	onError?: (failure: StreamFailure) => void;
}

/** Bus event frame shared by the engine and system streams. */
export interface BaseEventFrame {
	id: string;
	type: string;
	timestamp: number;
	event_name?: string | null;
	workflow_id?: string | null;
	execution_id?: string | null;
	agent_loop_id?: string | null;
	metadata?: Record<string, unknown> | null;
}

/** Execution-stream preface: the single-field view the workflow execute
 * stream emits first (`event: metadata`). */
export interface StreamMetadata {
	executionId: string;
}

/** Decoded execution-stream frame. Unknown payloads fall back to `raw`,
 * which preserves the event line and original data for forward
 * compatibility. Decoding never throws; transport failures travel through
 * `onError` instead. */
export type ExecutionFrame =
	| { kind: 'metadata'; executionId: string }
	| { kind: 'engine'; base: BaseEventFrame }
	| { kind: 'protocol'; eventType: string; data: Record<string, unknown> }
	| { kind: 'raw'; event: string; data: unknown };

/** Decoded system-event frame. The `connected` envelope stands alone; every
 * other frame decodes as a bare bus event, with `raw` preserving unknown
 * payloads. */
export type SystemFrame =
	| { kind: 'connected'; executionId: string | null }
	| { kind: 'base'; base: BaseEventFrame }
	| { kind: 'raw'; data: unknown };

/** Decoded error-chain frame. Whole error records decode to `record`;
 * anything without an id falls back to `raw`. */
export type ErrorChainFrame =
	{ kind: 'record'; record: ErrorRecord } | { kind: 'raw'; data: unknown };

// Future payload families keep the same branch-then-fallback shape when a
// caller appears. The LLM family branches on its `event_type` tag
// (MessageStreamEvent, 17 variants); no frontend surface consumes
// `POST /llm/generate-stream`, so it stays a documented extension point.

/** Protocol event kinds modelled by a run timeline. */
const KNOWN_PROTOCOL_TYPES = new Set([
	'llm_delta',
	'reasoning_delta',
	'iteration_start',
	'iteration_end',
	'tool_start',
	'tool_end',
	'usage',
	'sub_agent_started',
	'sub_agent_ended',
	'sub_agent_progress',
	'completed',
	'failed',
	'interrupted',
]);

function isRecord(value: unknown): value is Record<string, unknown> {
	return !!value && typeof value === 'object';
}

interface Frame {
	[key: string]: unknown;
}

function asRecord(data: unknown): Frame | null {
	if (!data || typeof data !== 'object') return null;
	return data as Frame;
}

function isBaseEventLenient(value: unknown): value is Record<string, unknown> {
	if (!isRecord(value)) return false;
	if (typeof value.type !== 'string' || value.type === '') return false;
	if (value.type === 'connected') return false;
	return typeof value.timestamp === 'number' || typeof value.id === 'string';
}

function connectedExecutionId(payload: Record<string, unknown>): string | null {
	const inner = isRecord(payload.data) ? payload.data : null;
	const fromInner =
		(inner && typeof inner.executionId === 'string' && inner.executionId) ||
		(inner && typeof inner.execution_id === 'string' && inner.execution_id) ||
		null;
	if (fromInner) return fromInner;
	const outer =
		(typeof payload.executionId === 'string' && payload.executionId) ||
		(typeof payload.execution_id === 'string' && payload.execution_id) ||
		null;
	return outer;
}

/** Decode one execution-stream frame by its `event:` line. */
export function decodeExecutionFrame(
	event: string,
	data: unknown,
): ExecutionFrame {
	if (event === 'metadata') {
		if (
			isRecord(data) &&
			typeof data.execution_id === 'string' &&
			data.execution_id
		) {
			return { kind: 'metadata', executionId: data.execution_id };
		}
		if (
			isRecord(data) &&
			typeof data.executionId === 'string' &&
			data.executionId
		) {
			return { kind: 'metadata', executionId: data.executionId };
		}
		return { kind: 'raw', event, data };
	}
	if (event === 'engine') {
		// Engine lines carry bare bus events. The run timeline only models
		// node lifecycle, but decoding accepts any typed bus frame here so a
		// minimal fixture (type plus metadata, no id/timestamp) keeps working;
		// untyped payloads fall back to raw.
		if (isRecord(data) && typeof data.type === 'string' && data.type !== '') {
			return { kind: 'engine', base: normalizeBaseEvent(data) };
		}
		return { kind: 'raw', event, data };
	}
	if (event === '') {
		if (
			isRecord(data) &&
			typeof data.type === 'string' &&
			KNOWN_PROTOCOL_TYPES.has(data.type)
		) {
			return { kind: 'protocol', eventType: data.type, data };
		}
		return { kind: 'raw', event, data };
	}
	return { kind: 'raw', event, data };
}

function normalizeBaseEvent(record: Record<string, unknown>): BaseEventFrame {
	return {
		id: typeof record.id === 'string' ? record.id : '',
		type: record.type as string,
		timestamp: typeof record.timestamp === 'number' ? record.timestamp : 0,
		event_name:
			typeof record.event_name === 'string' ? record.event_name : null,
		workflow_id:
			typeof record.workflow_id === 'string' ? record.workflow_id : null,
		execution_id:
			typeof record.execution_id === 'string' ? record.execution_id : null,
		agent_loop_id:
			typeof record.agent_loop_id === 'string' ? record.agent_loop_id : null,
		metadata: isRecord(record.metadata)
			? (record.metadata as Record<string, unknown>)
			: null,
	};
}

/** Decode one system-event frame. The hello envelope is detected by its
 * `type: connected` marker, since the server sends it as a bare data frame.
 * The preview mock emits the same marker with a minimal shape, so the base
 * check stays lenient (type plus timestamp or id). */
export function decodeSystemFrame(data: unknown): SystemFrame {
	if (isRecord(data) && data.type === 'connected') {
		return { kind: 'connected', executionId: connectedExecutionId(data) };
	}
	if (isBaseEventLenient(data)) {
		return { kind: 'base', base: normalizeBaseEvent(data) };
	}
	return { kind: 'raw', data };
}

/** Decode one error-chain frame. */
export function decodeErrorChainFrame(data: unknown): ErrorChainFrame {
	const record = toErrorRecord(data);
	if (record) return { kind: 'record', record };
	return { kind: 'raw', data };
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

/** Background linkage attached to tool end frames by newer servers. Absent
 * or malformed values read as null so older servers keep working. */
function parseBackgroundMarker(value: unknown): BackgroundMarker | null {
	const record = asRecord(value);
	const sessionId = asString(record?.session_id);
	if (!record || !sessionId) return null;
	return { sessionId, backgrounded: record.backgrounded !== false };
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
export function toErrorRecord(data: unknown): ErrorRecord | null {
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

/**
 * Route one frame of the execution protocol, the stream behind agent loop
 * runs, chat turns and workflow executions. Kinds the timeline does not
 * model are ignored, so newer server events never break a run.
 */
export function handleExecutionFrame(
	event: string,
	data: unknown,
	callbacks: StreamCallbacks,
): void {
	const decoded = decodeExecutionFrame(event, data);
	if (decoded.kind === 'metadata') {
		callbacks.onExecution?.(decoded.executionId);
		return;
	}
	if (decoded.kind === 'engine') {
		// Engine bus events keep their bus event type as the frame kind and
		// carry the node payload under `metadata`. Explicitly decoded above,
		// so a malformed engine line no longer falls into the protocol switch.
		const record = decoded.base as unknown as Frame;
		const status = NODE_STATUS[asString(record.type)];
		if (status) callbacks.onNode?.(toNodeUpdate(record, status));
		return;
	}
	if (decoded.kind !== 'protocol') return;
	const frame = decoded.data as Frame;
	switch (decoded.eventType) {
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
				asString(frame.tool_kind) || asString(frame.kind) || undefined,
			);
			break;
		case 'tool_end':
			callbacks.onToolEnd?.({
				toolCallId: asString(frame.tool_call_id),
				toolName: asString(frame.tool_name),
				toolKind:
					asString(frame.tool_kind) || asString(frame.kind) || undefined,
				success: frame.success !== false,
				result: asString(frame.result),
				error: asOptionalString(frame.error),
				display: parseDisplayPayload(frame.display),
				background: parseBackgroundMarker(frame.background),
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
			callbacks.onSubAgent?.(asString(frame.id), asString(frame.name), null, {
				toolCallId: asString(frame.tool_call_id) || undefined,
				preview: asOptionalString(frame.preview),
				background: frame.background === true,
			});
			break;
		case 'sub_agent_ended':
			callbacks.onSubAgent?.(
				asString(frame.id),
				asString(frame.name),
				frame.success !== false,
				{
					toolCallId: asString(frame.tool_call_id) || undefined,
					background: frame.background === true,
				},
			);
			break;
		case 'sub_agent_progress':
			callbacks.onSubAgent?.(asString(frame.id), '', null, {
				toolCallId: asString(frame.tool_call_id) || undefined,
				activity: asOptionalString(frame.activity),
				outputTokens: asOptionalNumber(frame.output_tokens),
				toolUses: asOptionalNumber(frame.tool_uses),
			});
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
