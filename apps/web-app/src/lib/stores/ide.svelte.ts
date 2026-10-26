import { SvelteMap } from 'svelte/reactivity';
import { listFs, readFsFile, writeFsFile } from '#lib/services/fs';
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

function languageFor(path: string): string {
	const lower = path.toLowerCase();
	if (lower.endsWith('.svelte')) return 'svelte';
	if (lower.endsWith('.ts') || lower.endsWith('.tsx')) return 'typescript';
	if (lower.endsWith('.js') || lower.endsWith('.jsx') || lower.endsWith('.mjs'))
		return 'javascript';
	if (lower.endsWith('.json')) return 'json';
	if (lower.endsWith('.rs')) return 'rust';
	if (lower.endsWith('.py')) return 'python';
	if (lower.endsWith('.toml')) return 'toml';
	if (lower.endsWith('.yaml') || lower.endsWith('.yml')) return 'yaml';
	if (lower.endsWith('.md')) return 'markdown';
	if (lower.endsWith('.css')) return 'css';
	if (lower.endsWith('.html')) return 'html';
	if (lower.endsWith('.sh')) return 'shell';
	return 'plaintext';
}

export class IdeStore {
	dirs = $state<SvelteMap<string, IdeDirState>>(new SvelteMap());
	tabs = $state<IdeFileTab[]>([]);
	activePath = $state<string | null>(null);
	root = $state<string>('');
	terminalOpen = $state(true);
	terminalHeight = $state(280);
	rightOpen = $state(false);

	constructor() {
		const saved = readSession<{ tabs: string[]; active: string | null }>('tabs', {
			tabs: [],
			active: null,
		});
		if (saved.tabs.length > 0) {
			this.tabs = saved.tabs.slice(0, 20).map((path) => ({
				path,
				content: '',
				original: '',
				hash: null,
				language: languageFor(path),
				loading: true,
				saving: false,
				error: null,
				isBinary: false,
				preview: false,
			}));
			this.activePath = saved.active ?? saved.tabs[0] ?? null;
		}
		if (typeof sessionStorage !== 'undefined') {
			const height = Number(sessionStorage.getItem(storageKey('term-height')) ?? '280');
			if (Number.isFinite(height) && height >= 120 && height <= 800) {
				this.terminalHeight = height;
			}
		}
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

	dirKey(path: string): string {
		return path;
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

	async openFile(path: string, preview = false): Promise<void> {
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
			language: languageFor(path),
			loading: true,
			saving: false,
			error: null,
			isBinary: false,
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
		this.activePath = path;
		this.persistTabs();
		await this.loadFile(path);
	}

	async loadFile(path: string): Promise<void> {
		const tab = this.tabs.find((t) => t.path === path);
		if (!tab) return;
		tab.loading = true;
		tab.error = null;
		try {
			const file = await readFsFile(path);
			tab.isBinary = file.is_binary;
			tab.hash = file.hash ?? null;
			tab.content = file.is_binary ? '' : (file.content ?? '');
			tab.original = tab.content;
		} catch (e) {
			tab.error = e instanceof Error ? e.message : String(e);
		} finally {
			tab.loading = false;
		}
	}

	updateContent(path: string, content: string): void {
		const tab = this.tabs.find((t) => t.path === path);
		if (tab) tab.content = content;
	}

	async saveFile(path: string): Promise<boolean> {
		const tab = this.tabs.find((t) => t.path === path);
		if (!tab || tab.saving || tab.isBinary) return false;
		tab.saving = true;
		tab.error = null;
		try {
			const saved = await writeFsFile(path, tab.content, tab.hash ?? undefined);
			tab.original = tab.content;
			tab.hash = saved.hash ?? tab.hash;
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
		this.persistTabs();
	}

	closeOthers(path: string): void {
		const keep = this.tabs.filter((t) => t.path === path || t.content === t.original);
		this.tabs = this.tabs.filter((t) => t.path === path || t.content !== t.original);
		void keep;
		this.activePath = path;
		this.persistTabs();
	}

	moveTab(path: string, direction: -1 | 1): void {
		const index = this.tabs.findIndex((t) => t.path === path);
		const target = index + direction;
		if (index < 0 || target < 0 || target >= this.tabs.length) return;
		const next = [...this.tabs];
		[next[index], next[target]] = [next[target], next[index]];
		this.tabs = next;
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
}

export const ide = new IdeStore();
