/**
 * Latency-injected store test mirroring preview mode, where every REST call
 * costs around 30ms. It replays the exact reported sequences (single create,
 * close-the-last, reconcile after close) so a failure here points at timing
 * while a pass points at panel reactivity or the live mock server.
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
	createPty: vi.fn(async (input: { title?: string } = {}) => {
		await delay(30);
		seq += 1;
		const session: FakePty = {
			id: `pty-latency-${seq}`,
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
		server.delete(id);
	}),
	connectPty: vi.fn(() => ({
		socket: () => null,
		send: () => {},
		dispose: () => {},
	})),
}));

import { PtySessionStore, ptyStoreFor } from '#lib/stores/pty-session.svelte';

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

let repoSeq = 0;
function freshRepo(): string {
	repoSeq += 1;
	return `/latency-repo-${repoSeq}`;
}

beforeEach(async () => {
	server.clear();
	seq = 0;
	localStorage.clear();
	const { createPty } = await import('#lib/services/pty');
	vi.mocked(createPty).mockClear();
});

async function createPtyCalls(): Promise<number> {
	const { createPty } = await import('#lib/services/pty');
	return vi.mocked(createPty).mock.calls.length;
}

describe('preview-latency terminal sequences', () => {
	it('a single spawn creates exactly one session', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn({});
		expect(session).not.toBeNull();
		expect(store.sessions.length).toBe(1);
		expect(await createPtyCalls()).toBe(1);
	});

	it('closing the last session stays empty after reconcile', async () => {
		const store = ptyStoreFor(freshRepo());
		const session = await store.spawn({});
		expect(store.sessions.length).toBe(1);
		await store.remove(session!.id);
		expect(store.sessions.length).toBe(0);
		await store.reconcile();
		expect(store.sessions.length).toBe(0);
		expect(store.activeId).toBeNull();
	});

	it('keeps an in-flight spawn across a concurrent reconcile', async () => {
		const store = ptyStoreFor(freshRepo());
		const first = await store.spawn({});
		expect(first).not.toBeNull();
		const second = store.spawn({});
		await store.reconcile();
		const created = await second;
		expect(created).not.toBeNull();
		expect(store.sessions.length).toBe(2);
	});
});
