import { connectPty, getPty } from '#lib/services/pty';
import type { PtySocket } from '#lib/services/pty';
import { allPtyStores } from '#lib/stores/pty-session.svelte';
import { hasPtyInstance } from '#lib/stores/pty-instances.svelte';

const SWEEP_MS = 5_000;
const SYNC_MS = 2_000;
const MAX_TRIES = 5;
/** Viewless ceiling: detached sessions older than this lose the headless
 * socket so the server idle sweep can reclaim them instead of leaking. */
const VIEWLESS_TTL_MS = 30 * 60 * 1000;
const TERMINAL_CLOSE_CODES = new Set([4001, 4004, 4010]);

const encoder = new TextEncoder();

function byteLength(text: string): number {
	for (let i = 0; i < text.length; i += 1) {
		if (text.charCodeAt(i) > 127) return encoder.encode(text).length;
	}
	return text.length;
}

interface Keeper {
	socket: PtySocket | null;
	timer: ReturnType<typeof setTimeout> | undefined;
	syncTimer: ReturnType<typeof setTimeout> | undefined;
	tries: number;
	cursor: number;
	pendingBytes: number;
	disposed: boolean;
	viewlessSince: number;
}

const keepers = new Map<string, Keeper>();
let sweepTimer: ReturnType<typeof setInterval> | undefined = undefined;

function keeperFor(id: string): Keeper | undefined {
	return keepers.get(id);
}

function dropKeeper(id: string): void {
	const keeper = keepers.get(id);
	if (!keeper) return;
	keepers.delete(id);
	keeper.disposed = true;
	if (keeper.timer !== undefined) clearTimeout(keeper.timer);
	if (keeper.syncTimer !== undefined) clearTimeout(keeper.syncTimer);
	try {
		keeper.socket?.dispose();
	} catch {
		// Disposal never fails the sweep.
	}
	keeper.socket = null;
}

function flushCursor(id: string): void {
	const keeper = keepers.get(id);
	if (!keeper || keeper.disposed || keeper.pendingBytes <= 0) return;
	keeper.pendingBytes = 0;
	const store = allPtyStores().find((s) => s.byId(id));
	store?.syncCursor(id, keeper.cursor);
}

function scheduleSync(id: string): void {
	const keeper = keepers.get(id);
	if (!keeper || keeper.disposed) return;
	if (keeper.syncTimer !== undefined) return;
	keeper.syncTimer = setTimeout(() => {
		keeper.syncTimer = undefined;
		flushCursor(id);
	}, SYNC_MS);
}

function openKeeper(id: string): void {
	const full = allPtyStores().find((s) => s.byId(id));
	if (!full) return;
	const session = full.byId(id);
	if (!session || full.isRemoved(id) || session.status === 'exited') return;
	if (full.connectionPhase(id) === 'removed' || full.connectionPhase(id) === 'exited') return;
	if (full.connectionRetries(id) >= MAX_TRIES && full.connectionPhase(id) === 'reconnecting')
		return;

	let keeper = keepers.get(id);
	if (keeper && !keeper.disposed) return;
	keeper = {
		socket: null,
		timer: undefined,
		syncTimer: undefined,
		tries: keeperFor(id)?.tries ?? 0,
		cursor: session.cursor ?? 0,
		pendingBytes: 0,
		disposed: false,
		viewlessSince: Date.now(),
	};
	keepers.set(id, keeper);
	attachKeeper(id);
}

function attachKeeper(id: string): void {
	const keeper = keepers.get(id);
	const store = allPtyStores().find((s) => s.byId(id));
	if (!keeper || keeper.disposed || !store) return;
	const session = store.byId(id);
	if (!session || store.isRemoved(id) || session.status === 'exited') {
		dropKeeper(id);
		return;
	}
	if (hasPtyInstance(id)) {
		dropKeeper(id);
		return;
	}
	if (keeper.timer !== undefined) clearTimeout(keeper.timer);
	keeper.timer = undefined;
	try {
		keeper.socket?.dispose();
	} catch {
		// Replace stale sockets silently.
	}
	keeper.socket = null;
	const startCursor = keeper.cursor;
	const current = keeper;
	current.socket = connectPty(id, startCursor, {
		onMeta: (next) => {
			if (current.disposed) return;
			current.cursor = next;
			current.tries = 0;
			store.reportSocketOpen(id);
		},
		onData: (chunk) => {
			if (current.disposed) return;
			current.cursor += byteLength(chunk);
			current.pendingBytes += byteLength(chunk);
			scheduleSync(id);
		},
		onExit: (info) => {
			if (current.disposed) return;
			flushCursor(id);
			store.markExited(id, info.exitCode ?? undefined);
			dropKeeper(id);
		},
		onClose: (code) => {
			if (current.disposed) return;
			flushCursor(id);
			try {
				current.socket?.dispose();
			} catch {
				// Ignore disposal races.
			}
			current.socket = null;
			if (TERMINAL_CLOSE_CODES.has(code)) {
				store.markExited(id);
				dropKeeper(id);
				return;
			}
			store.reportSocketClose(id, code);
			if (current.tries >= MAX_TRIES) {
				store.markConnectFailed(id);
				dropKeeper(id);
				return;
			}
			const delay = Math.min(250 * 2 ** Math.min(current.tries, 4), 4000);
			current.tries += 1;
			current.timer = setTimeout(() => {
				current.timer = undefined;
				if (current.disposed) return;
				if (hasPtyInstance(id)) {
					dropKeeper(id);
					return;
				}
				void getPty(id).then(
					(view) => {
						if (current.disposed) return;
						if (view.status === 'exited') {
							store.markExited(id, view.exit_code ?? undefined);
							dropKeeper(id);
							return;
						}
						attachKeeper(id);
					},
					() => {
						if (current.disposed) return;
						attachKeeper(id);
					},
				);
			}, delay);
		},
	});
}

function sweep(): void {
	if (typeof window === 'undefined') return;
	const stores = allPtyStores();
	const liveIds = new Set<string>();
	for (const store of stores) {
		for (const session of store.sessions) {
			if (store.isRemoved(session.id)) continue;
			if (session.status === 'exited') {
				if (keepers.has(session.id)) dropKeeper(session.id);
				continue;
			}
			liveIds.add(session.id);
			if (hasPtyInstance(session.id)) {
				if (keepers.has(session.id)) dropKeeper(session.id);
				continue;
			}
			const existing = keepers.get(session.id);
			if (existing) {
				if (Date.now() - existing.viewlessSince > VIEWLESS_TTL_MS) dropKeeper(session.id);
				continue;
			}
			openKeeper(session.id);
		}
	}
	for (const id of [...keepers.keys()]) {
		if (!liveIds.has(id)) dropKeeper(id);
	}
}

/** Start the layout-level headless keepalive; safe to call once from the host. */
export function startPtyKeepalive(): void {
	if (typeof window === 'undefined') return;
	if (sweepTimer !== undefined) return;
	sweepTimer = setInterval(sweep, SWEEP_MS);
	sweep();
}

/** Stop the keepalive and drop every headless socket. */
export function stopPtyKeepalive(): void {
	if (sweepTimer !== undefined) clearInterval(sweepTimer);
	sweepTimer = undefined;
	for (const id of [...keepers.keys()]) dropKeeper(id);
}
