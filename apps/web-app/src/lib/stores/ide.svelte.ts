import { SvelteMap } from 'svelte/reactivity';
import { listFs, readFsFile, writeFsFile, languageForPath } from '#lib/services/fs';
import { listFiles } from '#lib/services/git';
import { aggregateBadges, badgeForFile, type GitBadgeLevel } from '#lib/utils/git-badges';
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

export interface IdeReveal {
	path: string;
	line: number | null;
	col: number | null;
}

export class IdeStore {
	dirs = $state<SvelteMap<string, IdeDirState>>(new SvelteMap());
	tabs = $state<IdeFileTab[]>([]);
	activePath = $state<string | null>(null);
	root = $state<string>('');
	terminalOpen = $state(true);
	terminalHeight = $state(280);
	terminalMaximized = $state(false);
	sidebarOpen = $state(true);
	pendingReveal = $state<IdeReveal | null>(null);
	gitBadges = $state<SvelteMap<string, GitBadgeLevel>>(new SvelteMap());
	gitBadgesAvailable = $state(true);
	inspectorOpen = $state(false);
	revealRequest = $state<string | null>(null);

	constructor() {
		const saved = readSession<{ tabs: string[]; active: string | null }>('tabs', {
			tabs: [],
			active: null,
		});
		if (saved.tabs.length > 0) {
			this.tabs = saved.tabs.slice(0, MAX_OPEN_TABS).map((path) => ({
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
			this.activePath = saved.active ?? saved.tabs[0] ?? null;
		}
		if (typeof sessionStorage !== 'undefined') {
			const height = Number(sessionStorage.getItem(storageKey('term-height')) ?? '280');
			if (Number.isFinite(height) && height >= 120 && height <= 800) {
				this.terminalHeight = height;
			}
			this.inspectorOpen = sessionStorage.getItem(storageKey('inspector')) === '1';
			this.terminalMaximized = sessionStorage.getItem(storageKey('term-max')) === '1';
			const sidebar = sessionStorage.getItem(storageKey('sidebar'));
			this.sidebarOpen = sidebar === null ? true : sidebar === '1';
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
		let state = this.dirs.get(path);
		if (!state) {
			state = { entries: null, loading: false, error: null, loaded: false };
			this.dirs.set(path, state);
		}
		return state;
	}

	async loadDir(path: string, force = false): Promise<void> {
		const state = this.dirState(path);
		if (state.loading) return;
		if (state.loaded && !force) return;
		state.loading = true;
		state.error = null;
		try {
			const entries = await listFs(path);
			state.entries = entries;
			state.loaded = true;
		} catch (e) {
			state.error = e instanceof Error ? e.message : String(e);
		} finally {
			state.loading = false;
		}
	}

	invalidateDir(path: string): void {
		const state = this.dirs.get(path);
		if (state) {
			state.loaded = false;
			state.entries = null;
		}
	}

	invalidateParents(path: string): void {
		const parts = path.split('/').filter(Boolean);
		for (let i = parts.length - 1; i >= 0; i -= 1) {
			this.invalidateDir(parts.slice(0, i).join('/'));
		}
		this.invalidateDir('');
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

	async openFile(path: string, preview = false, line?: number | null, col?: number | null): Promise<void> {
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
			if (previewIndex >= 0) this.tabs.splice(previewIndex, 1, tab);
			else this.tabs.push(tab);
		} else {
			const previewIndex = this.tabs.findIndex((t) => t.preview);
			if (previewIndex >= 0) this.tabs.splice(previewIndex, 1, tab);
			else this.tabs.push(tab);
		}
		this.evictExcessTabs(path);
		this.activePath = path;
		this.persistTabs();
		await this.loadFile(path);
	}

	private evictExcessTabs(keepPath: string): void {
		while (this.tabs.length > MAX_OPEN_TABS) {
			const victim = this.tabs.find(
				(t) => t.path !== keepPath && t.path !== this.activePath && t.content === t.original,
			);
			if (!victim) return;
			this.tabs = this.tabs.filter((t) => t !== victim);
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
			const saved = await writeFsFile(path, tab.content, force ? undefined : (tab.hash ?? undefined));
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
		if (this.activePath === path) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.persistTabs();
		return true;
	}

	forceClose(path: string): void {
		this.tabs = this.tabs.filter((t) => t.path !== path);
		if (this.activePath === path) {
			this.activePath = this.tabs[0]?.path ?? null;
		}
		this.clearDraft(path);
		this.persistTabs();
	}

	setTerminalHeight(height: number): void {
		const clamped = Math.min(800, Math.max(120, Math.round(height)));
		this.terminalHeight = clamped;
		try {
			sessionStorage.setItem(storageKey('term-height'), String(clamped));
		} catch {
			// Ignore storage failures.
		}
	}

	setInspectorOpen(open: boolean): void {
		this.inspectorOpen = open;
		writeSession('inspector', open ? '1' : '0');
	}

	setSidebarOpen(open: boolean): void {
		this.sidebarOpen = open;
		writeSession('sidebar', open ? '1' : '0');
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
