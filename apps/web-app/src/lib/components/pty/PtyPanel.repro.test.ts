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
 * the template guards for removed ids.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

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
	createPty: vi.fn(
		async (
			input: { title?: string; cwd?: string; command?: string; env?: Record<string, string> } = {},
		) => {
		seq += 1;
		const session: FakePty = {
			id: `pty-repro-${seq}`,
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
	listPtys: vi.fn(async () => [...server.values()].map((s) => ({ ...s }))),
	getPty: vi.fn(async (id: string) => {
		const session = server.get(id);
		if (!session) throw new Error(`PTY session not found: ${id}`);
		return { ...session };
	}),
	updatePty: vi.fn(
		async (id: string, patch: { title?: string; rows?: number; cols?: number }) => {
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

const { PtySessionStore, ptyStoreFor } = await import('#lib/stores/pty-session.svelte');
const ptyService = await import('#lib/services/pty');

let repoSeq = 0;
function freshRepo(): string {
	repoSeq += 1;
	return `repro-store-${Date.now()}-${repoSeq}`;
}

describe('PtySessionStore terminal open/close behaviour', () => {
	beforeEach(() => {
		server.clear();
		seq = 0;
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
		server.clear();
		seq = 0;
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
			JSON.stringify([
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
