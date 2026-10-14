import type { IndexStatus, WorkspaceFile } from '$lib/services/checkpoints';

const ACTOR_STORAGE_KEY = 'wf:fileWorkspace:actor';

function loadActor(): string {
	try {
		if (typeof localStorage === 'undefined') return '';
		return localStorage.getItem(ACTOR_STORAGE_KEY) ?? '';
	} catch {
		return '';
	}
}

/**
 * File-workspace state shared by the checkpoints files tab: the acting
 * executor input (persisted across reloads), the inspected workspace
 * file list, the source-index health, and workspace-scoped in-flight
 * actions. Chain and approval state stay in `checkpoints.svelte.ts`.
 */
class FileWorkspaceStore {
	actor = $state(loadActor());
	workspaceFiles = $state<WorkspaceFile[]>([]);
	indexStatus = $state<IndexStatus | null>(null);

	action = $state<string | null>(null);
	actionError = $state<string | null>(null);

	setActor(value: string): void {
		this.actor = value;
		try {
			if (typeof localStorage !== 'undefined') {
				localStorage.setItem(ACTOR_STORAGE_KEY, value);
			}
		} catch {
			// Persistence is best-effort; the in-memory value still wins.
		}
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
}

export const fileWorkspace = new FileWorkspaceStore();
