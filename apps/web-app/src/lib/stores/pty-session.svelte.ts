import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { createPty, closePty, updatePty, getPty, listPtys } from '#lib/services/pty';
import type { PtySession } from '#lib/services/pty';

/**
 * Local view of a server PTY session. Buffer/scrollY/cursor live only in
 * the browser (serialized terminal output for reconnect replay); the server
 * keeps the session itself plus its viewport size. Title numbering is local
 * until the backend persists titles.
 */
export interface LocalPtySession {
	id: string;
	title: string;
	titleNumber?: number;
	rows?: number;
	cols?: number;
	buffer?: string;
	scrollY?: number;
	cursor?: number;
	status?: 'running' | 'exited';
	exitCode?: number;
}

const STORAGE_PREFIX = 'wf.pty.v1';
const MAX_TERMINAL_SESSIONS = 20;
const MAX_PERSIST_BUFFER_CHARS = 200_000;

function truncateBuffer(buffer: string): {
	buffer: string;
	truncated: boolean;
} {
	if (buffer.length <= MAX_PERSIST_BUFFER_CHARS) {
		return { buffer, truncated: false };
	}
	let kept = buffer.slice(buffer.length - MAX_PERSIST_BUFFER_CHARS);
	const first = kept.charCodeAt(0);
	if (first >= 0xdc00 && first <= 0xdfff) {
		kept = kept.slice(1);
	}
	return { buffer: kept, truncated: true };
}

function storageKey(repo: string): string {
	return `${STORAGE_PREFIX}.${repo}`;
}

function parseTitleNumber(title: string): number | undefined {
	const match = /(\d+)\s*$/.exec(title.trim());
	if (!match) return undefined;
	const n = Number(match[1]);
	return Number.isSafeInteger(n) && n > 0 ? n : undefined;
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
					const seen = new SvelteSet<string>();
					const cleaned: LocalPtySession[] = [];
					for (const s of parsed) {
						if (typeof s?.id !== 'string' || seen.has(s.id)) continue;
						seen.add(s.id);
						const title = typeof s.title === 'string' && s.title ? s.title : s.id;
						const numbered =
							typeof s.titleNumber === 'number' &&
							Number.isSafeInteger(s.titleNumber) &&
							s.titleNumber > 0
								? s.titleNumber
								: parseTitleNumber(title);
						let buffer = typeof s.buffer === 'string' ? s.buffer : undefined;
						let scrollY = typeof s.scrollY === 'number' ? s.scrollY : undefined;
						if (buffer !== undefined) {
							const cut = truncateBuffer(buffer);
							buffer = cut.buffer;
							if (cut.truncated) scrollY = 0;
						}
						cleaned.push({
							...s,
							title,
							buffer,
							scrollY,
							...(numbered ? { titleNumber: numbered } : {}),
						});
						if (cleaned.length >= MAX_TERMINAL_SESSIONS) break;
					}
					const used = new SvelteSet(
						cleaned.flatMap((s) => (s.titleNumber ? [s.titleNumber] : [])),
					);
					let next = 1;
					for (const s of cleaned) {
						if (s.titleNumber) continue;
						while (used.has(next)) next += 1;
						s.titleNumber = next;
						used.add(next);
					}
					this.sessions = cleaned;
					const rawActive = (() => {
						try {
							const marker = localStorage.getItem(`${storageKey(repo)}.active`);
							return typeof marker === 'string' ? marker : null;
						} catch {
							return null;
						}
					})();
					this.activeId = rawActive && seen.has(rawActive) ? rawActive : (cleaned[0]?.id ?? null);
				}
			}
		} catch {
			this.sessions = [];
		}
	}

	/** Keep sessions across panel hide and route leave; only flush timers. */
	close(): void {
		if (this.persistTimer !== undefined) clearTimeout(this.persistTimer);
		this.persistTimer = undefined;
		this.flushPersist();
	}

	active(): LocalPtySession | null {
		if (!this.activeId) return this.sessions[0] ?? null;
		return this.sessions.find((s) => s.id === this.activeId) ?? null;
	}

	byId(id: string): LocalPtySession | undefined {
		return this.sessions.find((s) => s.id === id);
	}

	private pickNextNumber(): number {
		const used = new SvelteSet(
			this.sessions.flatMap((s) => {
				if (typeof s.titleNumber === 'number' && s.titleNumber > 0) return [s.titleNumber];
				const parsed = parseTitleNumber(s.title);
				return parsed ? [parsed] : [];
			}),
		);
		let next = 1;
		while (used.has(next)) next += 1;
		return next;
	}

	/** Create a server session and make it active. */
	async spawn(options?: {
		command?: string;
		rows?: number;
		cols?: number;
	}): Promise<LocalPtySession | null> {
		if (this.sessions.length >= MAX_TERMINAL_SESSIONS) return null;
		const number = this.pickNextNumber();
		const title = `Terminal ${number}`;
		try {
			const view = await createPty({ ...options, title });
			const session: LocalPtySession = {
				id: view.id,
				title: view.title || title,
				titleNumber: number,
				rows: view.rows,
				cols: view.cols,
				status: view.status,
				exitCode: view.exit_code ?? undefined,
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
					title: view.title || existing.title,
				});
			} else {
				if (this.sessions.length >= MAX_TERMINAL_SESSIONS) {
					this.activeId = this.sessions[0]?.id ?? null;
					return;
				}
				const number = this.pickNextNumber();
				this.sessions = [
					...this.sessions,
					{
						id: view.id,
						title: `Terminal ${number}`,
						titleNumber: number,
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
			this.schedulePersist();
		}
	}

	setActive(id: string): void {
		if (this.byId(id)) {
			this.activeId = id;
			this.schedulePersist();
		}
	}

	rename(id: string, title: string): void {
		const trimmed = title.trim();
		const current = this.byId(id);
		if (!trimmed || !current || current.title === trimmed) return;
		const previous = current.title;
		this.patch(id, { title: trimmed });
		this.schedulePersist();
		void updatePty(id, { title: trimmed }).catch(() => {
			if (this.byId(id)) this.patch(id, { title: previous });
		});
	}

	/** Push viewport size to the server; local state updates immediately. */
	syncSize(id: string, rows: number, cols: number): void {
		this.patch(id, { rows, cols });
		this.schedulePersist();
		void updatePty(id, { rows, cols }).catch(() => {});
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
		this.sessions = this.sessions.map((s) => (s.id === id ? { ...s, ...update } : s));
	}

	/** Record the serialized buffer after a connection cycle completes. */
	saveBuffer(id: string, buffer: string, scrollY: number, cursor: number): void {
		const cut = truncateBuffer(buffer);
		this.patch(id, {
			buffer: cut.buffer,
			scrollY: cut.truncated ? 0 : scrollY,
			cursor,
		});
		this.schedulePersist();
	}

	/** Reconcile local sessions against the server list. */
	async reconcile(): Promise<void> {
		if (!this.repo || this.sessions.length === 0) return;
		let serverIds: SvelteSet<string>;
		try {
			const list = await listPtys();
			serverIds = new SvelteSet(list.map((s) => s.id));
		} catch {
			return;
		}
		let changed = false;
		const kept: LocalPtySession[] = [];
		for (const s of this.sessions) {
			if (serverIds.has(s.id)) {
				kept.push(s);
				continue;
			}
			if (s.buffer) {
				if (s.status !== 'exited') {
					kept.push({ ...s, status: 'exited' });
					changed = true;
				} else {
					kept.push(s);
				}
				continue;
			}
			changed = true;
		}
		if (changed) {
			this.sessions = kept;
			if (this.activeId && !kept.some((s) => s.id === this.activeId)) {
				this.activeId = kept[0]?.id ?? null;
			}
			this.schedulePersist();
		}
	}

	/** Server confirmed state; drop the now-redundant local buffer. */
	syncFromServer(id: string, meta: PtySession): void {
		const existing = this.byId(id);
		if (!existing) return;
		this.patch(id, {
			rows: meta.rows,
			cols: meta.cols,
			status: meta.status,
			exitCode: meta.exit_code ?? undefined,
			title: meta.title || existing.title,
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
			titleNumber: previous.titleNumber ?? replacement.titleNumber,
			buffer: previous.buffer,
			scrollY: previous.scrollY,
			cursor: previous.cursor,
		});
		this.schedulePersist();
		await this.remove(id);
		this.activeId = replacement.id;
		this.schedulePersist();
	}

	private persistNow(): void {
		if (!this.repo) return;
		try {
			localStorage.setItem(storageKey(this.repo), JSON.stringify(this.sessions));
			if (this.activeId) {
				localStorage.setItem(`${storageKey(this.repo)}.active`, this.activeId);
			} else {
				localStorage.removeItem(`${storageKey(this.repo)}.active`);
			}
		} catch {
			// Storage full or unavailable; buffers stay in memory only.
		}
	}

	private flushPersist(): void {
		this.persistNow();
	}

	private schedulePersist(): void {
		if (this.persistTimer !== undefined) clearTimeout(this.persistTimer);
		this.persistTimer = setTimeout(() => {
			this.persistTimer = undefined;
			this.persistNow();
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
