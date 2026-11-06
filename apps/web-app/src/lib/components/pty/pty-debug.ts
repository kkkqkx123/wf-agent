import { listPtys } from '#lib/services/pty';
import type { PtySessionStore } from '#lib/stores/pty-session.svelte';

export interface PtyTraceEntry {
	seq: number;
	time: string;
	event: string;
	detail?: Record<string, unknown>;
}

const TRACE_CAP = 300;
const DEBUG_QUERY = 'pty-debug';
const DEBUG_STORAGE_KEY = 'wf.pty.debug';
const PREFS_STORAGE_KEY = 'wf-ui-preferences';

let trace: PtyTraceEntry[] = [];
let seq = 0;

function readStorage(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

/** Build default: preview builds diagnose by default, formal builds stay silent unless opted in. */
function buildDebugDefault(): boolean {
	try {
		const env = (import.meta as unknown as { env?: Record<string, string | undefined> }).env;
		if (env?.VITE_PTY_DEBUG === '1') return true;
		if (env?.VITE_PTY_DEBUG === '0') return false;
		return env?.MODE === 'preview';
	} catch {
		return false;
	}
}

/** Preference track tri-state read straight from the persisted document. */
function preferenceDebugFlag(): 'on' | 'off' | null {
	const raw = readStorage(PREFS_STORAGE_KEY);
	if (!raw) return null;
	try {
		const parsed = JSON.parse(raw) as { ptyDebug?: unknown };
		if (parsed.ptyDebug === 'on') return 'on';
		if (parsed.ptyDebug === 'off') return 'off';
		return null;
	} catch {
		return null;
	}
}

export interface PtyDebugLayers {
	query: string | null;
	storage: string | null;
	preference: 'on' | 'off' | null;
	buildDefault: boolean;
	enabled: boolean;
}

/** Resolve the layered switch: query beats storage, storage beats preference, preference beats build default. */
export function describePtyDebug(): PtyDebugLayers {
	if (typeof window === 'undefined') {
		return { query: null, storage: null, preference: null, buildDefault: false, enabled: false };
	}
	let query: string | null = null;
	try {
		query = new URLSearchParams(window.location.search).get(DEBUG_QUERY);
	} catch {
		query = null;
	}
	if (query === '1') {
		return { query, storage: readStorage(DEBUG_STORAGE_KEY), preference: preferenceDebugFlag(), buildDefault: buildDebugDefault(), enabled: true };
	}
	if (query === '0') {
		return { query, storage: readStorage(DEBUG_STORAGE_KEY), preference: preferenceDebugFlag(), buildDefault: buildDebugDefault(), enabled: false };
	}
	const storage = readStorage(DEBUG_STORAGE_KEY);
	if (storage === '1') {
		return { query, storage, preference: preferenceDebugFlag(), buildDefault: buildDebugDefault(), enabled: true };
	}
	if (storage === '0') {
		return { query, storage, preference: preferenceDebugFlag(), buildDefault: buildDebugDefault(), enabled: false };
	}
	const preference = preferenceDebugFlag();
	if (preference === 'on') {
		return { query, storage, preference, buildDefault: buildDebugDefault(), enabled: true };
	}
	if (preference === 'off') {
		return { query, storage, preference, buildDefault: buildDebugDefault(), enabled: false };
	}
	const buildDefault = buildDebugDefault();
	return { query, storage, preference, buildDefault, enabled: buildDefault };
}

/** True when the operator opted into PTY diagnostics via any layer. Single choke point for all trace calls. */
export function isPtyDebugEnabled(): boolean {
	return describePtyDebug().enabled;
}

/** Append a timestamped entry; no-op unless diagnostics are enabled. */
export function ptyTrace(event: string, detail?: Record<string, unknown>): void {
	if (!isPtyDebugEnabled()) return;
	seq += 1;
	const entry: PtyTraceEntry = {
		seq,
		time: new Date().toISOString(),
		event,
		...(detail ? { detail } : {}),
	};
	trace.push(entry);
	if (trace.length > TRACE_CAP) trace = trace.slice(trace.length - TRACE_CAP);
	try {
		console.debug('[pty]', `#${entry.seq}`, entry.time, event, detail ?? '');
	} catch {
		// Console unavailable; the ring buffer still holds the entries.
	}
}

export function getPtyTrace(): PtyTraceEntry[] {
	return [...trace];
}

export function clearPtyTrace(): void {
	trace = [];
}

let errorHandlers: {
	error: (event: Event) => void;
	rejection: (event: Event) => void;
} | null = null;

/** Capture uncaught errors into the trace ring; call once when debug is on. */
export function installPtyErrorCapture(): void {
	if (typeof window === 'undefined' || errorHandlers) return;
	const onError = (event: Event): void => {
		const errorEvent = event as ErrorEvent;
		const cause = errorEvent.error as unknown;
		ptyTrace('window.error', {
			message: errorEvent.message ?? String(cause ?? event),
			stack: cause instanceof Error ? (cause.stack ?? null) : null,
		});
	};
	const onRejection = (event: Event): void => {
		const reason = (event as PromiseRejectionEvent).reason as unknown;
		ptyTrace('window.unhandledrejection', {
			message: reason instanceof Error ? reason.message : String(reason),
			stack: reason instanceof Error ? (reason.stack ?? null) : null,
		});
	};
	window.addEventListener('error', onError);
	window.addEventListener('unhandledrejection', onRejection);
	errorHandlers = { error: onError, rejection: onRejection };
}

export function uninstallPtyErrorCapture(): void {
	if (typeof window === 'undefined' || !errorHandlers) return;
	window.removeEventListener('error', errorHandlers.error);
	window.removeEventListener('unhandledrejection', errorHandlers.rejection);
	errorHandlers = null;
}

/** Snapshot the browser persistence for the active repo key. */
export function readPtyPersisted(repoKey: string): {
	sessionsRaw: string | null;
	activeRaw: string | null;
} {
	return {
		sessionsRaw: readStorage(`wf.pty.v1.${repoKey}`),
		activeRaw: readStorage(`wf.pty.v1.${repoKey}.active`),
	};
}

export interface PtyDiagResult {
	url: string;
	repoKey: string;
	localBefore: { ids: string[]; activeId: string | null };
	serverBefore: string[];
	spawnedId: string | null;
	localAfterSpawn: { ids: string[]; activeId: string | null };
	serverAfterSpawn: string[];
	localAfterRemove: { ids: string[]; activeId: string | null };
	serverAfterRemove: string[];
	serverError: string | null;
}

/**
 * Drive the live store against the active backend (preview mock or real
 * server) with one spawn/remove cycle and report both sides. A healthy run
 * shows the server and the local list moving together; divergence points at
 * the mock, while agreement with a stuck UI points at panel reactivity.
 */
export async function runPtyDiag(store: PtySessionStore): Promise<PtyDiagResult> {
	const repoKey = store.repo;
	const url = typeof window === 'undefined' ? '' : window.location.href;
	const localIds = () => store.sessions.map((s) => s.id);
	const serverIds = async (): Promise<string[]> => {
		const list = await listPtys();
		return list.map((s) => s.id);
	};
	const localBefore = { ids: localIds(), activeId: store.activeId };
	ptyTrace('diag.start', { repoKey, localBefore: localBefore.ids.join(',') });
	let serverBefore: string[] = [];
	let serverError: string | null = null;
	try {
		serverBefore = await serverIds();
	} catch (error) {
		serverError = error instanceof Error ? error.message : String(error);
	}
	ptyTrace('diag.serverBefore', { count: serverBefore.length, serverError });
	const spawned = await store.spawn({});
	const spawnedId = spawned?.id ?? null;
	ptyTrace('diag.spawned', { spawnedId });
	let serverAfterSpawn: string[] = [];
	try {
		serverAfterSpawn = await serverIds();
	} catch (error) {
		serverError = error instanceof Error ? error.message : String(error);
	}
	const localAfterSpawn = { ids: localIds(), activeId: store.activeId };
	if (spawnedId) await store.remove(spawnedId);
	const localAfterRemove = { ids: localIds(), activeId: store.activeId };
	let serverAfterRemove: string[] = [];
	try {
		serverAfterRemove = await serverIds();
	} catch (error) {
		serverError = error instanceof Error ? error.message : String(error);
	}
	ptyTrace('diag.done', {
		spawnedId,
		localAfterRemove: localAfterRemove.ids.join(','),
		serverAfterRemove: serverAfterRemove.join(','),
	});
	return {
		url,
		repoKey,
		localBefore,
		serverBefore,
		spawnedId,
		localAfterSpawn,
		serverAfterSpawn,
		localAfterRemove,
		serverAfterRemove,
		serverError,
	};
}
