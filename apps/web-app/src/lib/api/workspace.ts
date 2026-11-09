/**
 * Per-tab workspace identity: directory resolution, recent history and the
 * browser tab label.
 *
 * A server process serves many business directories, so the directory is a
 * per-tab choice, never a shared global. Resolution order is build-time pin,
 * then the address-bar `?directory=` parameter (shareable across tabs),
 * then the per-tab `sessionStorage` entry, then the legacy `localStorage`
 * entry which is adopted once and removed. `localStorage` only keeps the
 * recent-directory history used by pickers; it never decides the active
 * directory, so two tabs can pin different workspaces without overwriting
 * each other.
 *
 * Changing the directory updates the address bar in place and notifies
 * listeners through a window event. Mounted pages remount on that event
 * instead of reloading the browser tab, which drops in-flight REST calls
 * and reopens SSE/WebSocket streams under the new directory.
 */

export const DIRECTORY_PARAM = 'directory';
export const TAB_LABEL_PARAM = 'label';
export const HOST_PARAM = 'host';

const SESSION_DIRECTORY_KEY = 'wf.directory';
const LEGACY_DIRECTORY_KEY = 'wf.directory';
const SESSION_TAB_KEY = 'wf.tabName';
const SESSION_HOST_KEY = 'wf.host';
const RECENT_KEY = 'wf.directories.recent';
const MAX_RECENT = 10;

const DIRECTORY_EVENT = 'wf:directory-changed';
const LABEL_EVENT = 'wf:workspace-label-changed';

/** One entry of the cross-tab recent-directory history. */
export interface RecentDirectory {
	directory: string;
	lastUsed: number;
}

function browser(): boolean {
	return typeof window !== 'undefined' && typeof window.location !== 'undefined';
}

function readUrlParam(key: string): string | undefined {
	if (!browser()) return undefined;
	try {
		const value = new URLSearchParams(window.location.search).get(key)?.trim();
		return value ? value : undefined;
	} catch {
		return undefined;
	}
}

function readSession(key: string): string | undefined {
	try {
		if (typeof sessionStorage === 'undefined') return undefined;
		const value = sessionStorage.getItem(key)?.trim();
		return value ? value : undefined;
	} catch {
		return undefined;
	}
}

function writeSession(key: string, value: string | undefined): void {
	try {
		if (typeof sessionStorage === 'undefined') return;
		if (value === undefined || value.trim() === '') sessionStorage.removeItem(key);
		else sessionStorage.setItem(key, value.trim());
	} catch {
		// Per-tab state is best-effort; the URL parameter still carries it.
	}
}

function readLegacyDirectory(): string | undefined {
	try {
		if (typeof localStorage === 'undefined') return undefined;
		const value = localStorage.getItem(LEGACY_DIRECTORY_KEY)?.trim();
		return value ? value : undefined;
	} catch {
		return undefined;
	}
}

/** Build-time pin for single-machine deployments. Wins over every runtime source. */
export function pinnedDirectory(): string | undefined {
	try {
		const raw = import.meta.env.VITE_API_DIRECTORY as string | undefined;
		const trimmed = raw?.trim();
		return trimmed ? trimmed : undefined;
	} catch {
		return undefined;
	}
}

function buildTimeHost(): string | undefined {
	try {
		const raw = import.meta.env.VITE_WORKSPACE_HOST as string | undefined;
		const trimmed = raw?.trim();
		return trimmed ? trimmed : undefined;
	} catch {
		return undefined;
	}
}

/**
 * Resolve the business directory for this tab.
 * Build-time value wins, otherwise the address bar, otherwise the per-tab
 * session entry, otherwise the legacy global entry (adopted into the
 * session on first read). Absent means directory-less mode: workspace-bound
 * handlers reject requests instead of guessing.
 */
export function resolveDirectory(): string | undefined {
	const pinned = pinnedDirectory();
	if (pinned) return pinned;
	const fromUrl = readUrlParam(DIRECTORY_PARAM);
	if (fromUrl) {
		adoptIntoSession(SESSION_DIRECTORY_KEY, fromUrl);
		return fromUrl;
	}
	const fromSession = readSession(SESSION_DIRECTORY_KEY);
	if (fromSession) return fromSession;
	const legacy = readLegacyDirectory();
	if (legacy) {
		adoptIntoSession(SESSION_DIRECTORY_KEY, legacy);
		try {
			if (typeof localStorage !== 'undefined') localStorage.removeItem(LEGACY_DIRECTORY_KEY);
		} catch {
			// The legacy entry stays; it is only a fallback and never wins over session.
		}
		return legacy;
	}
	return undefined;
}

function adoptIntoSession(key: string, value: string): void {
	try {
		if (typeof sessionStorage === 'undefined') return;
		if (!sessionStorage.getItem(key)) sessionStorage.setItem(key, value);
	} catch {
		// Adoption is best-effort.
	}
}

/**
 * Validate a workspace directory spelling before persisting it: it must
 * be a non-empty absolute path. Server membership (allowlist, existence)
 * is checked by `fetchServerDirectories`, not here.
 */
export function validateDirectoryFormat(raw: string): string | undefined {
	const trimmed = raw.trim();
	if (trimmed.length === 0) return 'directory must not be empty';
	if (!trimmed.startsWith('/')) return 'directory must be absolute';
	return undefined;
}

/** Trim and drop trailing slashes (keeping the filesystem root intact). */
export function normalizeDirectory(raw: string): string {
	const trimmed = raw.trim().replace(/\/+$/, '');
	return trimmed === '' ? '/' : trimmed;
}

function syncUrlParam(key: string, value: string | undefined): void {
	if (!browser()) return;
	try {
		const url = new URL(window.location.href);
		if (value === undefined || value === '') url.searchParams.delete(key);
		else url.searchParams.set(key, value);
		const next = `${url.pathname}${url.search ? `?${url.searchParams.toString()}` : ''}${url.hash}`;
		if (next !== `${url.pathname}${window.location.search}${url.hash}`) {
			window.history.replaceState(null, '', next);
		}
	} catch {
		// The session entry already holds the value; the URL is shareability only.
	}
}

function dispatch(target: string, detail: string | undefined): void {
	if (typeof window === 'undefined') return;
	try {
		window.dispatchEvent(new CustomEvent<string | undefined>(target, { detail }));
	} catch {
		// Listeners are optional; the stored value is the source of truth.
	}
}

/**
 * Persist the workspace directory for this tab after format validation.
 * Updates the session entry and the address bar, records the recent
 * history and notifies listeners so pages remount under the new
 * directory. Empty clears back to directory-less mode. Throws on invalid
 * spelling or when the build pins the directory, so callers surface the
 * problem before any request fails.
 */
export function setDirectory(raw: string | undefined): string | undefined {
	if (pinnedDirectory()) {
		throw new Error('directory is pinned by the server build and cannot be switched');
	}
	const trimmed = (raw ?? '').trim();
	if (trimmed.length === 0) {
		writeSession(SESSION_DIRECTORY_KEY, undefined);
		syncUrlParam(DIRECTORY_PARAM, undefined);
		applyDocumentTitle();
		dispatch(DIRECTORY_EVENT, undefined);
		return undefined;
	}
	const problem = validateDirectoryFormat(trimmed);
	if (problem) throw new Error(problem);
	const next = normalizeDirectory(trimmed);
	writeSession(SESSION_DIRECTORY_KEY, next);
	syncUrlParam(DIRECTORY_PARAM, next);
	pushRecentDirectory(next);
	applyDocumentTitle();
	dispatch(DIRECTORY_EVENT, next);
	return next;
}

/** Subscribe to in-tab directory switches. Returns an unsubscribe function. */
export function onDirectoryChange(listener: (directory: string | undefined) => void): () => void {
	if (typeof window === 'undefined') return () => {};
	const handler = (event: Event): void => {
		listener((event as CustomEvent<string | undefined>).detail);
	};
	window.addEventListener(DIRECTORY_EVENT, handler);
	return () => window.removeEventListener(DIRECTORY_EVENT, handler);
}

/** Subscribe to tab-label/host changes (title only, no page remount). */
export function onWorkspaceLabelChange(listener: () => void): () => void {
	if (typeof window === 'undefined') return () => {};
	const handler = (): void => listener();
	window.addEventListener(LABEL_EVENT, handler);
	return () => window.removeEventListener(LABEL_EVENT, handler);
}

/** Read the cross-tab recent-directory history, newest first. */
export function readRecentDirectories(): RecentDirectory[] {
	try {
		if (typeof localStorage === 'undefined') return [];
		const raw = localStorage.getItem(RECENT_KEY);
		if (!raw) return [];
		const parsed = JSON.parse(raw) as unknown;
		if (!Array.isArray(parsed)) return [];
		const out: RecentDirectory[] = [];
		for (const entry of parsed) {
			if (typeof entry !== 'object' || entry === null) continue;
			const record = entry as Record<string, unknown>;
			if (typeof record.directory !== 'string') continue;
			const directory = record.directory.trim();
			if (!directory.startsWith('/')) continue;
			const lastUsed =
				typeof record.lastUsed === 'number' && Number.isFinite(record.lastUsed)
					? record.lastUsed
					: 0;
			out.push({ directory, lastUsed });
			if (out.length >= MAX_RECENT) break;
		}
		return out.sort((a, b) => b.lastUsed - a.lastUsed);
	} catch {
		return [];
	}
}

/** Merge one directory into a recent list, newest first, capped. Pure for tests. */
export function mergeRecent(
	list: RecentDirectory[],
	directory: string,
	now: number,
): RecentDirectory[] {
	const seen = new Set<string>();
	const out: RecentDirectory[] = [{ directory, lastUsed: now }];
	seen.add(directory);
	for (const entry of list) {
		if (seen.has(entry.directory)) continue;
		seen.add(entry.directory);
		out.push(entry);
		if (out.length >= MAX_RECENT) break;
	}
	return out;
}

/** Record one directory in the cross-tab recent history. Never throws. */
export function pushRecentDirectory(directory: string): void {
	try {
		if (typeof localStorage === 'undefined') return;
		const next = mergeRecent(readRecentDirectories(), directory, Date.now());
		localStorage.setItem(RECENT_KEY, JSON.stringify(next));
	} catch {
		// History is best-effort.
	}
}

/**
 * Stable per-directory namespace suffix for browser storage keys.
 * Empty for directory-less mode so the previous single-workspace keys keep
 * working; otherwise a short hash keeps tab state (IDE tabs, drafts)
 * isolated between workspaces sharing one browser profile.
 */
export function directoryNamespace(directory: string | undefined): string {
	const normalized = (directory ?? '').trim().replace(/\/+$/, '');
	if (!normalized) return '';
	let hash = 0x811c9dc5;
	for (let i = 0; i < normalized.length; i += 1) {
		hash ^= normalized.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193);
	}
	return `.${(hash >>> 0).toString(16).padStart(8, '0')}`;
}

const SEGMENT_BUDGET = 20;

/** Shorten one path segment so overlong names cannot blow up the tab bar. */
export function shortenSegment(segment: string): string {
	const trimmed = segment.trim();
	if (trimmed.length <= SEGMENT_BUDGET) return trimmed;
	return `${trimmed.slice(0, SEGMENT_BUDGET - 1)}…`;
}

/**
 * Short workspace name for the tab bar: the last two path levels
 * (`code/wf-agent`), falling back to the single level for shallow roots.
 * Full server paths are long and leak layout; the tail carries the project
 * identity while staying bounded.
 */
export function shortWorkspaceName(directory: string | undefined): string {
	const trimmed = (directory ?? '').trim();
	if (!trimmed) return '';
	const normalized = trimmed.replace(/\/+$/, '');
	if (!normalized) return '/';
	const parts = normalized.split('/').filter((part) => part.length > 0);
	if (parts.length === 0) return '/';
	const tail = parts.slice(-2).map(shortenSegment);
	return tail.join('/');
}

/**
 * Shorten a host/distro label to a tab-bar prefix (`archlinux` becomes
 * `arch`). Lowercase alphanumerics only, first four characters: long
 * enough to tell machines apart, short enough to leave room for the
 * project name.
 */
export function shortHostPrefix(host: string | undefined): string {
	const cleaned = (host ?? '').toLowerCase().replace(/[^a-z0-9]/g, '');
	return cleaned.slice(0, 4);
}

/** Explicit per-tab title override: address bar wins over the sticky session entry. */
export function resolveTabName(): string | undefined {
	return readUrlParam(TAB_LABEL_PARAM) ?? readSession(SESSION_TAB_KEY);
}

/** Persist an explicit per-tab title. Empty clears the override back to the derived name. */
export function setTabName(raw: string | undefined): string | undefined {
	const trimmed = (raw ?? '').trim();
	const next = trimmed ? trimmed.slice(0, 60) : undefined;
	writeSession(SESSION_TAB_KEY, next);
	syncUrlParam(TAB_LABEL_PARAM, next);
	applyDocumentTitle();
	dispatch(LABEL_EVENT, undefined);
	return next;
}

/** Host/distro marker for non-local backends: address bar, session, then the build env. */
export function resolveHostPrefix(): string | undefined {
	return readUrlParam(HOST_PARAM) ?? readSession(SESSION_HOST_KEY) ?? buildTimeHost();
}

/** Persist the host marker for this tab. Empty clears it. */
export function setHostPrefix(raw: string | undefined): string | undefined {
	const trimmed = (raw ?? '').trim();
	const next = trimmed ? trimmed.slice(0, 60) : undefined;
	writeSession(SESSION_HOST_KEY, next);
	syncUrlParam(HOST_PARAM, next);
	applyDocumentTitle();
	dispatch(LABEL_EVENT, undefined);
	return next;
}

/**
 * Build the tab label. An explicit override always wins; otherwise the
 * short workspace name, prefixed with the shortened host marker when one
 * is configured so same-named projects on different machines stay
 * distinguishable. Pure for tests.
 */
export function buildTabLabel(
	explicit: string | undefined,
	directory: string | undefined,
	host: string | undefined,
): string {
	const name = (explicit ?? '').trim();
	if (name) return name;
	const short = shortWorkspaceName(directory);
	if (!short) return '';
	const prefix = shortHostPrefix(host);
	return prefix ? `${prefix}:${short}` : short;
}

/** Resolve the current tab label from live sources. */
export function resolveTabLabel(): string {
	return buildTabLabel(resolveTabName(), resolveDirectory(), resolveHostPrefix());
}

/** Point the browser tab title at the workspace. The project leads because tab bars truncate the tail. */
export function applyDocumentTitle(): void {
	if (typeof document === 'undefined') return;
	try {
		const label = resolveTabLabel();
		document.title = label ? `${label} — wf-agent` : 'wf-agent';
	} catch {
		// Title is cosmetic; never fail the switch for it.
	}
}
