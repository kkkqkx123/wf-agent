/**
 * Diagnostic coverage for "closing a terminal needs two clicks" in preview.
 *
 * The reported trace shows `store.remove` dropping the session
 * (remaining: 1) while a millisecond-later `store.saveBuffer` for the same
 * id still finds it, and the next click again sees two sessions. Under one
 * store object that sequence is impossible (remove filters synchronously,
 * saveBuffer is guarded by byId, and only spawn/adopt/open can introduce an
 * id), so these tests pin down every path that can still surface the same
 * id afterwards:
 * - teardown writes racing the removal (must stay no-ops),
 * - reopening a repo key before the debounced persist flush (restores the
 *   pre-remove list with identical ids),
 * - two live store objects opened on one key (identical ids in both).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PtySession } from '#lib/services/pty';

interface FakePty {
	id: string;
	cwd: string;
	title: string;
	status: 'running' | 'exited';
	exit_code: number | null;
	rows: number;
	cols: number;
}

const server = new Map<string, FakePty>();
let seq = 0;

function delay(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

vi.mock('#lib/services/pty', () => ({
	createPty: vi.fn(async (input: { title?: string } = {}) => {
		await delay(30);
		seq += 1;
		const session: FakePty = {
			id: `pty-close-restore-${seq}`,
			cwd: '/test',
			title: input.title ?? `Terminal ${seq}`,
			status: 'running',
			exit_code: null,
			rows: 24,
			cols: 80,
		};
		server.set(session.id, { ...session });
		return { ...session };
	}),
	listPtys: vi.fn(async () => {
		await delay(30);
		return [...server.values()].map((s) => ({ ...s }));
	}),
	getPty: vi.fn(async (id: string) => {
		await delay(30);
		const session = server.get(id);
		if (!session) throw new Error(`PTY session not found: ${id}`);
		return { ...session };
	}),
	updatePty: vi.fn(
		async (id: string, patch: { title?: string; rows?: number; cols?: number }) => {
			await delay(30);
			const session = server.get(id);
			if (!session) throw new Error(`PTY session not found: ${id}`);
			const next = {
				...session,
				title: patch.title ?? session.title,
				rows: patch.rows ?? session.rows,
				cols: patch.cols ?? session.cols,
			};
			server.set(id, next);
			return { ...next };
		},
	),
	closePty: vi.fn(async (id: string) => {
		await delay(30);
		if (!server.delete(id)) throw new Error(`PTY session not found: ${id}`);
	}),
}));

function installLocalStorage(): void {
	const data = new Map<string, string>();
	const storage = {
		getItem: (key: string): string | null => data.get(key) ?? null,
		setItem: (key: string, value: string): void => {
			data.set(key, String(value));
		},
		removeItem: (key: string): void => {
			data.delete(key);
		},
		clear: (): void => {
			data.clear();
		},
	} as Storage;
	Object.defineProperty(globalThis, 'localStorage', {
		value: storage,
		configurable: true,
		writable: true,
	});
}

installLocalStorage();

const { PtySessionStore, ptyStoreFor } = await import('#lib/stores/pty-session.svelte');
const ptyService = await import('#lib/services/pty');

let repoSeq = 0;
function freshRepo(): string {
	repoSeq += 1;
	return `/close-restore-repo-${repoSeq}`;
}

/** Longer than the store persist debounce so localStorage settles. */
function flushPersist(): Promise<void> {
	return delay(400);
}

describe('terminal close teardown races', () => {
	beforeEach(() => {
		// NOTE: seq stays monotonic across tests on purpose. Removed ids are
		// tombstoned module-wide for the page lifetime, so reusing an id
		// string in a later test would pollute it with an earlier removal.
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('destroy-time writes after remove stay no-ops and never resurrect', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn({});
		await store.spawn({});
		expect(store.sessions.length).toBe(2);

		await store.remove(first!.id);
		expect(store.sessions.length).toBe(1);

		// TerminalInstance onDestroy finalizes after the each-block teardown.
		store.saveBuffer(first!.id, 'stale-output', 0, 12);
		store.syncSize(first!.id, 24, 80);
		expect(store.sessions.length).toBe(1);
		expect(store.byId(first!.id)).toBeUndefined();
		expect(vi.mocked(ptyService.updatePty)).not.toHaveBeenCalled();
	});

	it('concurrent double remove stays removed and tolerates the second server 404', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn({});
		expect(store.sessions.length).toBe(1);

		await Promise.all([store.remove(first!.id), store.remove(first!.id)]);
		expect(store.sessions.length).toBe(0);
		expect(store.activeId).toBeNull();
	});
});

describe('persisted-list restore windows', () => {
	beforeEach(() => {
		// NOTE: seq stays monotonic across tests on purpose (see above).
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('reopening before the debounced persist flush no longer restores removed ids', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const first = await store.spawn({});
		const second = await store.spawn({});
		await flushPersist();
		expect(store.sessions.length).toBe(2);

		// Removal only schedules persistence; storage still holds both ids.
		// The tombstone makes removed terminal anyway: any fresh store object
		// opened on this key right now drops the removed id instead of
		// rendering it again with sessions: 2.
		await store.remove(first!.id);
		expect(store.sessions.length).toBe(1);

		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.uid).not.toBe(store.uid);
		expect(reopened.sessions.map((s) => s.id)).toEqual([second!.id]);
	});

	it('reopening after the persist flush stays removed', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const first = await store.spawn({});
		const second = await store.spawn({});
		await store.remove(first!.id);
		await flushPersist();

		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.sessions.map((s) => s.id)).toEqual([second!.id]);
	});

	it('adopt cannot revive a removed id even when the server still lists it', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn({});
		await store.remove(first!.id);
		expect(store.sessions.length).toBe(0);

		// Simulate a server that still lists the session (stale generation).
		server.set(first!.id, {
			id: first!.id,
			cwd: '/test',
			title: first!.title,
			status: 'running',
			exit_code: null,
			rows: 24,
			cols: 80,
		});
		await store.adopt(first!.id);
		expect(store.sessions.length).toBe(0);
		expect(store.activeId).toBeNull();
	});

	it('a stale generation holding the id cannot exit-mark or re-sync it', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const first = await store.spawn({});
		await flushPersist();

		// A second live object on the same key restores the same ids.
		const stale = new PtySessionStore();
		stale.open(repo);
		expect(stale.sessions.map((s) => s.id)).toEqual([first!.id]);

		await store.remove(first!.id);
		server.set(first!.id, {
			id: first!.id,
			cwd: '/test',
			title: first!.title,
			status: 'exited',
			exit_code: 0,
			rows: 24,
			cols: 80,
		});

		// Late teardown writes against the stale object stay suppressed:
		// no buffer patch, no exit-mark, no meta adoption.
		stale.saveBuffer(first!.id, 'late-scrollback', 0, 16);
		expect(stale.byId(first!.id)?.buffer).toBeUndefined();
		stale.syncMetaFromServer(first!.id, {
			id: first!.id,
			cwd: '/test',
			title: first!.title,
			status: 'exited',
			exit_code: 0,
			rows: 24,
			cols: 80,
			pid: 1,
		} as PtySession);
		expect(stale.byId(first!.id)?.status).toBe('running');
		expect(stale.byId(first!.id)?.buffer).toBeUndefined();
		await stale.adopt(first!.id);
		expect(store.sessions.length).toBe(0);
	});

	it('two store objects on one key restore identical ids', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		await store.spawn({});
		await store.spawn({});
		await flushPersist();

		const first = new PtySessionStore();
		first.open(repo);
		const second = new PtySessionStore();
		second.open(repo);
		expect(first).not.toBe(second);
		expect(first.uid).not.toBe(second.uid);
		expect(second.sessions.map((s) => s.id)).toEqual(first.sessions.map((s) => s.id));
		expect(second.sessions.length).toBe(2);
	});
});
