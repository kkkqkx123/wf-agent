/**
 * Store-level reproduction for the IDE terminal panel issues:
 * - closing the last terminal must empty the panel and stay empty
 * - every spawn call must create exactly one terminal, including concurrent clicks
 * - reconcile must not prune sessions created while its fetch is in flight
 * - a fresh repo without any stored record is distinguishable from a
 *   user-closed-all repo with a persisted empty list
 *
 * The previous component mount harness cannot run under vitest SSR
 * compilation, so this file drives PtySessionStore directly with a fake
 * PTY service. Panel behavior follows from these store guarantees plus
 * the template guards for removed ids. Also covers the detach and attach
 * transfers, foreign store discovery, persisted payload hardening,
 * explicit connection phases and headless cursor sync.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

interface FakePty {
	id: string;
	cwd: string;
	title: string;
	title_source: 'default' | 'auto' | 'manual';
	status: 'running' | 'exited';
	exit_code: number | null;
	rows: number;
	cols: number;
	created_at: number;
	last_activity: number;
}

const server = new Map<string, FakePty>();
let seq = 0;

function delay(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

vi.mock('#lib/services/pty', () => ({
	createPty: vi.fn(
		async (
			input: { title?: string; cwd?: string; command?: string; env?: Record<string, string> } = {},
		) => {
			seq += 1;
			const session: FakePty = {
				id: `pty-repro-${seq}`,
				cwd: '/test',
				title: input.title ?? `Terminal ${seq}`,
				title_source: 'default',
				status: 'running',
				exit_code: null,
				rows: 24,
				cols: 80,
				created_at: Date.now(),
				last_activity: Date.now(),
			};
			server.set(session.id, { ...session });
			return { ...session };
		},
	),
	listPtys: vi.fn(async () => [...server.values()].map((s) => ({ ...s }))),
	getPty: vi.fn(async (id: string) => {
		const session = server.get(id);
		if (!session) throw new Error(`PTY session not found: ${id}`);
		return { ...session };
	}),
	updatePty: vi.fn(async (
			id: string,
			patch: { title?: string; title_source?: FakePty['title_source']; rows?: number; cols?: number },
		) => {
		const session = server.get(id);
		if (!session) throw new Error(`PTY session not found: ${id}`);
		const next = {
			...session,
			title:
								patch.title !== undefined &&
								(patch.title_source !== 'auto' || session.title_source !== 'manual')
									? patch.title
									: session.title,
			title_source:
								patch.title !== undefined &&
								(patch.title_source !== 'auto' || session.title_source !== 'manual')
									? (patch.title_source ?? 'manual')
									: session.title_source,
			rows: patch.rows ?? session.rows,
			cols: patch.cols ?? session.cols,
			last_activity: Date.now(),
		};
		server.set(id, next);
		return { ...next };
	}),
	closePty: vi.fn(async (id: string) => {
		server.delete(id);
	}),
	connectPty: vi.fn(() => ({
		socket: () => null,
		send: () => {},
		dispose: () => {},
	})),
	createTerminalWriter: vi.fn(),
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
		key: (index: number): string | null => [...data.keys()][index] ?? null,
		get length(): number {
			return data.size;
		},
	} as Storage;
	Object.defineProperty(globalThis, 'localStorage', {
		value: storage,
		configurable: true,
		writable: true,
	});
}

installLocalStorage();

/** Wrap session records in the store persistence envelope. */
function envelope(sessions: unknown[], detachedIds: string[] = []): unknown {
	return { v: 1, sessions, detachedIds };
}

const { PtySessionStore, discoverForeignPtyStores, ptyStoreFor } =
	await import('#lib/stores/pty-session.svelte');
const ptyService = await import('#lib/services/pty');

let repoSeq = 0;
function freshRepo(): string {
	repoSeq += 1;
	return `repro-store-${Date.now()}-${repoSeq}`;
}

describe('PtySessionStore terminal open/close behaviour', () => {
	beforeEach(() => {
		// NOTE: seq stays monotonic on purpose. Removed ids are tombstoned
		// module-wide, so reusing an id string would pollute later tests.
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('creates exactly one terminal per sequential spawn', async () => {
		const store = ptyStoreFor(freshRepo());
		expect(store.sessions.length).toBe(0);
		expect(store.hasStoredRecord).toBe(false);

		const first = await store.spawn();
		expect(first).not.toBeNull();
		expect(store.sessions.length).toBe(1);

		const second = await store.spawn();
		expect(second).not.toBeNull();
		expect(store.sessions.length).toBe(2);
		expect(first?.id).not.toBe(second?.id);
		expect(store.activeId).toBe(second?.id);
	});

	it('gives concurrent spawns distinct sessions instead of swallowing one', async () => {
		const store = ptyStoreFor(freshRepo());
		const [first, second] = await Promise.all([store.spawn(), store.spawn()]);
		expect(first).not.toBeNull();
		expect(second).not.toBeNull();
		expect(store.sessions.length).toBe(2);
		const numbers = store.sessions.map((s) => s.titleNumber);
		expect(new Set(numbers).size).toBe(2);
	});

	it('closing the last terminal empties the store and stays empty', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const session = await store.spawn();
		expect(store.sessions.length).toBe(1);

		await store.remove(session!.id);
		expect(store.sessions.length).toBe(0);
		expect(store.activeId).toBeNull();

		store.saveBuffer(session!.id, 'stale-output', 0, 0);
		store.syncSize(session!.id, 24, 80);
		expect(store.sessions.length).toBe(0);
		expect(vi.mocked(ptyService.updatePty)).not.toHaveBeenCalled();

		await store.reconcile();
		expect(store.sessions.length).toBe(0);

		await delay(5);
		expect(store.sessions.length).toBe(0);
	});

	it('does not prune sessions created while reconcile fetch is in flight', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn();
		expect(store.sessions.length).toBe(1);

		vi.mocked(ptyService.listPtys).mockImplementationOnce(async () => {
			const stale = [...server.values()].map((s) => ({ ...s }));
			expect(stale.length).toBe(1);
			await delay(20);
			return stale;
		});

		const reconciling = store.reconcile();
		const second = await store.spawn();
		expect(second).not.toBeNull();
		expect(second?.id).not.toBe(first?.id);
		await reconciling;

		expect(store.sessions.length).toBe(2);
		expect(store.byId(first!.id)).toBeDefined();
		expect(store.byId(second!.id)).toBeDefined();
	});

	it('distinguishes fresh repos from user-closed-all persisted empty lists', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		expect(store.hasStoredRecord).toBe(false);

		const session = await store.spawn();
		await store.remove(session!.id);
		store.close();

		const remount = new PtySessionStore();
		remount.open(repo);
		expect(remount.sessions.length).toBe(0);
		expect(remount.hasStoredRecord).toBe(true);
	});

	it('reuses the stored origin when reviving without explicit options', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn({
			cwd: '/repo/sub',
			command: 'echo hi',
			env: { PREVIEW_MODE: '1' },
		});
		expect(first?.origin).toEqual({
			cwd: '/repo/sub',
			command: 'echo hi',
			env: { PREVIEW_MODE: '1' },
		});
		vi.mocked(ptyService.createPty).mockClear();
		await store.clone(first!.id);
		expect(store.sessions.length).toBe(1);
		expect(vi.mocked(ptyService.createPty)).toHaveBeenCalledTimes(1);
		expect(vi.mocked(ptyService.createPty)).toHaveBeenCalledWith(
			expect.objectContaining({
				cwd: '/repo/sub',
				command: 'echo hi',
				env: { PREVIEW_MODE: '1' },
			}),
		);
	});
});

describe('PtySessionStore lifecycle and persistence validation', () => {
	beforeEach(() => {
		// Same monotonic-seq rule as above: never reuse an id string.
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('classifies live, exited and missing sessions explicitly', async () => {
		const store = ptyStoreFor(freshRepo());
		expect(store.hasLive()).toBe(false);
		expect(store.allExited()).toBe(false);
		expect(store.lifecycle('nope')).toBe('missing');

		const session = await store.spawn();
		expect(store.lifecycle(session!.id)).toBe('live');
		expect(store.hasLive()).toBe(true);

		store.patch(session!.id, { status: 'exited' });
		expect(store.lifecycle(session!.id)).toBe('exited');
		expect(store.hasLive()).toBe(false);
		expect(store.allExited()).toBe(true);
	});

	it('drops unusable persisted records and validates every field', () => {
		const repo = freshRepo();
		const big = 'x'.repeat(300_000);
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(
				envelope([
					{
						id: 'good',
						title: 'Terminal 1',
						titleNumber: 1,
						buffer: 'hi',
						scrollY: 3,
						cursor: 2,
						rows: 24,
						cols: 80,
						status: 'running',
						origin: { cwd: '/r' },
						extra: 'dropped',
					},
					{
						id: 'bad-shape',
						title: 'Terminal 2',
						rows: 'wide',
						cols: -1,
						scrollY: -5,
						cursor: -1,
						status: 'ghost',
						exitCode: 'x',
						origin: { env: { A: 1, B: 'ok' } },
					},
					{ id: 'huge', title: 'Terminal 3', buffer: big, scrollY: 100, cursor: 300_000 },
					{ title: 'missing id' },
					{ id: 'good', title: 'duplicate' },
				]),
			),
		);
		const store = new PtySessionStore();
		store.open(repo);
		expect(store.sessions.map((s) => s.id)).toEqual(['good', 'bad-shape', 'huge']);
		const good = store.byId('good');
		expect(good?.origin).toEqual({ cwd: '/r' });
		expect((good as unknown as Record<string, unknown>).extra).toBeUndefined();
		const bad = store.byId('bad-shape');
		expect(bad?.rows).toBeUndefined();
		expect(bad?.cols).toBeUndefined();
		expect(bad?.scrollY).toBeUndefined();
		expect(bad?.cursor).toBeUndefined();
		expect(bad?.status).toBeUndefined();
		expect(bad?.exitCode).toBeUndefined();
		expect(bad?.origin).toEqual({ env: { B: 'ok' } });
		const huge = store.byId('huge');
		expect(huge?.buffer?.length).toBeLessThan(big.length);
		expect(huge?.scrollY).toBe(0);
	});
});
describe('persisted payload hardening', () => {
	beforeEach(() => {
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('rejects future schema versions and truncates overlong titles', () => {
		const repo = freshRepo();
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(
				envelope([
					{ id: 'future', title: 'Terminal 1', v: 999 },
					{ id: 'long', title: `T${'x'.repeat(500)}` },
				]),
			),
		);
		const store = new PtySessionStore();
		store.open(repo);
		expect(store.sessions.map((s) => s.id)).toEqual(['long']);
		expect(store.byId('long')?.title.length).toBeLessThanOrEqual(120);
	});

	it('caps env vars, drops non-string entries and overlong values', () => {
		const repo = freshRepo();
		const env: Record<string, unknown> = { KEEP: 'yes', DROP: 42 };
		for (let i = 0; i < 100; i += 1) env[`K${i}`] = 'v';
		env.BIG = 'z'.repeat(10_000);
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(envelope([{ id: 'env', title: 'Terminal 1', origin: { cwd: '/r', env } }])),
		);
		const store = new PtySessionStore();
		store.open(repo);
		const origin = store.byId('env')?.origin?.env ?? {};
		expect(origin.KEEP).toBe('yes');
		expect(origin.DROP).toBeUndefined();
		expect(Object.keys(origin).length).toBeLessThanOrEqual(64);
		expect((origin.BIG ?? '').length).toBeLessThanOrEqual(4096);
	});

	it('drops out-of-range cursors and exit codes instead of persisting them', () => {
		const repo = freshRepo();
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(
				envelope([
					{ id: 'a', title: 'Terminal 1', cursor: -1, exitCode: 'x' },
					{ id: 'b', title: 'Terminal 2', cursor: 2 ** 40, exitCode: 999_999_999 },
				]),
			),
		);
		const store = new PtySessionStore();
		store.open(repo);
		expect(store.byId('a')?.cursor).toBeUndefined();
		expect(store.byId('a')?.exitCode).toBeUndefined();
		expect(store.byId('b')?.cursor).toBeUndefined();
		expect(store.byId('b')?.exitCode).toBeUndefined();
	});

	it('sheds scrollback tails before dropping tabs under the total cap', () => {
		const repo = freshRepo();
		const big = 'y'.repeat(700_000);
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(
				envelope([
					{ id: 'one', title: 'Terminal 1', buffer: big, cursor: 1 },
					{ id: 'two', title: 'Terminal 2', buffer: big, cursor: 2 },
				]),
			),
		);
		const store = new PtySessionStore();
		store.open(repo);
		expect(store.sessions.length).toBeGreaterThanOrEqual(1);
		for (const s of store.sessions) {
			expect((s.buffer ?? '').length).toBeLessThanOrEqual(64 * 1024);
		}
	});
});

describe('explicit connection phases', () => {
	beforeEach(() => {
		// NOTE: seq stays monotonic on purpose. Removed ids are tombstoned
		// module-wide, so reusing an id string would pollute later tests.
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('moves live to reconnecting to exited through store reports', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		expect(store.connectionPhase(id)).toBe('live');

		store.reportSocketOpen(id);
		expect(store.connectionPhase(id)).toBe('live');
		expect(store.connectionRetries(id)).toBe(0);

		store.reportSocketClose(id, 1006);
		expect(store.connectionPhase(id)).toBe('reconnecting');
		expect(store.connectionRetries(id)).toBe(1);

		store.reportSocketOpen(id);
		expect(store.connectionPhase(id)).toBe('live');
		expect(store.connectionRetries(id)).toBe(0);

		store.reportSocketClose(id, 4010);
		expect(store.connectionPhase(id)).toBe('exited');
		expect(store.byId(id)?.status).toBe('exited');
	});

	it('marks terminal close codes exited without a failure banner retry', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		for (const code of [4001, 4004, 4010]) {
			store.patch(id, { status: 'running' });
			store.reportSocketClose(id, code);
			expect(store.connectionPhase(id)).toBe('exited');
		}
	});

	it('removed stays terminal for every phase query', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		await store.remove(id);
		expect(store.connectionPhase(id)).toBe('removed');
		store.reportSocketOpen(id);
		store.markExited(id);
		expect(store.connectionPhase(id)).toBe('removed');
		expect(store.byId(id)).toBeUndefined();
	});
});

describe('detach and attach transfers', () => {
	beforeEach(() => {
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('detaches without killing and reattaches as active', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn();
		const second = await store.spawn();
		expect(store.activeId).toBe(second!.id);

		expect(store.detach(second!.id)).toBe(true);
		expect(store.isDetached(second!.id)).toBe(true);
		expect(store.attachedSessions().map((s) => s.id)).toEqual([first!.id]);
		expect(store.activeId).toBe(first!.id);
		expect(store.hasAttachedLive()).toBe(true);

		expect(store.detach(second!.id)).toBe(false);

		expect(store.detach(first!.id)).toBe(true);
		expect(store.attachedSessions().length).toBe(0);
		expect(store.activeId).toBeNull();
		expect(store.hasAttachedLive()).toBe(false);

		expect(store.attach(first!.id)).toBe(true);
		expect(store.isDetached(first!.id)).toBe(false);
		expect(store.activeId).toBe(first!.id);
		expect(store.attach(first!.id)).toBe(false);
	});

	it('keeps the hidden state across teardown writes and server re-syncs', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn();
		const second = await store.spawn();
		expect(store.detach(second!.id)).toBe(true);

		// TerminalInstance teardown serializes scrollback after the tab hides.
		store.saveBuffer(second!.id, 'scrollback', 0, 11);
		// Late socket metadata re-syncs only viewport and lifecycle fields.
		const view = await ptyService.getPty(second!.id);
		store.syncMetaFromServer(second!.id, view);
		store.syncCursor(second!.id, 11);
		store.syncSize(first!.id, 30, 100);

		expect(store.isDetached(second!.id)).toBe(true);
		expect(store.attachedSessions().map((s) => s.id)).toEqual([first!.id]);
		expect(store.detach(second!.id)).toBe(false);
	});

	it('carries the hidden state across eject and ingest without reshowing', async () => {
		const source = ptyStoreFor(freshRepo());
		const session = await source.spawn();
		expect(source.detach(session!.id)).toBe(true);
		const ejected = source.eject(session!.id);
		expect(ejected?.detached).toBe(true);

		const target = ptyStoreFor(freshRepo());
		expect(target.ingest(ejected!.record, ejected!.detached)).toBe(true);
		expect(target.isDetached(session!.id)).toBe(true);
		expect(target.attachedSessions().length).toBe(0);

		// Re-ingesting a present id only focuses it, never reshows it.
		expect(target.ingest(ejected!.record)).toBe(true);
		expect(target.isDetached(session!.id)).toBe(true);
		expect(target.activeId).toBe(session!.id);
	});

	it('ejects without killing or tombstoning so another store can ingest', async () => {
		const source = ptyStoreFor(freshRepo());
		const session = await store_spawn(source);
		const ejected = source.eject(session.id);
		expect(ejected).not.toBeNull();
		expect(source.byId(session.id)).toBeUndefined();

		const target = ptyStoreFor(freshRepo());
		expect(target.ingest(ejected!.record, ejected!.detached)).toBe(true);
		expect(target.byId(session.id)?.title).toBe(session.title);
		expect(target.activeId).toBe(session.id);
		// No tombstone: the moved id stays fully usable in its new home.
		target.syncCursor(session.id, 7);
		expect(target.byId(session.id)?.cursor).toBe(7);
		await target.adopt(session.id);
		expect(target.byId(session.id)).toBeDefined();

		async function store_spawn(s: typeof source) {
			const created = await s.spawn();
			if (!created) throw new Error('spawn failed');
			return created;
		}
	});

	it('refuses ingest when the target store is full and keeps the record', async () => {
		const source = ptyStoreFor(freshRepo());
		const session = await source.spawn();
		const ejected = source.eject(session!.id);
		expect(ejected).not.toBeNull();

		const target = ptyStoreFor(freshRepo());
		for (let i = 0; i < 20; i += 1) {
			await target.spawn();
		}
		expect(target.sessions.length).toBe(20);
		expect(target.ingest(ejected!.record, ejected!.detached)).toBe(false);
		expect(source.ingest(ejected!.record, ejected!.detached)).toBe(true);
	});

	it('persists the detached flag and restores it on reopen', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const session = await store.spawn();
		store.detach(session!.id);
		store.close();

		await new Promise((resolve) => setTimeout(resolve, 400));
		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.isDetached(session!.id)).toBe(true);
		expect(reopened.attachedSessions().length).toBe(0);
	});

	it('ignores hidden ids that have no restored record on reopen', () => {
		const repo = freshRepo();
		localStorage.setItem(
			`wf.pty.v1.${repo}`,
			JSON.stringify(envelope([{ id: 'solo', title: 'Terminal 1' }], ['solo', 'ghost'])),
		);
		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.isDetached('solo')).toBe(true);
		expect(reopened.isDetached('ghost')).toBe(false);
	});
});

describe('foreign store discovery', () => {
	beforeEach(() => {
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('opens persisted-but-unmounted stores and honors tombstones', async () => {
		const current = ptyStoreFor(freshRepo());
		const victim = await current.spawn();
		await current.remove(victim!.id);

		const foreignRepo = `foreign-discover-${Date.now()}`;
		localStorage.setItem(
			`wf.pty.v1.${foreignRepo}`,
			JSON.stringify(
				envelope([
					{ id: victim!.id, title: 'killed before' },
					{ id: 'foreign-live-1', title: 'Terminal 1' },
				]),
			),
		);
		localStorage.setItem(`wf.pty.v1.${foreignRepo}.active`, victim!.id);

		const found = discoverForeignPtyStores(current);
		expect(found.length).toBe(1);
		expect(found[0].sessions.map((s) => s.id)).toEqual(['foreign-live-1']);
	});

	it('caps the number of opened keys', () => {
		const current = ptyStoreFor(freshRepo());
		const stamp = Date.now();
		for (let i = 0; i < 12; i += 1) {
			localStorage.setItem(
				`wf.pty.v1.cap-${stamp}-${i}`,
				JSON.stringify(envelope([{ id: `cap-${stamp}-${i}`, title: 'Terminal 1' }])),
			);
		}
		const found = discoverForeignPtyStores(current);
		expect(found.length).toBeLessThanOrEqual(10);
	});
});

describe('headless cursor sync', () => {
	beforeEach(() => {
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('only advances the cursor and ignores removed ids', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		store.syncCursor(id, 10);
		expect(store.byId(id)?.cursor).toBe(10);
		store.syncCursor(id, 5);
		expect(store.byId(id)?.cursor).toBe(10);
		await store.remove(id);
		store.syncCursor(id, 99);
		expect(store.byId(id)).toBeUndefined();
	});
});

describe('shared visibility, titles and activity', () => {
	beforeEach(() => {
		server.clear();
		localStorage.clear();
		vi.clearAllMocks();
	});

	it('shares one store per repo key so both panels see the same tabs', async () => {
		const repo = freshRepo();
		const idePanel = ptyStoreFor(repo);
		const terminalPage = ptyStoreFor(repo);
		expect(terminalPage).toBe(idePanel);
		const session = await idePanel.spawn();
		expect(terminalPage.byId(session!.id)).toBeDefined();
		expect(idePanel.detach(session!.id)).toBe(true);
		expect(terminalPage.attachedSessions().length).toBe(0);
		expect(terminalPage.attach(session!.id)).toBe(true);
		expect(idePanel.isDetached(session!.id)).toBe(false);
	});

	it('accepts process titles until the user renames, then ignores them', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		store.adoptAutoTitle(id, 'vim foo.rs');
		await vi.waitFor(() => expect(store.byId(id)?.title).toBe('vim foo.rs'));
		store.rename(id, 'my service');
		await vi.waitFor(() => expect(server.get(id)?.title).toBe('my service'));
		expect(store.byId(id)?.manualTitle).toBe(true);
		store.adoptAutoTitle(id, 'less bar');
		expect(store.byId(id)?.title).toBe('my service');
		store.clearManualTitle(id);
		expect(store.byId(id)?.manualTitle).toBeUndefined();
		store.adoptAutoTitle(id, 'less bar');
		expect(store.byId(id)?.title).toBe('less bar');
	});

	it('treats fresh sessions as busy and quiet ones as idle estimates', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn();
		const id = session!.id;
		expect(store.isBusy(id)).toBe(true);
		store.reportInput(id);
		expect(store.lastActiveAt(id)).toBeGreaterThan(0);
		store.reportOutput(id);
		expect(store.isBusy(id, store.lastActiveAt(id) + 1)).toBe(true);
		expect(store.isBusy(id, store.lastActiveAt(id) + 60_000)).toBe(false);
		store.markExited(id);
		expect(store.isBusy(id)).toBe(false);
	});

	it('closes every exited session in one pass for the manager cleanup', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn();
		const second = await store.spawn();
		store.markExited(first!.id);
		const exited = store.sessions.filter((s) => s.status === 'exited').map((s) => s.id);
		expect(exited).toEqual([first!.id]);
		for (const id of exited) await store.remove(id);
		expect(store.sessions.map((s) => s.id)).toEqual([second!.id]);
	});

	it('keeps the record but drops a non-boolean manual flag from storage', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const session = await store.spawn();
		store.close();
		await new Promise((resolve) => setTimeout(resolve, 400));
		const raw = JSON.parse(localStorage.getItem(`wf.pty.v1.${repo}`) ?? '{}') as {
			sessions: Array<Record<string, unknown>>;
		};
		raw.sessions[0]!['manualTitle'] = 'yes';
		localStorage.setItem(`wf.pty.v1.${repo}`, JSON.stringify({ v: 1, sessions: raw.sessions, detachedIds: [] }));
		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.byId(session!.id)?.manualTitle).toBeUndefined();
		expect(reopened.byId(session!.id)?.title).toBe(session!.title);
	});

	it('restores the manual flag and survives a server title race', async () => {
		const repo = freshRepo();
		const store = ptyStoreFor(repo);
		const session = await store.spawn();
		store.rename(session!.id, 'my service');
		await vi.waitFor(() => expect(server.get(session!.id)?.title).toBe('my service'));
		store.close();
		await new Promise((resolve) => setTimeout(resolve, 400));
		const reopened = new PtySessionStore();
		reopened.open(repo);
		expect(reopened.byId(session!.id)?.manualTitle).toBe(true);
		await reopened.adopt(session!.id);
		expect(reopened.byId(session!.id)?.title).toBe('my service');
	});
});
