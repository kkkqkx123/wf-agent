import { openGetStream, openPostStream } from '$lib/api/stream';
import type { StreamFailure } from '$lib/api/stream';
import type { RunLoopMessage } from '$lib/services/agent-loops';
import {
	handleExecutionFrame,
	handleGenerationFrame,
	toErrorRecord,
} from '$lib/utils/stream-parser';
import type { ErrorRecord, StreamCallbacks } from '$lib/utils/stream-parser';

export type {
	ToolLifecycle,
	UsageSnapshot,
	StreamNodeUpdate,
	ErrorRecord,
	StreamCallbacks,
} from '$lib/utils/stream-parser';

/** The wire `Message` the LLM endpoints require, identity and time included. */
function createUserMessage(text: string): RunLoopMessage {
	return {
		id: crypto.randomUUID(),
		role: 'user',
		content: text,
		timestamp: Date.now(),
	};
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
