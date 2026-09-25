import type { UsageSnapshot } from '$lib/services/streaming';

/** Milliseconds between buffer flushes into the rendered answer. */
export const STREAM_FLUSH_MS = 80;

export interface LiveToolCall {
	id: string;
	name: string;
	status: 'pending' | 'running' | 'completed' | 'failed';
	result: string;
}

export interface SubAgentNote {
	id: string;
	name: string;
	success: boolean | null;
}

/**
 * One active streamed run: frames only append to pending buffers and a fixed
 * tick merges them into the rendered state, so high-frequency deltas never
 * trigger a parse per frame. Each surface keeps its own instance.
 */
export class StreamRunStore {
	sessionKey = $state<string | null>(null);
	active = $state(false);
	answer = $state('');
	reasoning = $state('');
	tools = $state<LiveToolCall[]>([]);
	subAgents = $state<SubAgentNote[]>([]);
	usage = $state<UsageSnapshot | null>(null);
	iteration = $state<number | null>(null);
	done = $state(false);
	error = $state<string | null>(null);
	/** Set when the server asked for a wait before the next attempt. */
	retryAfterMs = $state<number | null>(null);
	/** The run ended because it was stopped, not because it finished. */
	cancelled = $state(false);

	private pending = '';
	private reasoningPending = '';
	private timer: ReturnType<typeof setInterval> | null = null;
	private controller: AbortController | null = null;

	get signal(): AbortSignal | null {
		return this.controller?.signal ?? null;
	}

	start(sessionKey: string): AbortSignal {
		this.stop();
		this.sessionKey = sessionKey;
		this.active = true;
		this.answer = '';
		this.reasoning = '';
		this.tools = [];
		this.subAgents = [];
		this.usage = null;
		this.iteration = null;
		this.done = false;
		this.error = null;
		this.retryAfterMs = null;
		this.cancelled = false;
		this.pending = '';
		this.reasoningPending = '';
		this.controller = new AbortController();
		this.timer = setInterval(() => this.flush(), STREAM_FLUSH_MS);
		return this.controller.signal;
	}

	appendDelta(text: string): void {
		if (text) this.pending += text;
	}

	appendReasoning(text: string): void {
		if (text) this.reasoningPending += text;
	}

	toolStart(toolCallId: string, toolName: string): void {
		if (!toolCallId) return;
		const existing = this.tools.find((tool) => tool.id === toolCallId);
		if (existing) {
			existing.status = 'running';
			return;
		}
		this.tools = [
			...this.tools,
			{
				id: toolCallId,
				name: toolName || toolCallId,
				status: 'running',
				result: '',
			},
		];
	}

	toolEnd(
		toolCallId: string,
		toolName: string,
		success: boolean,
		result: string,
	): void {
		const existing = this.tools.find((tool) => tool.id === toolCallId);
		const status = success ? 'completed' : 'failed';
		if (existing) {
			existing.status = status;
			existing.result = result;
			this.tools = [...this.tools];
			return;
		}
		this.tools = [
			...this.tools,
			{
				id: toolCallId,
				name: toolName || toolCallId,
				status,
				result,
			},
		];
	}

	noteSubAgent(id: string, name: string, success: boolean | null): void {
		if (!id) return;
		const existing = this.subAgents.find((note) => note.id === id);
		if (existing) {
			existing.success = success;
			this.subAgents = [...this.subAgents];
			return;
		}
		this.subAgents = [...this.subAgents, { id, name, success }];
	}

	complete(): void {
		this.flush();
		this.done = true;
		this.active = false;
		this.clearTimer();
	}

	fail(message: string, retryAfterMs: number | null = null): void {
		this.flush();
		this.error = message;
		this.retryAfterMs = retryAfterMs;
		this.active = false;
		this.clearTimer();
	}

	stop(): void {
		this.controller?.abort();
		this.controller = null;
		this.clearTimer();
		if (this.active) {
			this.flush();
			this.active = false;
			this.cancelled = true;
		}
	}

	private flush(): void {
		if (this.pending) {
			this.answer += this.pending;
			this.pending = '';
		}
		if (this.reasoningPending) {
			this.reasoning += this.reasoningPending;
			this.reasoningPending = '';
		}
	}

	private clearTimer(): void {
		if (this.timer !== null) {
			clearInterval(this.timer);
			this.timer = null;
		}
	}
}

export const chatStream = new StreamRunStore();
