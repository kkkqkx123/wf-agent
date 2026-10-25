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

export interface LoopStreamBody {
	model: string;
	message: string;
	conversation?: RunConversationMessage[];
	/** Optional agent template id; empty falls back to the default main agent. */
	agentId?: string;
	/** Optional per-run filesystem scope; absent inherits the global default. */
	fileScope?: ExecutionFileScope | null;
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
	const message = assertAgentMessage(body.message);
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
