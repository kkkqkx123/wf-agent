import type { StreamFailure } from '$lib/api/stream';
import { parseDisplayPayload } from '$lib/utils/tool-output';
import type { DisplayPayload } from '$lib/utils/tool-output';

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
