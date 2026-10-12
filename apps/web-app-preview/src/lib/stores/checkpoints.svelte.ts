import { openEventStream, type StreamState } from '$lib/api/sse';

export type CheckpointTab = 'chain' | 'files' | 'approvals';

type CheckpointListener = () => void;

/**
 * Checkpoint page state: per-tab selection (survives tab switches),
 * in-flight destructive actions with errors, and a checkpoint-only live
 * subscription that bumps `version` so the page refreshes its current
 * tab instead of reloading everything.
 */
class CheckpointStore {
	// ── context selection (one slot per tab) ──
	selectedCheckpointId = $state<string | null>(null);
	selectedChangeId = $state<string | null>(null);
	selectedApprovalId = $state<string | null>(null);

	// ── in-flight action + error ──
	action = $state<string | null>(null);
	actionError = $state<string | null>(null);

	// ── live subscription ──
	liveState = $state<StreamState>('closed');
	version = $state(0);

	private listeners = new Set<CheckpointListener>();
	private closeStream: (() => void) | null = null;

	selectionFor(tab: CheckpointTab): string | null {
		return tab === 'chain'
			? this.selectedCheckpointId
			: tab === 'files'
				? this.selectedChangeId
				: this.selectedApprovalId;
	}

	select(tab: CheckpointTab, id: string | null): void {
		if (tab === 'chain') this.selectedCheckpointId = id;
		else if (tab === 'files') this.selectedChangeId = id;
		else this.selectedApprovalId = id;
	}

	beginAction(key: string): void {
		this.action = key;
		this.actionError = null;
	}

	endAction(error?: string): void {
		this.action = null;
		this.actionError = error ?? null;
	}

	get busy(): boolean {
		return this.action !== null;
	}

	/**
	 * Subscribe to checkpoint-domain changes. The SSE connection opens
	 * with the first subscriber and closes with the last; every
	 * checkpoint event bumps `version` and notifies listeners so the
	 * page can refresh its current scope.
	 */
	subscribeLive(listener: CheckpointListener): () => void {
		this.listeners.add(listener);
		if (!this.closeStream) {
			this.closeStream = openEventStream({
				checkpointOnly: true,
				onEvent: () => {
					this.version += 1;
					for (const notify of this.listeners) notify();
				},
				onState: (state) => {
					this.liveState = state;
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
}

export const checkpoints = new CheckpointStore();
