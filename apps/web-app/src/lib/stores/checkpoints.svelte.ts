import { openRealtime } from '#lib/api/realtime';
import type { StreamState } from '#lib/api/sse';
import type { EventRecord } from '#lib/types/models';

export type CheckpointTab = 'chain' | 'files' | 'approvals';

export type CheckpointDomain = 'chain' | 'files' | 'approvals';

type CheckpointListener = (domain: CheckpointDomain) => void;

/** Old servers without review events wrap submits in file changes. */
function isReviewFilePayload(payload: string): boolean {
	if (!payload) return false;
	const needle = payload.toLowerCase();
	return needle.includes('review') || needle.includes('submit') || needle.includes('approv');
}

/**
 * Map a wire event type to the refresh domain it invalidates.
 *
 * Dedicated approval events (`REVIEW_SUBMITTED`, `APPROVAL_RESOLVED`,
 * `TOOL_APPROVAL_*`, `FOLLOWUP_*`) map to approvals only. File changes map
 * to files only; the old files-fan-out-to-approvals fallback stays for
 * servers that still emit no review event on submit.
 */
function domainForEvent(type: string): CheckpointDomain | 'all' {
	switch (type) {
		case 'CHECKPOINT_CREATED':
		case 'CHECKPOINT_RESTORED':
		case 'CHECKPOINT_DELETED':
		case 'CHECKPOINT_FAILED':
		case 'CHECKPOINT_GC_COMPLETED':
		case 'CHECKPOINT_RESTORE_STARTED':
		case 'CHECKPOINT_RESTORE_COMPLETED':
			return 'chain';
		case 'CHECKPOINT_FILE_CHANGED':
		case 'CHECKPOINT_MERGE_CONFLICTED':
			return 'files';
		case 'CHECKPOINT_REVIEW_SUBMITTED':
		case 'CHECKPOINT_APPROVAL_RESOLVED':
		case 'TOOL_APPROVAL_REQUESTED':
		case 'TOOL_APPROVAL_RESPONDED':
		case 'TOOL_APPROVAL_FAILED':
		case 'FOLLOWUP_QUESTION_REQUESTED':
		case 'FOLLOWUP_QUESTION_RESPONDED':
		case 'FOLLOWUP_QUESTION_FAILED':
			return 'approvals';
		default:
			return 'all';
	}
}

/**
 * Checkpoint page state: per-tab selection (survives tab switches),
 * single in-flight destructive action, and a checkpoint-only live
 * subscription that bumps per-domain counters so each tab refreshes
 * only its own segment.
 *
 * File-workspace inputs (actor, workspace files, index state) live in
 * `fileWorkspace.svelte.ts`; approvals keep no shared input beyond
 * their domain counter and page-side list state.
 */
class CheckpointStore {
	// ── context selection (one slot per tab) ──
	selectedCheckpointId = $state<string | null>(null);
	selectedChangeId = $state<string | null>(null);
	selectedApprovalId = $state<string | null>(null);

	// ── in-flight action ──
	action = $state<string | null>(null);

	// ── live subscription ──
	liveState = $state<StreamState>('closed');
	chainVersion = $state(0);
	filesVersion = $state(0);
	approvalsVersion = $state(0);

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
	}

	endAction(): void {
		this.action = null;
	}

	get busy(): boolean {
		return this.action !== null;
	}

	/**
	 * Bump one domain counter and notify live listeners. Local mutations
	 * bump each domain they invalidate; server events map through
	 * `domainForEvent` first.
	 */
	bump(domain: CheckpointDomain): void {
		if (domain === 'chain') this.chainVersion += 1;
		else if (domain === 'files') this.filesVersion += 1;
		else this.approvalsVersion += 1;
		for (const notify of this.listeners) notify(domain);
	}

	private bumpAll(): void {
		this.chainVersion += 1;
		this.filesVersion += 1;
		this.approvalsVersion += 1;
		for (const notify of this.listeners) notify('files');
		for (const notify of this.listeners) notify('chain');
		for (const notify of this.listeners) notify('approvals');
	}

	private handleEvent(event: EventRecord): void {
		const domain = domainForEvent(event.type);
		if (domain === 'all') {
			this.bumpAll();
			return;
		}
		if (domain === 'files' && isReviewFilePayload(event.payload)) {
			// Compat fallback: old servers wrap a review submit in a plain
			// file change with no dedicated approval event.
			this.filesVersion += 1;
			this.approvalsVersion += 1;
			for (const notify of this.listeners) notify('files');
			for (const notify of this.listeners) notify('approvals');
			return;
		}
		this.bump(domain);
	}

	/**
	 * Subscribe to checkpoint-domain changes. The SSE connection opens
	 * with the first subscriber and closes with the last; every event
	 * bumps only its domain counter so subscribers refresh their own
	 * scope.
	 */
	subscribeLive(listener: CheckpointListener): () => void {
		this.listeners.add(listener);
		if (!this.closeStream) {
			this.closeStream = openRealtime({
				dimensions: [{ checkpoint: true }],
				onEvent: (event) => {
					this.handleEvent(event);
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
