import { SvelteMap } from 'svelte/reactivity';
import { createPty, closePty, resizePty, getPty } from '$lib/services/pty';
import type { PtySession } from '$lib/services/pty';

/**
 * Local view of a server PTY session. Buffer/scrollY/cursor/title live only in
 * the browser (serialized terminal output for reconnect replay); the server
 * keeps the session itself plus its viewport size.
 */
export interface LocalPtySession {
	id: string;
	title: string;
	rows?: number;
	cols?: number;
	buffer?: string;
	scrollY?: number;
	cursor?: number;
	status?: 'running' | 'exited';
	exitCode?: number;
}

const STORAGE_PREFIX = 'wf.pty.v1';

function storageKey(repo: string): string {
	return `${STORAGE_PREFIX}.${repo}`;
}

/**
 * Per-repository PTY session manager. Owns the session list, local buffer
 * persistence, and the active-tab selection. Server session lifecycle is
 * delegated to the pty service layer.
 */
export class PtySessionStore {
	sessions = $state<LocalPtySession[]>([]);
	activeId = $state<string | null>(null);
	repo = $state<string>('');

	private persistTimer: ReturnType<typeof setTimeout> | undefined;

	/** Restore persisted sessions for a repo and prune stale records. */
	open(repo: string): void {
		this.repo = repo;
		this.sessions = [];
		this.activeId = null;
		try {
			const raw = localStorage.getItem(storageKey(repo));
			if (raw) {
				const parsed = JSON.parse(raw) as LocalPtySession[];
				if (Array.isArray(parsed)) {
					this.sessions = parsed.filter((s) => typeof s?.id === 'string');
				}
			}
		} catch {
			this.sessions = [];
		}
	}

	/** Clear local state; sessions stay on the server. */
	close(): void {
		this.sessions = [];
		this.activeId = null;
	}

	active(): LocalPtySession | null {
		if (!this.activeId) return this.sessions[0] ?? null;
		return this.sessions.find((s) => s.id === this.activeId) ?? null;
	}

	byId(id: string): LocalPtySession | undefined {
		return this.sessions.find((s) => s.id === id);
	}

	/** Create a server session and make it active. */
	async spawn(options?: {
		command?: string;
		rows?: number;
		cols?: number;
	}): Promise<LocalPtySession | null> {
		try {
			const view = await createPty(options);
			const session: LocalPtySession = {
				id: view.id,
				title: `Terminal ${this.sessions.length + 1}`,
				rows: view.rows,
				cols: view.cols,
			};
			this.sessions = [...this.sessions, session];
			this.activeId = view.id;
			this.schedulePersist();
			return session;
		} catch {
			return null;
		}
	}

	/** Open an existing server session (already in the list) or adopt it. */
	async adopt(id: string): Promise<void> {
		try {
			const view = await getPty(id);
			const existing = this.byId(id);
			if (existing) {
				this.patch(id, {
					rows: view.rows,
					cols: view.cols,
					status: view.status,
					exitCode: view.exit_code ?? undefined,
				});
			} else {
				this.sessions = [
					...this.sessions,
					{
						id: view.id,
						title: view.id,
						rows: view.rows,
						cols: view.cols,
						status: view.status,
						exitCode: view.exit_code ?? undefined,
					},
				];
			}
			this.activeId = id;
			this.schedulePersist();
		} catch {
			// Missing sessions are dropped from the local list.
			this.sessions = this.sessions.filter((s) => s.id !== id);
			if (this.activeId === id) {
				this.activeId = this.sessions[0]?.id ?? null;
			}
		}
	}

	setActive(id: string): void {
		if (this.byId(id)) this.activeId = id;
	}

	/** Remove a session locally and close it on the server. */
	async remove(id: string): Promise<void> {
		this.sessions = this.sessions.filter((s) => s.id !== id);
		if (this.activeId === id) {
			this.activeId = this.sessions[0]?.id ?? null;
		}
		this.schedulePersist();
		await closePty(id).catch(() => {});
	}

	patch(id: string, update: Partial<LocalPtySession>): void {
		this.sessions = this.sessions.map((s) =>
			s.id === id ? { ...s, ...update } : s,
		);
	}

	/** Record the serialized buffer after a connection cycle completes. */
	saveBuffer(id: string, buffer: string, scrollY: number, cursor: number): void {
		this.patch(id, { buffer, scrollY, cursor });
		this.schedulePersist();
	}

	/** Server confirmed status/size; drop the now-redundant local buffer. */
	syncFromServer(id: string, meta: PtySession): void {
		const existing = this.byId(id);
		if (!existing) return;
		this.patch(id, {
			rows: meta.rows,
			cols: meta.cols,
			status: meta.status,
			exitCode: meta.exit_code ?? undefined,
			buffer: undefined,
			scrollY: undefined,
		});
		this.schedulePersist();
	}

	/** Move a session one position left or right in the tab order. */
	move(id: string, direction: -1 | 1): void {
		const index = this.sessions.findIndex((s) => s.id === id);
		if (index < 0) return;
		const target = index + direction;
		if (target < 0 || target >= this.sessions.length) return;
		const next = [...this.sessions];
		[next[index], next[target]] = [next[target], next[index]];
		this.sessions = next;
		this.schedulePersist();
	}

	/**
	 * Recover a broken session by cloning it: the local buffer is kept so the
	 * user does not lose scrollback, but a fresh server session is spawned.
	 */
	async clone(id: string): Promise<void> {
		const previous = this.byId(id);
		if (!previous) return;
		const replacement = await this.spawn();
		if (!replacement) return;
		this.patch(replacement.id, {
			title: previous.title,
			buffer: previous.buffer,
			scrollY: previous.scrollY,
			cursor: previous.cursor,
			status: previous.status,
			exitCode: previous.exitCode,
		});
		this.schedulePersist();
		await this.remove(id);
	}

	private schedulePersist(): void {
		if (this.persistTimer !== undefined) clearTimeout(this.persistTimer);
		this.persistTimer = setTimeout(() => {
			this.persistTimer = undefined;
			if (!this.repo) return;
			try {
				localStorage.setItem(
					storageKey(this.repo),
					JSON.stringify(this.sessions),
				);
			} catch {
				// Storage full or unavailable; buffers stay in memory only.
			}
		}, 300);
	}
}

/** Shared store registry keyed by repo root (one store per open repo). */
const stores = new SvelteMap<string, PtySessionStore>();

export function ptyStoreFor(repo: string): PtySessionStore {
	let store = stores.get(repo);
	if (!store) {
		store = new PtySessionStore();
		store.open(repo);
		stores.set(repo, store);
	}
	return store;
}

export function resizeSession(
	id: string,
	rows: number,
	cols: number,
): Promise<PtySession> {
	return resizePty(id, rows, cols);
}
