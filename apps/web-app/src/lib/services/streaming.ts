import { openGetStream, openPostStream } from '#lib/api/stream';
import type { StreamFailure } from '#lib/api/stream';
import type { RunConversationMessage } from '#lib/types/models';
import { assertAgentMessage, assertAgentModel, assertFileScope } from '#lib/services/run';
import type { ExecutionFileScope } from '#lib/types/models';
import { decodeErrorChainFrame, handleExecutionFrame } from '#lib/utils/stream-parser';
import type { ErrorRecord, StreamCallbacks } from '#lib/utils/stream-parser';

export type {
	ToolLifecycle,
	UsageSnapshot,
	StreamNodeUpdate,
	ErrorRecord,
	StreamCallbacks,
	BaseEventFrame,
	ExecutionFrame,
	SystemFrame,
	ErrorChainFrame,
	StreamMetadata,
} from '#lib/utils/stream-parser';

export interface ImagePayload {
	/** Inline base64 payload. Empty when `url` carries the image. */
	data?: string;
	/** IANA media type of `data`. Required for inline payloads. */
	media_type?: string;
	/** Remote address or `data:` URI. Takes precedence over `data`. */
	url?: string;
	/** Render detail hint forwarded to the provider codec. */
	detail?: 'auto' | 'low' | 'high' | 'original';
}

export interface AttachmentPayload {
	name: string;
	content: string;
	truncated?: boolean;
	original_chars?: number;
}

export type TemporaryRunMode = 'restricted' | 'ephemeral';

export interface LoopStreamBody {
	model: string;
	message: string;
	conversation?: RunConversationMessage[];
	/** Optional agent template id; empty falls back to the default main agent. */
	agentId?: string;
	/**
	 * Fenced scope of this turn: `restricted` fences the turn onto the
	 * read-only/network allowlist, `ephemeral` runs a short read-only Q&A
	 * with no checkpoints at all. Absent means a full agent run; the
	 * server enforces the exposure.
	 */
	temporaryMode?: TemporaryRunMode;
	/** Optional per-run filesystem scope; absent inherits the global default. */
	fileScope?: ExecutionFileScope | null;
	/** Structured image payload of the turn. */
	images?: ImagePayload[];
	/** Structured text attachments of the turn. */
	attachments?: AttachmentPayload[];
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
	const message = assertAgentMessage(body.message, body.images, body.attachments);
	const model = assertAgentModel(body.model);
	const fileScope = assertFileScope(body.fileScope ?? null);
	return openPostStream({
		path: `/api/v1/agent-loops/${id}/stream`,
		body: {
			model,
			message,
			agent_id: body.agentId ?? '',
			tool_call_protocol: { format: 'json' },
			conversation: body.conversation ?? [],
			file_scope: fileScope,
			...(body.temporaryMode ? { temporary_mode: body.temporaryMode } : {}),
			...(body.images && body.images.length > 0 ? { images: body.images } : {}),
			...(body.attachments && body.attachments.length > 0 ? { attachments: body.attachments } : {}),
		},
		signal,
		onFrame: (event, data) => handleExecutionFrame(event, data, callbacks),
		onError: (failure) => callbacks.onError?.(failure),
	});
}

/** Body of one chat turn. */
export interface ChatTurnBody {
	/** LLM profile id that serves the turn. */
	profileId: string;
	/** The user text of this turn. */
	message: string;
	/** Session anchor of the conversation; absent starts a new session. */
	sessionId?: string;
	/** Structured image payload of the turn. */
	images?: ImagePayload[];
	/** Structured text attachments of the turn. */
	attachments?: AttachmentPayload[];
}

/**
 * Stream one chat turn. The turn belongs to a session: the session id
 * anchors the conversation, so a session continues where the previous turn
 * left off.
 */
export function streamChat(
	body: ChatTurnBody,
	callbacks: StreamCallbacks,
	signal: AbortSignal,
): Promise<void> {
	return openPostStream({
		path: '/api/v1/chat/stream',
		body: {
			profile_id: body.profileId,
			message: body.message,
			...(body.sessionId ? { session_id: body.sessionId } : {}),
			...(body.images && body.images.length > 0 ? { images: body.images } : {}),
			...(body.attachments && body.attachments.length > 0 ? { attachments: body.attachments } : {}),
		},
		signal,
		onFrame: (event, data) => handleExecutionFrame(event, data, callbacks),
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
			const decoded = decodeErrorChainFrame(data);
			if (decoded.kind === 'record') callbacks.onRecord(decoded.record);
		},
		onError: (failure) => callbacks.onError(failure),
	});
}
