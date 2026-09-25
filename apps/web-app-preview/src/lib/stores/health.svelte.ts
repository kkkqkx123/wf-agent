import { getHealth } from '$lib/services/events';

export type HealthStatus = 'checking' | 'reachable' | 'unreachable';

class HealthStore {
	status = $state<HealthStatus>('checking');
	private inflight = false;

	/** Poll the liveness probe; overlapping calls collapse into the first one. */
	async refresh(): Promise<void> {
		if (this.inflight) return;
		this.inflight = true;
		try {
			this.status = (await getHealth()) ? 'reachable' : 'unreachable';
		} finally {
			this.inflight = false;
		}
	}
}

/** Backend reachability, polled on an interval by the app shell. */
export const health = new HealthStore();
