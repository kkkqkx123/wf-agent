import { openEventStream, type StreamState } from '$lib/api/sse';
import type { EventRecord } from '$lib/types/models';

type LiveListener = (event: EventRecord) => void;

const RECENT_CAP = 200;

/**
 * App-wide live event feed backed by one shared SSE connection. Consumers
 * subscribe with a listener and filter client-side (by executionId etc.);
 * the connection opens with the first subscriber and closes when the last
 * one leaves.
 */
class LiveStore {
	state = $state<StreamState>('closed');
	recent = $state<EventRecord[]>([]);

	private listeners = new Set<LiveListener>();
	private closeStream: (() => void) | null = null;

	subscribe(listener: LiveListener): () => void {
		this.listeners.add(listener);
		if (!this.closeStream) {
			this.closeStream = openEventStream({
				onEvent: (event) => this.dispatch(event),
				onState: (state) => {
					this.state = state;
					if (state === 'closed') this.closeStream = null;
				},
			});
		}
		return () => {
			this.listeners.delete(listener);
			if (this.listeners.size === 0 && this.closeStream) {
				const close = this.closeStream;
				this.closeStream = null;
				close();
			}
		};
	}

	private dispatch(event: EventRecord): void {
		this.recent = [event, ...this.recent].slice(0, RECENT_CAP);
		for (const listener of this.listeners) listener(event);
	}
}

export const live = new LiveStore();
