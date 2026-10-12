import { getFingerprint } from '$lib/services/git';

export type GitPanel = 'status' | 'files' | 'branches' | 'commits' | 'stash';

type GitListener = () => void;

class GitStore {
	selectedFile = $state<string | null>(null);
	selectedCommit = $state<string | null>(null);
	selectedBranch = $state<string | null>(null);
	selectedStash = $state<string | null>(null);

	action = $state<string | null>(null);
	actionError = $state<string | null>(null);

	version = $state(0);
	liveState = $state<'open' | 'closed'>('closed');

	private listeners = new Set<GitListener>();
	private timer: ReturnType<typeof setInterval> | null = null;
	private lastFingerprint = '';

	select(panel: GitPanel, id: string | null): void {
		if (panel === 'files') this.selectedFile = id;
		else if (panel === 'commits') this.selectedCommit = id;
		else if (panel === 'branches') this.selectedBranch = id;
		else if (panel === 'stash') this.selectedStash = id;
	}

	selectionFor(panel: GitPanel): string | null {
		if (panel === 'files') return this.selectedFile;
		if (panel === 'commits') return this.selectedCommit;
		if (panel === 'branches') return this.selectedBranch;
		if (panel === 'stash') return this.selectedStash;
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
			this.timer = setInterval(() => {
				void getFingerprint()
					.then((fp) => {
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
					})
					.catch(() => {});
			}, 5000);
		}
		return () => {
			this.listeners.delete(listener);
			if (this.listeners.size === 0 && this.timer) {
				clearInterval(this.timer);
				this.timer = null;
				this.liveState = 'closed';
			}
		};
	}
}

export const git = new GitStore();
