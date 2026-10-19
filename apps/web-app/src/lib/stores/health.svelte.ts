import { EXPECTED_API_VERSION, checkApiVersion, getHealth } from '$lib/services/events';

export type HealthStatus = 'checking' | 'reachable' | 'unreachable';

class HealthStore {
	status = $state<HealthStatus>('checking');
	apiVersion = $state<string | null>(null);
	versionMismatch = $state(false);
	private inflight = false;

	/** Poll liveness plus protocol version; overlapping calls collapse into the first one. */
	async refresh(): Promise<void> {
		if (this.inflight) return;
		this.inflight = true;
		try {
			this.status = (await getHealth()) ? 'reachable' : 'unreachable';
			const compat = await checkApiVersion();
			if (compat === null) {
				this.apiVersion = null;
				this.versionMismatch = false;
			} else {
				this.apiVersion = compat ? EXPECTED_API_VERSION : 'mismatch';
				this.versionMismatch = !compat;
				if (!compat) {
					console.warn(
						`Backend API version mismatch: expected ${EXPECTED_API_VERSION}`
					);
				}
			}
		} finally {
			this.inflight = false;
		}
	}
}

/** Backend reachability, polled on an interval by the app shell. */
export const health = new HealthStore();
