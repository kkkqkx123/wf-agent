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
	/** Manual rename wins over process-reported titles until cleared. */
	manualTitle?: boolean;
	/** Last server-reported activity clock, refreshed by list/get/attach. */
	updatedAt?: number;
	/** Local input/output clocks for the busy/idle heuristic; transient. */
	lastInputAt?: number;
	lastOutputAt?: number;
	origin?: {
		cwd?: string;
		command?: string;
		env?: Record<string, string>;
	};
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
// Persisted scrollback tail budget, shared with the server attach replay
// budget: cutting the tail never invalidates the absolute cursor stored
// alongside it, and a stale cursor can at most over-replay this many bytes.
// Keep in sync with REPLAY_BUDGET_BYTES in wf-api pty state.
export const PTY_REPLAY_BUDGET_BYTES = 64 * 1024;
const MAX_PERSIST_BUFFER_BYTES = PTY_REPLAY_BUDGET_BYTES;
const MAX_VIEWPORT_EDGE = 4096;
const PERSIST_SCHEMA_VERSION = 1;
const MAX_TITLE_LENGTH = 120;
/** Idle threshold for the busy/idle heuristic (an estimate, not a task signal). */
export const PTY_BUSY_IDLE_MS = 45_000;
const MAX_TITLE_NUMBER = 10_000;
const MAX_CURSOR_VALUE = 64 * 1024 * 1024;
const MAX_ENV_VARS = 64;
const MAX_ENV_VALUE_BYTES = 4096;
const MAX_PERSIST_TOTAL_BYTES = 4 * 1024 * 1024;

/** Explicit session lifecycle: live sessions stream, exited ones keep scrollback, missing ones are gone. */
export type PtyLifecycle = 'live' | 'exited' | 'missing';

/** Connection phase owned by the session store; views only report socket events. */
export type PtyConnectionPhase = 'live' | 'reconnecting' | 'exited' | 'removed';

export interface PtyConnectionState {
	phase: PtyConnectionPhase;
	retries: number;
	updatedAt: number;
}

const TERMINAL_CLOSE_CODES = new Set([4001, 4004, 4010]);

const textEncoder = new TextEncoder();

/** Removed session ids for this page lifetime. Server ids are unique per
 * load, so a removed id never legitimately returns: every transition
 * consults this set, which suppresses writes from any store object
 * generation or stale teardown, plus the pre-flush reopen window. Reads stay
 * reactive so future deriveds observe tombstoning. Bounded by evicting the
 * oldest entries, whose in-flight writes have long settled and whose ids no
 * longer appear in rewritten storage. */
const removedPtyIds = new SvelteSet<string>();
const MAX_REMOVED_PTY_IDS = 500;

function rememberRemoved(id: string): void {
	removedPtyIds.add(id);
	if (removedPtyIds.size > MAX_REMOVED_PTY_IDS) {
		const oldest = removedPtyIds.values().next().value;
		if (oldest !== undefined) removedPtyIds.delete(oldest);
	}
}

function wasRemoved(id: string): boolean {
	return removedPtyIds.has(id);
}

function byteLength(text: string): number {
	for (let i = 0; i < text.length; i += 1) {
		if (text.charCodeAt(i) > 127) return textEncoder.encode(text).length;
	}
	return text.length;
}

function truncateBuffer(buffer: string): {
	buffer: string;
	truncated: boolean;
} {
	if (byteLength(buffer) <= MAX_PERSIST_BUFFER_BYTES) {
		return { buffer, truncated: false };
	}
	// Walk back from the tail by code points so the cut lands on a UTF-8
	// boundary; astral characters never tear.
	const points = [...buffer];
	let keptBytes = 0;
	let start = points.length;
	for (let i = points.length - 1; i >= 0; i -= 1) {
		const point = points[i] ?? '';
		const first = point.charCodeAt(0) || 0;
		keptBytes += point.length > 1 || first > 127 ? textEncoder.encode(point).length : 1;
		if (keptBytes > MAX_PERSIST_BUFFER_BYTES) break;
		start = i;
	}
	return { buffer: points.slice(start).join(''), truncated: true };
}

function validViewportEdge(value: unknown): number | undefined {
	return typeof value === 'number' &&
		Number.isInteger(value) &&
		value >= 1 &&
		value <= MAX_VIEWPORT_EDGE
		? value
		: undefined;
}

function validNonNegativeInt(value: unknown): number | undefined {
	return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

function validStatus(value: unknown): 'running' | 'exited' | undefined {
	return value === 'running' || value === 'exited' ? value : undefined;
}

function validTitle(value: unknown, fallback: string): string {
	const raw = typeof value === 'string' && value.trim() ? value.trim() : fallback;
	return raw.length > MAX_TITLE_LENGTH ? raw.slice(0, MAX_TITLE_LENGTH) : raw;
}

function validManualTitle(value: unknown): boolean | undefined {
	return value === true ? true : undefined;
}

function validTitleNumber(value: unknown): number | undefined {
	return typeof value === 'number' &&
		Number.isSafeInteger(value) &&
		value > 0 &&
		value <= MAX_TITLE_NUMBER
		? value
		: undefined;
}

function validCursor(value: unknown): number | undefined {
	return typeof value === 'number' &&
		Number.isSafeInteger(value) &&
		value >= 0 &&
		value <= MAX_CURSOR_VALUE
		? value
		: undefined;
}

function validExitCode(value: unknown): number | undefined {
	return typeof value === 'number' &&
		Number.isSafeInteger(value) &&
		value >= -32768 &&
		value <= 32768
		? value
		: undefined;
}

function validOrigin(value: unknown): LocalPtySession['origin'] {
	if (typeof value !== 'object' || value === null || Array.isArray(value)) return undefined;
	const raw = value as Record<string, unknown>;
	const out: { cwd?: string; command?: string; env?: Record<string, string> } = {};
	if (typeof raw.cwd === 'string' && raw.cwd) out.cwd = raw.cwd.slice(0, 4096);
	if (typeof raw.command === 'string' && raw.command) out.command = raw.command.slice(0, 8192);
	if (typeof raw.env === 'object' && raw.env !== null && !Array.isArray(raw.env)) {
		const env: Record<string, string> = {};
		for (const [key, entry] of Object.entries(raw.env as Record<string, unknown>)) {
			if (Object.keys(env).length >= MAX_ENV_VARS) break;
			if (typeof entry !== 'string' || !key || key.length > 256) continue;
			env[key] = entry.length > MAX_ENV_VALUE_BYTES ? entry.slice(0, MAX_ENV_VALUE_BYTES) : entry;
		}
		if (Object.keys(env).length > 0) out.env = env;
	}
	return Object.keys(out).length > 0 ? out : undefined;
}

/** Fully validate one persisted record; unknown fields are dropped, unusable records return null. */
function validatePersistedSession(raw: unknown): LocalPtySession | null {
	if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return null;
	const item = raw as Record<string, unknown>;
	if (typeof item.id !== 'string' || !item.id || item.id.length > 256) return null;
	if (item.v !== undefined && item.v !== PERSIST_SCHEMA_VERSION) return null;
	const title = validTitle(item.title, item.id);
	const numbered = validTitleNumber(item.titleNumber) ?? parseTitleNumber(title);
	const manualTitle = validManualTitle(item.manualTitle);
	let buffer = typeof item.buffer === 'string' ? item.buffer : undefined;
	let scrollY = validNonNegativeInt(item.scrollY);
	if (buffer !== undefined) {
		const cut = truncateBuffer(buffer);
		buffer = cut.buffer;
		if (cut.truncated) scrollY = 0;
	}
	const rows = validViewportEdge(item.rows);
	const cols = validViewportEdge(item.cols);
	const cursor = validCursor(item.cursor);
	const status = validStatus(item.status);
	const origin = validOrigin(item.origin);
	const exitCode = validExitCode(item.exitCode);
	return {
		id: item.id,
		title,
		...(numbered ? { titleNumber: numbered } : {}),
		...(manualTitle ? { manualTitle } : {}),
		...(origin ? { origin } : {}),
		...(rows !== undefined ? { rows } : {}),
		...(cols !== undefined ? { cols } : {}),
		...(buffer !== undefined ? { buffer } : {}),
		...(scrollY !== undefined ? { scrollY } : {}),
		...(cursor !== undefined ? { cursor } : {}),
		...(status !== undefined ? { status } : {}),
		...(exitCode !== undefined ? { exitCode } : {}),
	};
}

function storageKey(repo: string): string {
	return `${STORAGE_PREFIX}.${repo}`;
}

function parseTitleNumber(title: string): number | undefined {
	const match = /(\d+)\s*$/.exec(title.trim());
	if (!match) return undefined;
	const n = Number(match[1]);
	return Number.isSafeInteger(n) && n > 0 && n <= MAX_TITLE_NUMBER ? n : undefined;
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
	hasStoredRecord = false;

	private persistTimer: ReturnType<typeof setTimeout> | undefined;
	private lastViewport: { rows: number; cols: number } | undefined;
	private spawnQueue: Promise<void> = Promise.resolve();
	private cloneInFlight = new Set<string>();
	private connections = new SvelteMap<string, PtyConnectionState>();
	/**
	 * Hidden tab ids. Kept beside the session records instead of on them so
	 * buffer, cursor and metadata rewrites can never drop the hidden state.
	 */
	private detachedIds = new SvelteSet<string>();

	/** Restore persisted sessions for a repo and prune stale records. */
	open(repo: string): void {
		this.repo = repo;
		this.sessions = [];
		this.detachedIds.clear();
		this.activeId = null;
		this.hasStoredRecord = false;
		try {
			const raw = localStorage.getItem(storageKey(repo));
			if (raw) {
				const parsed = JSON.parse(raw) as {
					v?: unknown;
					sessions?: unknown;
					detachedIds?: unknown;
				};
				if (
					typeof parsed === 'object' &&
					parsed !== null &&
					(parsed.v === undefined || parsed.v === PERSIST_SCHEMA_VERSION) &&
					Array.isArray(parsed.sessions)
				) {
					const seen = new SvelteSet<string>();
					const cleaned: LocalPtySession[] = [];
					for (const item of parsed.sessions) {
						const session = validatePersistedSession(item);
						if (!session || seen.has(session.id)) continue;
						if (wasRemoved(session.id)) continue;
						seen.add(session.id);
						cleaned.push(session);
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
					// Total-bytes ceiling: shed scrollback tails from the end first so
					// titles and cursors survive a quota blowout instead of losing tabs.
					try {
						while (JSON.stringify(cleaned).length > MAX_PERSIST_TOTAL_BYTES) {
							const victim = [...cleaned].reverse().find((s) => s.buffer);
							if (!victim) break;
							victim.buffer = undefined;
							victim.scrollY = undefined;
						}
						while (JSON.stringify(cleaned).length > MAX_PERSIST_TOTAL_BYTES && cleaned.length > 0) {
							cleaned.pop();
						}
					} catch {
						// Size probing failed; keep the count-capped list as-is.
					}
					this.sessions = cleaned;
					if (Array.isArray(parsed.detachedIds)) {
						for (const entry of parsed.detachedIds) {
							if (typeof entry === 'string' && entry && seen.has(entry)) {
								this.detachedIds.add(entry);
							}
						}
					}
					this.hasStoredRecord = true;
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

	/** True once remove() recorded this id. Removed is terminal: every
	 * transition consults it so no path revives the id as running or exited. */
	isRemoved(id: string): boolean {
		return wasRemoved(id);
	}

	/** Classify one session for explicit lifecycle transitions (spawn, exit, prune). */
	lifecycle(id: string): PtyLifecycle {
		const session = this.byId(id);
		if (!session) return 'missing';
		return session.status === 'exited' ? 'exited' : 'live';
	}

	/** Connection phase for the explicit state machine; views report, the store decides. */
	connectionPhase(id: string): PtyConnectionPhase {
		if (wasRemoved(id)) return 'removed';
		const session = this.byId(id);
		if (!session) return 'removed';
		if (session.status === 'exited') return 'exited';
		return this.connections.get(id)?.phase ?? 'live';
	}

	connectionRetries(id: string): number {
		return this.connections.get(id)?.retries ?? 0;
	}

	/** Socket opened and replay meta arrived: the session streams. */
	reportSocketOpen(id: string): void {
		if (!this.byId(id) || wasRemoved(id)) return;
		this.connections.set(id, { phase: 'live', retries: 0, updatedAt: Date.now() });
	}

	/** Socket closed: terminal close codes end the session, others reconnect. */
	reportSocketClose(id: string, code: number): void {
		if (!this.byId(id) || wasRemoved(id)) return;
		if (TERMINAL_CLOSE_CODES.has(code)) {
			this.markExited(id);
			return;
		}
		const previous = this.connections.get(id);
		this.connections.set(id, {
			phase: 'reconnecting',
			retries: (previous?.retries ?? 0) + 1,
			updatedAt: Date.now(),
		});
	}

	/** Server exited frame or process-end probe: keep scrollback, stop streaming. */
	markExited(id: string, exitCode?: number): void {
		if (!this.byId(id) || wasRemoved(id)) return;
		this.patch(id, {
			status: 'exited',
			...(exitCode !== undefined ? { exitCode } : {}),
		});
		this.connections.set(id, {
			phase: 'exited',
			retries: this.connections.get(id)?.retries ?? 0,
			updatedAt: Date.now(),
		});
		this.schedulePersist();
	}

	/** Reconnect budget spent without a socket: surface the failure banner. */
	markConnectFailed(id: string): void {
		if (!this.byId(id) || wasRemoved(id)) return;
		const previous = this.connections.get(id);
		this.connections.set(id, {
			phase: 'reconnecting',
			retries: previous?.retries ?? 0,
			updatedAt: Date.now(),
		});
	}

	clearConnection(id: string): void {
		this.connections.delete(id);
	}

	/** Headless keepalive advances the resume cursor without touching scrollback. */
	syncCursor(id: string, cursor: number): void {
		if (!this.byId(id) || wasRemoved(id)) return;
		if (!Number.isSafeInteger(cursor) || cursor < 0 || cursor > MAX_CURSOR_VALUE) return;
		const current = this.byId(id)?.cursor ?? 0;
		if (cursor < current) return;
		if (cursor === current) return;
		this.patch(id, { cursor });
		this.schedulePersist();
	}

	/** True when at least one session can still stream output. */
	hasLive(): boolean {
		return this.sessions.some((s) => s.status !== 'exited');
	}

	/** True when tabs exist but every one has ended; an empty list is not "all exited". */
	allExited(): boolean {
		return this.sessions.length > 0 && this.sessions.every((s) => s.status === 'exited');
	}

	/** Visible tabs: every session that is not detached. */
	attachedSessions(): LocalPtySession[] {
		return this.sessions.filter((s) => !this.detachedIds.has(s.id));
	}

	/** Hidden tabs: detached sessions keep their server process. */
	detachedSessions(): LocalPtySession[] {
		return this.sessions.filter((s) => this.detachedIds.has(s.id));
	}

	/** True when the tab is hidden but its server process is kept. */
	isDetached(id: string): boolean {
		return this.detachedIds.has(id);
	}

	/** True when at least one visible session can still stream output. */
	hasAttachedLive(): boolean {
		return this.sessions.some((s) => !this.detachedIds.has(s.id) && s.status !== 'exited');
	}

	/**
	 * Hide a tab without ending its server process. Reports whether the tab
	 * was actually hidden; an already hidden tab is left untouched so a
	 * repeated click can never flip it back.
	 */
	detach(id: string): boolean {
		if (!this.byId(id) || this.isRemoved(id) || this.detachedIds.has(id)) return false;
		this.detachedIds.add(id);
		if (this.activeId === id) {
			this.activeId =
				this.sessions.find((s) => s.id !== id && !this.detachedIds.has(s.id))?.id ?? null;
		}
		this.schedulePersist();
		return true;
	}

	/**
	 * Reopen a detached tab and make it active. Reports whether the tab was
	 * actually reopened. Only this path clears the hidden state.
	 */
	attach(id: string): boolean {
		if (!this.byId(id) || this.isRemoved(id) || !this.detachedIds.has(id)) return false;
		this.detachedIds.delete(id);
		this.activeId = id;
		this.schedulePersist();
		return true;
	}

	/**
	 * Lift a record out of this store without ending its server process and
	 * without recording a removal tombstone, so another store can adopt it.
	 * The hidden state travels with the record instead of staying behind.
	 */
	eject(id: string): { record: LocalPtySession; detached: boolean } | null {
		const session = this.byId(id);
		if (!session) return null;
		const detached = this.detachedIds.has(id);
		this.detachedIds.delete(id);
		this.sessions = this.sessions.filter((s) => s.id !== id);
		this.connections.delete(id);
		if (this.activeId === id) {
			this.activeId = this.sessions.find((s) => !this.detachedIds.has(s.id))?.id ?? null;
		}
		this.schedulePersist();
		return { record: session, detached };
	}

	/**
	 * Adopt a record ejected from another store. The server session is
	 * untouched; only local metadata moves. An already present id is only
	 * focused, never rehidden or reshown. Returns false when full.
	 */
	ingest(record: LocalPtySession, detached = false): boolean {
		if (this.isRemoved(record.id)) return false;
		if (this.byId(record.id)) {
			this.activeId = record.id;
			this.schedulePersist();
			return true;
		}
		if (this.sessions.length >= MAX_TERMINAL_SESSIONS) return false;
		const taken = new SvelteSet(
			this.sessions.flatMap((s) => (s.titleNumber ? [s.titleNumber] : [])),
		);
		const numbered =
			record.titleNumber && !taken.has(record.titleNumber) ? record.titleNumber : undefined;
		const next: LocalPtySession = { ...record };
		if (numbered) next.titleNumber = numbered;
		else {
			let n = 1;
			while (taken.has(n)) n += 1;
			next.titleNumber = n;
		}
		this.sessions = [...this.sessions, next];
		if (detached) this.detachedIds.add(next.id);
		this.activeId = next.id;
		this.schedulePersist();
		return true;
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

	/** Create a server session and make it active. Queued so every click creates one. */
	async spawn(options?: {
		cwd?: string;
		command?: string;
		rows?: number;
		cols?: number;
		env?: Record<string, string>;
	}): Promise<LocalPtySession | null> {
		const previous = this.spawnQueue;
		let release: () => void = () => {};
		this.spawnQueue = new Promise<void>((resolve) => {
			release = resolve;
		});
		await previous;
		try {
			if (this.sessions.length >= MAX_TERMINAL_SESSIONS) {
				return null;
			}
			const number = this.pickNextNumber();
			const title = `Terminal ${number}`;
			try {
				const view = await createPty({
					...options,
					rows: options?.rows ?? this.lastViewport?.rows,
					cols: options?.cols ?? this.lastViewport?.cols,
					title,
					title_source: 'default',
				});
				// Server ids are never reused, but a reused id must not turn into
				// an unusable tab: release it on the server and report failure.
				if (wasRemoved(view.id)) {
					await closePty(view.id).catch(() => {});
					return null;
				}
				const now = Date.now();
				const session: LocalPtySession = {
					id: view.id,
					title: view.title || title,
					titleNumber: number,
					updatedAt: view.last_activity ?? now,
					lastInputAt: now,
					lastOutputAt: now,
					origin:
						options?.cwd !== undefined ||
						options?.command !== undefined ||
						options?.env !== undefined
							? {
									...(options?.cwd !== undefined ? { cwd: options.cwd } : {}),
									...(options?.command !== undefined ? { command: options.command } : {}),
									...(options?.env !== undefined ? { env: options.env } : {}),
								}
							: undefined,
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
		} finally {
			release();
		}
	}

	/** Open an existing server session (already in the list) or adopt it. */
	async adopt(id: string): Promise<void> {
		if (this.isRemoved(id)) return;
		try {
			const view = await getPty(id);
			const existing = this.byId(id);
			if (existing) {
				const manual = existing.manualTitle === true || view.title_source === 'manual';
				this.patch(id, {
					rows: view.rows,
					cols: view.cols,
					status: view.status,
					exitCode: view.exit_code ?? undefined,
					title: manual && existing.title ? existing.title : view.title || existing.title,
					...(manual ? { manualTitle: true as const } : {}),
					updatedAt: view.last_activity ?? Date.now(),
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
						updatedAt: view.last_activity ?? Date.now(),
						lastInputAt: Date.now(),
						lastOutputAt: Date.now(),
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
		const wasManual = current.manualTitle === true;
		this.patch(id, { title: trimmed, manualTitle: true });
		this.schedulePersist();
		void updatePty(id, { title: trimmed, title_source: 'manual' }).catch(() => {
			if (this.byId(id)) this.patch(id, { title: previous, manualTitle: wasManual || undefined });
		});
	}

	/** Clear the manual flag so process-reported titles apply again. */
	clearManualTitle(id: string): void {
		const current = this.byId(id);
		if (!current || current.manualTitle !== true) return;
		this.patch(id, { manualTitle: undefined });
		this.schedulePersist();
	}

	/**
	 * Accept a process-reported title (OSC title sequence observed by the
	 * terminal view). Ignored once the user renamed the session manually.
	 */
	adoptAutoTitle(id: string, title: string): void {
		const current = this.byId(id);
		if (!current || this.isRemoved(id) || current.manualTitle === true) return;
		const trimmed = title.trim();
		if (!trimmed || current.title === trimmed) return;
		const clean = trimmed.length > MAX_TITLE_LENGTH ? trimmed.slice(0, MAX_TITLE_LENGTH) : trimmed;
		const previous = current.title;
		this.patch(id, { title: clean });
		this.schedulePersist();
		void updatePty(id, { title: clean, title_source: 'auto' })
			.then((view) => {
				if (!this.byId(id)) return;
				this.patch(id, {
					title: view.title || clean,
					...(view.title_source === 'manual' ? { manualTitle: true as const } : {}),
					updatedAt: view.last_activity ?? Date.now(),
				});
				this.schedulePersist();
			})
			.catch(() => {
				if (this.byId(id)) this.patch(id, { title: previous });
			});
	}

	/** Record user input for the busy/idle heuristic. */
	reportInput(id: string): void {
		if (!this.byId(id) || this.isRemoved(id)) return;
		this.patch(id, { lastInputAt: Date.now() });
	}

	/** Record terminal output for the busy/idle heuristic. */
	reportOutput(id: string): void {
		if (!this.byId(id) || this.isRemoved(id)) return;
		this.patch(id, { lastOutputAt: Date.now() });
	}

	/** Latest known activity clock (local input/output or server report). */
	lastActiveAt(id: string): number {
		const session = this.byId(id);
		if (!session) return 0;
		return Math.max(
			session.lastInputAt ?? 0,
			session.lastOutputAt ?? 0,
			session.updatedAt ?? 0,
			0,
		);
	}

	/**
	 * Busy/idle estimate for running sessions. Idle only means no recent
	 * input or output was observed; it never claims a task finished.
	 */
	isBusy(id: string, now: number = Date.now()): boolean {
		const session = this.byId(id);
		if (!session || session.status === 'exited') return false;
		return now - this.lastActiveAt(id) < PTY_BUSY_IDLE_MS;
	}

	/** Push viewport size to the server; local state updates immediately. */
	syncSize(id: string, rows: number, cols: number): void {
		if (!Number.isInteger(rows) || !Number.isInteger(cols) || rows < 1 || cols < 1) return;
		if (!this.byId(id) || this.isRemoved(id)) return;
		this.patch(id, { rows, cols });
		this.schedulePersist();
		const remember = () => {
			this.lastViewport = { rows, cols };
		};
		void updatePty(id, { rows, cols })
			.then(remember)
			.catch(() => {
				// Transient failure: retry once while the requested size is still current.
				if (this.byId(id)?.rows !== rows || this.byId(id)?.cols !== cols) return;
				setTimeout(() => {
					if (this.byId(id)?.rows !== rows || this.byId(id)?.cols !== cols) return;
					void updatePty(id, { rows, cols })
						.then(remember)
						.catch(() => {});
				}, 400);
			});
	}

	/** Remove a session locally and close it on the server. */
	async remove(id: string): Promise<void> {
		this.sessions = this.sessions.filter((s) => s.id !== id);
		rememberRemoved(id);
		this.detachedIds.delete(id);
		this.connections.delete(id);
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
		if (!this.byId(id) || this.isRemoved(id)) return;
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
		const startingIds = new Set(this.sessions.map((s) => s.id));
		let serverById: Map<string, PtySession>;
		try {
			const list = await listPtys();
			serverById = new Map(list.map((s) => [s.id, s]));
		} catch {
			return;
		}
		let changed = false;
		const kept: LocalPtySession[] = [];
		for (const s of this.sessions) {
			// Tombstoned ids never resurface, even when another store removed them.
			if (wasRemoved(s.id)) {
				changed = true;
				continue;
			}
			if (!startingIds.has(s.id)) {
				kept.push(s);
				continue;
			}
			if (serverById.has(s.id)) {
				const view = serverById.get(s.id)!;
				const manual = s.manualTitle === true || view.title_source === 'manual';
				const title = manual && s.title ? s.title : view.title || s.title;
				if (view.last_activity !== s.updatedAt || title !== s.title) {
					kept.push({
						...s,
						title,
						...(manual ? { manualTitle: true as const } : {}),
						updatedAt: view.last_activity ?? s.updatedAt,
					});
					changed = true;
				} else {
					kept.push(s);
				}
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

	/** Server confirmed metadata; viewport size stays owned by the terminal view. */
	syncMetaFromServer(id: string, meta: PtySession): void {
		const existing = this.byId(id);
		if (!existing || this.isRemoved(id)) return;
		const manual = existing.manualTitle === true || meta.title_source === 'manual';
		this.patch(id, {
			status: meta.status,
			exitCode: meta.exit_code ?? undefined,
			title: manual && existing.title ? existing.title : meta.title || existing.title,
			...(manual ? { manualTitle: true as const } : {}),
			updatedAt: meta.last_activity ?? Date.now(),
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
	async clone(
		id: string,
		options?: { cwd?: string; command?: string; env?: Record<string, string> },
	): Promise<void> {
		if (this.cloneInFlight.has(id)) return;
		const previous = this.byId(id);
		if (!previous) return;
		this.cloneInFlight.add(id);
		try {
			const effective = options ?? previous.origin;
			const replacement = await this.spawn(effective);
			if (!replacement) return;
			this.patch(replacement.id, {
				title: previous.title,
				...(previous.manualTitle === true ? { manualTitle: true as const } : {}),
				titleNumber: previous.titleNumber ?? replacement.titleNumber,
				buffer: previous.buffer,
				scrollY: previous.scrollY,
				cursor: previous.cursor,
			});
			this.schedulePersist();
			await this.remove(id);
			this.activeId = replacement.id;
			this.schedulePersist();
		} finally {
			this.cloneInFlight.delete(id);
		}
	}

	private persistNow(): void {
		if (!this.repo) return;
		try {
			// Transient activity clocks never hit storage; the server clock
			// is refreshed by reconcile on the next open.
			const versioned = this.sessions.map((s) => {
				const { lastInputAt: _input, lastOutputAt: _output, updatedAt: _updated, ...rest } =
					s;
				return { ...rest, v: PERSIST_SCHEMA_VERSION };
			});
			const payload = {
				v: PERSIST_SCHEMA_VERSION,
				sessions: versioned,
				detachedIds: this.sessions.filter((s) => this.detachedIds.has(s.id)).map((s) => s.id),
			};
			localStorage.setItem(storageKey(this.repo), JSON.stringify(payload));
			if (this.activeId) {
				localStorage.setItem(`${storageKey(this.repo)}.active`, this.activeId);
			} else {
				localStorage.removeItem(`${storageKey(this.repo)}.active`);
			}
			this.hasStoredRecord = true;
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

/** Normalize a repo root into a store key. Empty stays empty so callers
 * without a resolved root defer session creation instead of polluting
 * a placeholder store. */
export function normalizePtyRepoKey(value: unknown): string {
	if (typeof value !== 'string') return '';
	return value.trim();
}

/** Shared store registry keyed by repo root (one store per open repo). */
const stores = new SvelteMap<string, PtySessionStore>();

export function ptyStoreFor(repo: string): PtySessionStore {
	const key = normalizePtyRepoKey(repo);
	let store = stores.get(key);
	if (!store) {
		store = new PtySessionStore();
		store.open(key);
		stores.set(key, store);
	}
	return store;
}

/** All live store objects for the layout host keepalive sweep. */
export function allPtyStores(): PtySessionStore[] {
	return [...stores.values()];
}

const MAX_DISCOVERED_STORE_KEYS = 10;

/**
 * Lazily open foreign stores found in persisted storage so the attach
 * dialog can list sessions from repositories this page load never mounted
 * (renamed roots, historical strays, preview demo seeds). Removal
 * tombstones still apply on open, so killed sessions never resurface.
 */
export function discoverForeignPtyStores(current: PtySessionStore): PtySessionStore[] {
	const found: PtySessionStore[] = [];
	try {
		const prefix = `${STORAGE_PREFIX}.`;
		const total = localStorage.length;
		for (let i = 0; i < total && found.length < MAX_DISCOVERED_STORE_KEYS; i += 1) {
			const key = localStorage.key(i);
			if (!key || !key.startsWith(prefix)) continue;
			const repo = key.slice(prefix.length);
			if (!repo || repo.endsWith('.active')) continue;
			const candidate = ptyStoreFor(repo);
			if (candidate === current || candidate.sessions.length === 0) continue;
			if (!found.includes(candidate)) found.push(candidate);
		}
	} catch {
		// Storage unavailable; only already-open stores are listed.
	}
	return found;
}

/** Module-level removed check for registries that never hold a store. */
export function isPtyRemoved(id: string): boolean {
	return wasRemoved(id);
}
