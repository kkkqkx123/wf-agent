import { untrack } from 'svelte';
import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { releaseFileModel, retargetFileModel } from '#lib/ide/editor-models';
import { listFs, readFsFile, writeFsFile, languageForPath } from '#lib/services/fs';
import { listFiles } from '#lib/services/git';
import { aggregateBadges, badgeForFile, type GitBadgeLevel } from '#lib/utils/git-badges';
import { toasts } from '#lib/stores/toast.svelte';
import { t } from '#lib/i18n';
import type { FsEntry } from '#lib/services/fs';

export interface IdeFileTab {
	path: string;
	content: string;
	original: string;
	hash: string | null;
	language: string;
	loading: boolean;
	saving: boolean;
	error: string | null;
	isBinary: boolean;
	truncated: boolean;
	preview: boolean;
}

export interface IdeDirState {
	entries: FsEntry[] | null;
	loading: boolean;
	error: string | null;
	loaded: boolean;
	/** In-flight listing; forced reloads join it instead of dropping themselves. */
	pending: Promise<void> | null;
	/** Bumped by every invalidate; a fetch that spans one is stale. */
	epoch: number;
}

function storageKey(suffix: string): string {
	return `wf.ide.v1.${suffix}`;
}

function readSession<T>(key: string, fallback: T): T {
	try {
		const raw = sessionStorage.getItem(storageKey(key));
		if (!raw) return fallback;
		return JSON.parse(raw) as T;
	} catch {
		return fallback;
	}
}

function writeSession(key: string, value: unknown): void {
	try {
		sessionStorage.setItem(storageKey(key), JSON.stringify(value));
	} catch {
		// Session storage unavailable; tabs stay memory-only.
	}
}

const MAX_OPEN_TABS = 20;

const TERMINAL_HEIGHT_DEFAULT = 192;
const TERMINAL_HEIGHT_MIN = 120;
const TERMINAL_HEIGHT_MAX = 600;
const SIDEBAR_WIDTH_DEFAULT = 256;
const SIDEBAR_WIDTH_MIN = 160;
const SIDEBAR_WIDTH_MAX = 480;
const HISTORY_WIDTH_DEFAULT = 288;
const HISTORY_WIDTH_MIN = 200;
const HISTORY_WIDTH_MAX = 480;
const HISTORY_CHANGES_HEIGHT_DEFAULT = 280;
const HISTORY_CHANGES_HEIGHT_MIN = 120;
const HISTORY_CHANGES_HEIGHT_MAX = 640;

export interface IdeReveal {
	path: string;
	line: number | null;
	col: number | null;
}

/** UI-level IDE commands that can be queued from the command palette. */
export type IdeCommand =
	| 'quickOpen'
	| 'contentSearch'
	| 'newFile'
	| 'newFolder'
	| 'saveFile'
	| 'saveAll'
	| 'reveal'
	| 'copyPath'
	| 'cut'
	| 'copy'
	| 'paste'
	| 'selectAll'
	| 'clearSelection'
	| 'upload'
	| 'download'
	| 'focusSearch'
	| 'toggleHistory'
	| 'nextChange'
	| 'prevChange'
	| 'viewDiff';

/** Session-scoped clipboard: paths plus copy/cut semantics. Never persisted. */
export interface IdeClipboard {
	mode: 'cut' | 'copy';
	paths: string[];
}

export type IdeSidebarView = 'tree' | 'search';

export class IdeStore {
	dirs = $state<SvelteMap<string, IdeDirState>>(new SvelteMap());
	tabs = $state<IdeFileTab[]>([]);
	activePath = $state<string | null>(null);
	root = $state<string>('');
	terminalOpen = $state(false);
	terminalHeight = $state(TERMINAL_HEIGHT_DEFAULT);
	terminalMaximized = $state(false);
	sidebarOpen = $state(true);
	sidebarWidth = $state(SIDEBAR_WIDTH_DEFAULT);
	historyWidth = $state(HISTORY_WIDTH_DEFAULT);
	historyChangesHeight = $state(HISTORY_CHANGES_HEIGHT_DEFAULT);
	pendingReveal = $state<IdeReveal | null>(null);
	gitBadges = $state<SvelteMap<string, GitBadgeLevel>>(new SvelteMap());
	gitBadgesAvailable = $state(true);
	revealRequest = $state<string | null>(null);
	commandQueue = $state<IdeCommand[]>([]);
	rootCreateRequest = $state<{ isDir: boolean; nonce: number } | null>(null);
	uploadRequest = $state<{ dir: string; nonce: number } | null>(null);
	clipboard = $state<IdeClipboard | null>(null);
	selectedPaths = $state<string[]>([]);
	anchorPath = $state<string | null>(null);
	sidebarView = $state<IdeSidebarView>('tree');
	searchTab = $state<'files' | 'content'>('files');
	historyOpen = $state(false);
	/** Bumped after saves and git writes so gutter and history refresh. */
	gitRefreshSeq = $state(0);

	constructor() {
		const saved = readSession<{ tabs: string[]; active: string | null }>('tabs', {
			tabs: [],
			active: null,
		});
		// Session storage is untrusted: validate the shape and drop duplicate
		// paths so a keyed each block can never throw each_key_duplicate.
		const savedPaths = Array.isArray(saved?.tabs) ? saved.tabs : [];
		const seenPaths = new SvelteSet<string>();
		const restoredPaths: string[] = [];
		for (const candidate of savedPaths) {
			if (typeof candidate !== 'string' || candidate === '' || seenPaths.has(candidate)) {
				continue;
			}
			seenPaths.add(candidate);
			restoredPaths.push(candidate);
			if (restoredPaths.length >= MAX_OPEN_TABS) break;
		}
		if (restoredPaths.length > 0) {
			this.tabs = restoredPaths.map((path) => ({
				path,
				content: '',
				original: '',
				hash: null,
				language: languageForPath(path),
				loading: true,
				saving: false,
				error: null,
				isBinary: false,
				truncated: false,
				preview: false,
			}));
			const savedActive = typeof saved?.active === 'string' ? saved.active : null;
			this.activePath =
				savedActive && seenPaths.has(savedActive) ? savedActive : (restoredPaths[0] ?? null);
		}
		if (typeof sessionStorage !== 'undefined') {
			const terminal = sessionStorage.getItem(storageKey('terminal'));
			this.terminalOpen = terminal === '1';
			const height = Number(
				sessionStorage.getItem(storageKey('term-height')) ?? String(TERMINAL_HEIGHT_DEFAULT),
			);
			if (
				Number.isFinite(height) &&
				height >= TERMINAL_HEIGHT_MIN &&
				height <= TERMINAL_HEIGHT_MAX
			) {
				this.terminalHeight = height;
			}
			this.terminalMaximized = sessionStorage.getItem(storageKey('term-max')) === '1';
			const sidebar = sessionStorage.getItem(storageKey('sidebar'));
			this.sidebarOpen = sidebar === null ? true : sidebar === '1';
			const sidebarWidth = Number(
				sessionStorage.getItem(storageKey('sidebar-width')) ?? String(SIDEBAR_WIDTH_DEFAULT),
			);
			if (
				Number.isFinite(sidebarWidth) &&
				sidebarWidth >= SIDEBAR_WIDTH_MIN &&
				sidebarWidth <= SIDEBAR_WIDTH_MAX
			) {
				this.sidebarWidth = sidebarWidth;
			}
			const historyWidth = Number(
				sessionStorage.getItem(storageKey('history-width')) ?? String(HISTORY_WIDTH_DEFAULT),
			);
			if (
				Number.isFinite(historyWidth) &&
				historyWidth >= HISTORY_WIDTH_MIN &&
				historyWidth <= HISTORY_WIDTH_MAX
			) {
				this.historyWidth = historyWidth;
			}
			const historyChangesHeight = Number(
				sessionStorage.getItem(storageKey('history-changes-height')) ??
					String(HISTORY_CHANGES_HEIGHT_DEFAULT),
			);
			if (
				Number.isFinite(historyChangesHeight) &&
				historyChangesHeight >= HISTORY_CHANGES_HEIGHT_MIN &&
				historyChangesHeight <= HISTORY_CHANGES_HEIGHT_MAX
			) {
				this.historyChangesHeight = historyChangesHeight;
			}
			this.historyOpen = sessionStorage.getItem(storageKey('history')) === '1';
		}
		this.restoreDrafts();
	}

	private persistTabs(): void {
		writeSession('tabs', {
			tabs: this.tabs.map((t) => t.path),
			active: this.activePath,
		});
	}

	setRoot(root: string): void {
		this.root = root;
	}

	dirState(path: string): IdeDirState {
		const hit = this.dirs.get(path);
		if (hit) return hit;
		// First-seen dirs are created here too: the write runs untracked so
		// template and derived callers never throw state_unsafe_mutation.
		return untrack(() => {
			let state = this.dirs.get(path);
			if (!state) {
				state = {
					entries: null,
					loading: false,
					error: null,
					loaded: false,
					pending: null,
					epoch: 0,
				};
				this.dirs.set(path, state);
			}
			return state;
		});
	}

	/**
	 * Publish a snapshot so template readers see the new rows. Map values
	 * are not deeply reactive: mutating the stored object in place never
	 * notifies, so every user-visible transition replaces the entry. A
	 * same-reference set would be a no-op, hence the snapshot copy.
	 */
	private publishDir(path: string, state: IdeDirState): IdeDirState {
		const snapshot: IdeDirState = { ...state };
		this.dirs.set(path, snapshot);
		return snapshot;
	}

	async loadDir(path: string, force = false): Promise<void> {
		let state = this.dirState(path);
		if (state.loading) {
			// A forced reload joins the in-flight fetch instead of dropping
			// itself: structural mutations must never lose their tree refresh.
			if (!force || !state.pending) return;
			try {
				await state.pending;
			} catch {
				// A failed stale fetch must not block the fresh load below.
			}
			state = this.dirState(path);
			if (state.loaded && !force) return;
		} else if (state.loaded && !force) {
			return;
		}
		// Loop: an invalidate that lands while fetching voids the rows just
		// read, so re-read instead of publishing a stale tree.
		for (;;) {
			state = this.dirState(path);
			const seen = state.epoch;
			state.loading = true;
			state.error = null;
			state = this.publishDir(path, state);
			let entries: FsEntry[] | null = null;
			let failure: unknown = null;
			const pending = listFs(path).then(
				(rows) => {
					entries = rows;
				},
				(e) => {
					failure = e;
				},
			);
			state.pending = pending;
			await pending;
			state = this.dirState(path);
			if (state.pending === pending) {
				state.pending = null;
				state.loading = false;
			}
			if (state.epoch !== seen) continue;
			if (failure !== null) {
				state.error = failure instanceof Error ? failure.message : String(failure);
			} else if (entries !== null) {
				state.entries = entries;
				state.loaded = true;
			}
			this.publishDir(path, state);
			return;
		}
	}

	/**
	 * Targeted invalidate of one directory listing. Ancestors never render
	 * the changed entries (a listing shows direct children only), so only
	 * the direct parent is invalidated; its forced reload publishes the
	 * visible transition. Stays silent on purpose: the reload that always
	 * follows renders the rows without flashing an empty tree.
	 */
	invalidateDir(path: string): void {
		const state = this.dirs.get(path);
		if (state) {
			state.loaded = false;
			state.entries = null;
			// In-flight fetches that span this invalidate re-read on completion
			// instead of publishing rows from before the mutation.
			state.epoch += 1;
		}
	}

	active(): IdeFileTab | null {
		if (!this.activePath) return this.tabs[0] ?? null;
		return this.tabs.find((t) => t.path === this.activePath) ?? null;
	}

	isDirty(path: string): boolean {
		const tab = this.tabs.find((t) => t.path === path);
		return tab ? tab.content !== tab.original : false;
	}

	get dirtyPaths(): string[] {
		return this.tabs.filter((t) => t.content !== t.original).map((t) => t.path);
	}

	async openFile(
		path: string,
		preview = false,
		line?: number | null,
		col?: number | null,
	): Promise<void> {
		const lineNo = line !== undefined && line !== null && line >= 1 ? Math.floor(line) : null;
		const colNo = col !== undefined && col !== null && col >= 1 ? Math.floor(col) : null;
		if (lineNo !== null || colNo !== null) {
			this.pendingReveal = { path, line: lineNo, col: colNo };
		} else if (this.pendingReveal?.path === path) {
			this.pendingReveal = null;
		}
		const existing = this.tabs.find((t) => t.path === path);
		if (existing) {
			this.activePath = path;
			if (!preview) existing.preview = false;
			if (existing.loading) await this.loadFile(path);
			this.persistTabs();
			return;
		}
		const tab: IdeFileTab = {
			path,
			content: '',
			original: '',
			hash: null,
			language: languageForPath(path),
			loading: true,
			saving: false,
			error: null,
			isBinary: false,
			truncated: false,
			preview,
		};
		if (preview) {
			const previewIndex = this.tabs.findIndex((t) => t.preview);
			if (previewIndex >= 0) {
				const [replaced] = this.tabs.splice(previewIndex, 1, tab);
				if (replaced && replaced.path !== tab.path) releaseFileModel(replaced.path);
			} else this.tabs.push(tab);
		} else {
			this.tabs.push(tab);
		}
		this.evictExcessTabs(path);
		this.activePath = path;
		this.persistTabs();
		await this.loadFile(path);
	}

	private evictExcessTabs(keepPath: string): void {
		let evicted = 0;
		while (this.tabs.length > MAX_OPEN_TABS) {
			const victim = this.tabs.find(
				(t) => t.path !== keepPath && t.path !== this.activePath && t.content === t.original,
			);
			if (!victim) return;
			this.tabs = this.tabs.filter((t) => t !== victim);
			releaseFileModel(victim.path);
			evicted += 1;
		}
		if (evicted > 0) {
			toasts.warning(t('page.ide.tabLimitTitle', { count: evicted }));
		}
	}

	async loadFile(path: string, silent = false): Promise<void> {
		const tab = this.tabs.find((t) => t.path === path);
		if (!tab) return;
		const background = silent && tab.content !== '';
		if (!background) {
			tab.loading = true;
		}
		tab.error = null;
		try {
			const file = await readFsFile(path);
			tab.isBinary = file.is_binary;
			tab.truncated = file.truncated ?? false;
			tab.hash = file.hash ?? null;
			const serverContent = file.is_binary ? '' : (file.content ?? '');
			const drafts = this.readDrafts();
			const draft = drafts[path];
			if (
				!tab.truncated &&
				!tab.isBinary &&
				draft &&
				typeof draft.content === 'string' &&
				draft.content !== serverContent
			) {
				tab.original = serverContent;
				tab.content = draft.content;
			} else {
				tab.content = serverContent;
				tab.original = tab.content;
				this.clearDraft(path);
			}
		} catch (e) {
			tab.error = e instanceof Error ? e.message : String(e);
		} finally {
			tab.loading = false;
		}
	}

	updateContent(path: string, content: string): void {
		const tab = this.tabs.find((t) => t.path === path);
		if (tab) {
			tab.content = content;
			this.persistDraft(path, content, tab.original);
		}
	}

	moveTab(from: string, to: string): void {
		if (from === to) return;
		const entries = [...this.tabs];
		const fromIndex = entries.findIndex((t) => t.path === from);
		const toIndex = entries.findIndex((t) => t.path === to);
		if (fromIndex < 0 || toIndex < 0) return;
		const [moved] = entries.splice(fromIndex, 1);
		if (!moved) return;
		const target = fromIndex < toIndex ? toIndex - 1 : toIndex;
		entries.splice(target, 0, moved);
		this.tabs = entries;
		this.persistTabs();
	}

	private draftKey(): string {
		return 'wf.ide.drafts.v1';
	}

	private readDrafts(): Record<string, { content: string; savedAt: number }> {
		try {
			const raw = localStorage.getItem(this.draftKey());
			if (!raw) return {};
			const parsed = JSON.parse(raw) as Record<string, { content: string; savedAt: number }>;
			if (typeof parsed !== 'object' || parsed === null) return {};
			return parsed;
		} catch {
			return {};
		}
	}

	private persistDraft(path: string, content: string, original: string): void {
		try {
			if (typeof localStorage === 'undefined') return;
			const drafts = this.readDrafts();
			if (content === original) {
				if (path in drafts) {
					delete drafts[path];
					localStorage.setItem(this.draftKey(), JSON.stringify(drafts));
				}
				return;
			}
			if (content.length > 512 * 1024) return;
			drafts[path] = { content, savedAt: Date.now() };
			const keys = Object.keys(drafts);
			if (keys.length > 30) {
				const oldest = keys
					.map((key) => ({ key, at: drafts[key]?.savedAt ?? 0 }))
					.sort((a, b) => a.at - b.at)[0];
				if (oldest) delete drafts[oldest.key];
			}
			localStorage.setItem(this.draftKey(), JSON.stringify(drafts));
		} catch {
			// Draft persistence is best-effort.
		}
	}

	private restoreDrafts(): void {
		try {
			if (typeof localStorage === 'undefined') return;
			const drafts = this.readDrafts();
			for (const tab of this.tabs) {
				const draft = drafts[tab.path];
				if (draft && typeof draft.content === 'string') {
					tab.content = draft.content;
					tab.original = '';
				}
			}
		} catch {
			// Corrupt drafts never block startup.
		}
	}

	private clearDraft(path: string): void {
		try {
			if (typeof localStorage === 'undefined') return;
			const drafts = this.readDrafts();
			if (path in drafts) {
				delete drafts[path];
				localStorage.setItem(this.draftKey(), JSON.stringify(drafts));
			}
		} catch {
			// Ignore storage failures.
		}
	}

	async saveFile(path: string, force = false): Promise<boolean> {
		const tab = this.tabs.find((t) => t.path === path);
		if (!tab || tab.saving || tab.isBinary || tab.truncated) return false;
		tab.saving = true;
		tab.error = null;
		try {
			const saved = await writeFsFile(
				path,
				tab.content,
				force ? undefined : (tab.hash ?? undefined),
			);
			tab.original = tab.content;
			tab.hash = saved.hash ?? tab.hash;
			this.clearDraft(path);
			return true;
		} catch (e) {
			tab.error = e instanceof Error ? e.message : String(e);
			return false;
		} finally {
			tab.saving = false;
		}
	}

	async saveAll(): Promise<void> {
		for (const tab of this.tabs) {
			if (tab.content !== tab.original && !tab.isBinary) {
				await this.saveFile(tab.path);
			}
		}
	}

	closeFile(path: string): boolean {
		const tab = this.tabs.find((t) => t.path === path);
		if (!tab) return true;
		if (tab.content !== tab.original) return false;
		this.tabs = this.tabs.filter((t) => t.path !== path);
		releaseFileModel(path);
		if (this.activePath === path) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.persistTabs();
		return true;
	}

	forceClose(path: string): void {
		this.tabs = this.tabs.filter((t) => t.path !== path);
		releaseFileModel(path);
		if (this.activePath === path) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.clearDraft(path);
		this.persistTabs();
	}

	setTerminalOpen(open: boolean): void {
		this.terminalOpen = open;
		try {
			sessionStorage.setItem(storageKey('terminal'), open ? '1' : '0');
		} catch {
			// Ignore storage failures.
		}
	}

	setTerminalHeight(height: number): void {
		const clamped = Math.min(
			TERMINAL_HEIGHT_MAX,
			Math.max(TERMINAL_HEIGHT_MIN, Math.round(height)),
		);
		this.terminalHeight = clamped;
		try {
			sessionStorage.setItem(storageKey('term-height'), String(clamped));
		} catch {
			// Ignore storage failures.
		}
	}

	setSidebarOpen(open: boolean): void {
		this.sidebarOpen = open;
		writeSession('sidebar', open ? '1' : '0');
	}

	setSidebarWidth(width: number): void {
		const clamped = Math.min(SIDEBAR_WIDTH_MAX, Math.max(SIDEBAR_WIDTH_MIN, Math.round(width)));
		this.sidebarWidth = clamped;
		try {
			sessionStorage.setItem(storageKey('sidebar-width'), String(clamped));
		} catch {
			// Ignore storage failures.
		}
	}

	resetSidebarWidth(): void {
		this.setSidebarWidth(SIDEBAR_WIDTH_DEFAULT);
	}

	setHistoryWidth(width: number): void {
		const clamped = Math.min(HISTORY_WIDTH_MAX, Math.max(HISTORY_WIDTH_MIN, Math.round(width)));
		this.historyWidth = clamped;
		try {
			sessionStorage.setItem(storageKey('history-width'), String(clamped));
		} catch {
			// Ignore storage failures.
		}
	}

	resetHistoryWidth(): void {
		this.setHistoryWidth(HISTORY_WIDTH_DEFAULT);
	}

	setHistoryChangesHeight(height: number): void {
		const clamped = Math.min(
			HISTORY_CHANGES_HEIGHT_MAX,
			Math.max(HISTORY_CHANGES_HEIGHT_MIN, Math.round(height)),
		);
		this.historyChangesHeight = clamped;
		try {
			sessionStorage.setItem(storageKey('history-changes-height'), String(clamped));
		} catch {
			// Ignore storage failures.
		}
	}

	resetHistoryChangesHeight(): void {
		this.setHistoryChangesHeight(HISTORY_CHANGES_HEIGHT_DEFAULT);
	}

	resetTerminalHeight(): void {
		this.setTerminalHeight(TERMINAL_HEIGHT_DEFAULT);
	}

	setHistoryOpen(open: boolean): void {
		this.historyOpen = open;
		writeSession('history', open ? '1' : '0');
	}

	setClipboard(next: IdeClipboard | null): void {
		this.clipboard = next ? { mode: next.mode, paths: [...next.paths] } : null;
	}

	clearClipboard(): void {
		this.clipboard = null;
	}

	setSelection(paths: string[]): void {
		this.selectedPaths = [...paths];
		if (paths.length === 0) this.anchorPath = null;
		else if (!this.anchorPath || !paths.includes(this.anchorPath)) {
			this.anchorPath = paths[paths.length - 1] ?? null;
		}
	}

	clearSelection(): void {
		this.selectedPaths = [];
		this.anchorPath = null;
	}

	isSelected(path: string): boolean {
		return this.selectedPaths.includes(path);
	}

	bumpGitRefresh(): void {
		this.gitRefreshSeq += 1;
	}

	/** Rewrite open tabs after a move/copy target change. */
	retargetPath(from: string, to: string): void {
		const prefix = `${from}/`;
		const previousActive = this.activePath;
		const moved: Array<{ from: string; to: string }> = [];
		for (const tab of this.tabs) {
			if (tab.path === from) {
				moved.push({ from: tab.path, to });
				tab.path = to;
				tab.language = languageForPath(to);
			} else if (tab.path.startsWith(prefix)) {
				const next = `${to}${tab.path.slice(from.length)}`;
				moved.push({ from: tab.path, to: next });
				tab.path = next;
				tab.language = languageForPath(tab.path);
			}
		}
		for (const pair of moved) retargetFileModel(pair.from, pair.to);
		// A rename onto an already-open path would duplicate a keyed-each key;
		// keep the first tab and drop the shadowed ones.
		const seenPaths = new SvelteSet<string>();
		const dropped: string[] = [];
		this.tabs = this.tabs.filter((tab) => {
			if (seenPaths.has(tab.path)) {
				dropped.push(tab.path);
				return false;
			}
			seenPaths.add(tab.path);
			return true;
		});
		for (const path of dropped) releaseFileModel(path);
		if (previousActive === from) this.activePath = to;
		else if (previousActive?.startsWith(prefix)) {
			this.activePath = `${to}${previousActive.slice(from.length)}`;
		}
		if (this.activePath && !this.tabs.some((t) => t.path === this.activePath)) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.persistTabs();
	}

	/** Close tabs whose files were removed. */
	closePaths(paths: string[]): void {
		const doomed = new SvelteSet(paths);
		const prefixes = paths.map((p) => `${p}/`);
		const removed: string[] = [];
		this.tabs = this.tabs.filter((tab) => {
			if (doomed.has(tab.path)) {
				this.clearDraft(tab.path);
				removed.push(tab.path);
				return false;
			}
			if (prefixes.some((prefix) => tab.path.startsWith(prefix))) {
				removed.push(tab.path);
				return false;
			}
			return true;
		});
		for (const path of removed) releaseFileModel(path);
		if (this.activePath && !this.tabs.some((t) => t.path === this.activePath)) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.persistTabs();
	}

	setTerminalMaximized(maximized: boolean): void {
		this.terminalMaximized = maximized;
		try {
			sessionStorage.setItem(storageKey('term-max'), maximized ? '1' : '0');
		} catch {
			// Ignore storage failures.
		}
	}

	requestReveal(path: string): void {
		this.revealRequest = path;
	}

	consumeReveal(): string | null {
		const pending = this.revealRequest;
		this.revealRequest = null;
		return pending;
	}

	/** Queue a UI-level IDE command (from the command palette) for the page. */
	requestCommand(command: IdeCommand): void {
		this.commandQueue = [...this.commandQueue, command];
	}

	consumeCommand(): IdeCommand | null {
		const [next, ...rest] = this.commandQueue;
		if (next === undefined) return null;
		this.commandQueue = rest;
		return next;
	}

	/** Ask the file tree to start an inline create input at the workspace root. */
	requestRootCreate(isDir: boolean): void {
		this.rootCreateRequest = { isDir, nonce: (this.rootCreateRequest?.nonce ?? 0) + 1 };
	}

	/** Ask the file tree to open the upload picker for `dir`. */
	requestUpload(dir: string): void {
		this.uploadRequest = { dir, nonce: (this.uploadRequest?.nonce ?? 0) + 1 };
	}

	/** Refresh the version-status table from the git change list. Never throws. */
	async refreshGitStatus(): Promise<void> {
		try {
			const files = await listFiles();
			const next = new SvelteMap<string, GitBadgeLevel>();
			for (const file of files) {
				const level = badgeForFile(file);
				if (level) next.set(file.path, level);
			}
			this.gitBadges = next;
			this.gitBadgesAvailable = true;
		} catch {
			this.gitBadges = new SvelteMap<string, GitBadgeLevel>();
			this.gitBadgesAvailable = false;
		}
	}

	/** Aggregate badge for a directory from its changed descendants. */
	dirBadge(dirPath: string): GitBadgeLevel | null {
		const prefix = dirPath ? `${dirPath}/` : '';
		const levels: GitBadgeLevel[] = [];
		for (const [path, level] of this.gitBadges) {
			if (prefix && !path.startsWith(prefix)) continue;
			levels.push(level);
		}
		return aggregateBadges(levels);
	}
}

export const ide = new IdeStore();
