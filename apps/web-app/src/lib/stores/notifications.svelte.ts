import { openWebSocket } from '#lib/api/ws';
import type { StreamState } from '#lib/api/sse';
import type { EventRecord } from '#lib/types/models';

export interface NotificationItem {
	id: string;
	type: string;
	at: string;
	executionId: string | null;
	summary: string;
}

const RECENT_CAP = 100;

function summarize(event: EventRecord): string {
	try {
		const payload = event.payload ? (JSON.parse(event.payload) as Record<string, unknown>) : {};
		const message =
			(typeof payload.message === 'string' && payload.message) ||
			(typeof payload.error === 'string' && payload.error) ||
			(typeof payload.tool_name === 'string' && `Tool ${payload.tool_name}`) ||
			'';
		return message.slice(0, 200);
	} catch {
		return '';
	}
}

function toItem(event: EventRecord, index: number): NotificationItem {
	return {
		id: event.id || `${event.type}-${event.at}-${index}`,
		type: event.type,
		at: event.at,
		executionId: event.executionId,
		summary: summarize(event),
	};
}

/**
 * WS `notifications` feed: approvals, follow-ups, failures and recovery
 * hints arrive as metadata-only pushes; details stay on REST. Counts are
 * derived locally from the received window plus REST reconciliation, the
 * backend keeps no persistent unread inbox.
 */
class NotificationsStore {
	items = $state<NotificationItem[]>([]);
	version = $state(0);
	liveState = $state<StreamState>('closed');

	private listeners = new Set<(version: number) => void>();
	private closeSocket: (() => void) | null = null;
	private seen = 0;

	subscribe(listener: (version: number) => void): () => void {
		this.listeners.add(listener);
		if (!this.closeSocket) {
			this.closeSocket = openWebSocket({
				subscriptions: [{ notifications: true }],
				onEvent: (event) => this.handle(event),
				onState: (state) => {
					this.liveState = state;
					if (state === 'closed') this.closeSocket = null;
				},
			});
		}
		return () => {
			this.listeners.delete(listener);
			if (this.listeners.size === 0 && this.closeSocket) {
				const close = this.closeSocket;
				this.closeSocket = null;
				close();
			}
		};
	}

	/** Local actions bump immediately without waiting for the push. */
	bump(): void {
		this.version += 1;
		for (const notify of this.listeners) notify(this.version);
	}

	private handle(event: EventRecord): void {
		this.seen += 1;
		this.items = [toItem(event, this.seen), ...this.items].slice(0, RECENT_CAP);
		this.version += 1;
		for (const notify of this.listeners) notify(this.version);
	}
}

export const notifications = new NotificationsStore();
