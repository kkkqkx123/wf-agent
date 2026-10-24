import { getFingerprint } from '#lib/services/git';
import { live } from '#lib/stores/live.svelte';

export type GitPanel =
	| 'status'
	| 'files'
	| 'conflicts'
	| 'branches'
	| 'remotes'
	| 'tags'
	| 'commits'
	| 'reflog'
	| 'stash'
	| 'worktrees'
	| 'submodules';

type GitListener = () => void;

const STREAM_POLL_MS = 120_000;
const OFFLINE_POLL_MS = 15_000;

class GitStore {
	selectedFile = $state<string | null>(null);
	selectedCommit = $state<string | null>(null);
	selectedBranch = $state<string | null>(null);
	selectedStash = $state<string | null>(null);
	selectedWorktree = $state<string | null>(null);
	selectedSubmodule = $state<string | null>(null);

	action = $state<string | null>(null);
	actionError = $state<string | null>(null);

	version = $state(0);
	liveState = $state<'open' | 'closed'>('closed');

	private listeners = new Set<GitListener>();
	private timer: ReturnType<typeof setTimeout> | null = null;
	private unsubscribeLive: (() => void) | null = null;
	private lastFingerprint = '';
	private visibilityHandler: (() => void) | null = null;

	/**
	 * Push seam for repository transitions: the server publishes a
	 * `GIT_CHANGED` bus event on every write-entry mutation, and the shared
	 * live feed delivers it here. The fingerprint stays the cheap change
	 * guard; `notifyExternalChange` remains the manual entry for sources
	 * the backend cannot see (a future file watcher will call it too).
	 */
	notifyExternalChange(): void {
		void this.pollOnce();
	}

	private async pollOnce(): Promise<void> {
		try {
			const fp = await getFingerprint();
			const key = `${fp.statusHash}:${fp.head}:${fp.branchSummary}`;
			if (!this.lastFingerprint) {
				this.lastFingerprint = key;
				return;
			}
			if (key !== this.lastFingerprint) {
				this.lastFingerprint = key;
				this.version += 1;
				for (const notify of this.listeners) notify();
			}
		} catch {
			// Fingerprint failures keep the last known version.
		}
	}

	private pollIntervalMs(): number {
		// Pushes drive refresh while the shared feed is open; the timer is
		// only a safety net for missed or external changes.
		if (live.state === 'open') return STREAM_POLL_MS;
		return OFFLINE_POLL_MS;
	}

	private schedulePoll(): void {
		if (this.timer !== null) clearTimeout(this.timer);
		this.timer = setTimeout(() => {
			if (this.listeners.size === 0) return;
			void this.pollOnce().finally(() => {
				if (this.listeners.size > 0) this.schedulePoll();
			});
		}, this.pollIntervalMs());
	}

	select(panel: GitPanel, id: string | null): void {
		if (panel === 'files') this.selectedFile = id;
		else if (panel === 'commits') this.selectedCommit = id;
		else if (panel === 'branches') this.selectedBranch = id;
		else if (panel === 'stash') this.selectedStash = id;
		else if (panel === 'worktrees') this.selectedWorktree = id;
		else if (panel === 'submodules') this.selectedSubmodule = id;
	}

	selectionFor(panel: GitPanel): string | null {
		if (panel === 'files') return this.selectedFile;
		if (panel === 'commits') return this.selectedCommit;
		if (panel === 'branches') return this.selectedBranch;
		if (panel === 'stash') return this.selectedStash;
		if (panel === 'worktrees') return this.selectedWorktree;
		if (panel === 'submodules') return this.selectedSubmodule;
		return null;
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

	subscribeLive(listener: GitListener): () => void {
		this.listeners.add(listener);
		if (!this.timer) {
			this.liveState = 'open';
			void this.pollOnce();
			this.unsubscribeLive = live.subscribe((event) => {
				if (event.type !== 'GIT_CHANGED') return;
				if (this.listeners.size === 0) return;
				this.notifyExternalChange();
			});
			this.schedulePoll();
			if (typeof document !== 'undefined' && !this.visibilityHandler) {
				this.visibilityHandler = () => {
					if (this.listeners.size === 0) return;
					this.schedulePoll();
					if (!document.hidden) void this.pollOnce();
				};
				document.addEventListener('visibilitychange', this.visibilityHandler);
			}
		}
		return () => {
			this.listeners.delete(listener);
			if (this.listeners.size === 0) {
				if (this.timer !== null) {
					clearTimeout(this.timer);
					this.timer = null;
				}
				if (this.unsubscribeLive !== null) {
					this.unsubscribeLive();
					this.unsubscribeLive = null;
				}
				if (typeof document !== 'undefined' && this.visibilityHandler) {
					document.removeEventListener(
						'visibilitychange',
						this.visibilityHandler,
					);
					this.visibilityHandler = null;
				}
				this.liveState = 'closed';
			}
		};
	}
}

export const git = new GitStore();
