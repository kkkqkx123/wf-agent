import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { toasts } from '#lib/stores/toast.svelte';
import { git as gitStore } from '#lib/stores/git.svelte';
import {
	getCommitFiles,
	getDiff,
	getDiffRange,
	getMeta,
	getRegistry,
	getStagedDiff,
	getStatus,
	listBranches,
	listCommits,
	listFiles,
	listReflog,
	listRemotes,
	listStash,
	listSubmodules,
	listTags,
	listWorktrees,
	runAction,
	type BranchEntry,
	type CommitEntry,
	type CommitFileEntry,
	type GitFile,
	type ReflogEntry,
	type RegistryEntry,
	type RemoteEntry,
	type RepoMeta,
	type StashEntry,
	type StatusCounts,
	type SubmoduleEntry,
	type TagEntry,
	type WorktreeEntry,
} from '#lib/services/git';
import { checkDrift, UNDO_TOAST_ACTIONS, visibleControls } from '#lib/git/bindings';
import type { DiffFile, DiffLine } from '#lib/types/diff';
import { CLIENT_ONLY_ACTIONS, confirmTarget, parseToken } from '#lib/git/parse';

export const PANELS = [
	{ id: 'status', label: 'Status' },
	{ id: 'files', label: 'Files' },
	{ id: 'conflicts', label: 'Conflicts' },
	{ id: 'worktrees', label: 'Worktrees' },
	{ id: 'submodules', label: 'Submodules' },
	{ id: 'branches', label: 'Branches' },
	{ id: 'remotes', label: 'Remotes' },
	{ id: 'tags', label: 'Tags' },
	{ id: 'commits', label: 'Commits' },
	{ id: 'reflog', label: 'Reflog' },
	{ id: 'stash', label: 'Stash' },
] as const;

export type PanelId = (typeof PANELS)[number]['id'];

export interface PanelGroup {
	id: string;
	panels: PanelId[];
}

/** Side panel groups mirroring the lazygit default sidebar layout. */
export const PANEL_GROUPS: PanelGroup[] = [
	{ id: 'status', panels: ['status'] },
	{ id: 'workspace', panels: ['files', 'worktrees', 'submodules'] },
	{ id: 'refs', panels: ['branches', 'remotes', 'tags'] },
	{ id: 'history', panels: ['commits', 'reflog'] },
	{ id: 'stash', panels: ['stash'] },
];

/** Keyboard panel cycling order for Tab navigation across the board. */
export const PANEL_ORDER: PanelId[] = PANELS.map((entry) => entry.id);

export function groupForPanel(panel: PanelId): PanelGroup {
	return PANEL_GROUPS.find((group) => group.panels.includes(panel)) ?? PANEL_GROUPS[0];
}

export type MainViewMode = 'diff' | 'staging' | 'commitFiles';

/** Focus-driven screen mode for the git layout. */
export type GitLayoutFocus = 'none' | 'board' | 'main' | 'terminal';

export function defaultViewModeForPanel(panel: PanelId): MainViewMode {
	if (panel === 'commits') return 'commitFiles';
	return 'diff';
}

export function isViewModeValidForPanel(mode: MainViewMode, panel: PanelId): boolean {
	if (panel === 'files' || panel === 'conflicts') return mode === 'diff' || mode === 'staging';
	if (panel === 'commits') return mode === 'diff' || mode === 'commitFiles';
	return mode === 'diff';
}

export interface ActionLogEntry {
	seq: number;
	action: string;
	summary: string;
	result: string;
}

export const PARAM_LABELS: Record<string, string> = {
	message: 'Message',
	branch: 'Branch',
	remote: 'Remote',
	path: 'Path',
	commit: 'Commit',
	base: 'Base commit',
	onto: 'Onto',
	from: 'From',
	to: 'To',
	tag: 'Tag',
	index: 'Stash index',
	query: 'Query',
	hunkIndex: 'Hunk',
	lines: 'Lines',
};

export const PARAM_DEFAULTS: Record<string, Record<string, string>> = {
	push: { remote: 'origin' },
	pull: { remote: 'origin' },
};

export const GUARDED_DANGERS = new SvelteSet(['destructive', 'history_rewrite']);

/** Reverse staging actions for toast shortcuts; history undo does not apply to the index. */
const STAGING_REVERSE: Record<string, string> = {
	'stage.toggle': 'unstage.all',
	'stage.all': 'unstage.all',
	'stage.hunk': 'unstage.hunk',
	'stage.lines': 'unstage.lines',
	'unstage.all': 'stage.all',
	'unstage.hunk': 'stage.hunk',
	'unstage.lines': 'stage.lines',
};

/** Split flattened diff rows back into per-hunk content rows (meta rows are boundaries). */
export function hunksOfDiffFile(file: DiffFile): DiffLine[][] {
	const hunks: DiffLine[][] = [];
	let current: DiffLine[] | null = null;
	for (const line of file.lines) {
		if (line.type === 'meta') {
			if (current) hunks.push(current);
			current = [];
		} else if (current) {
			current.push(line);
		}
	}
	if (current) hunks.push(current);
	return hunks;
}

/** Selectable content indices inside one hunk (added or removed rows only). */
export function selectableIndices(rows: DiffLine[]): number[] {
	const out: number[] = [];
	rows.forEach((row, idx) => {
		if (row.type === 'add' || row.type === 'del') out.push(idx);
	});
	return out;
}

export class GitWorkbench {
	panel = $state<PanelId>('status');
	viewMode = $state<MainViewMode>('diff');
	meta = $state<RepoMeta | null>(null);
	status = $state<StatusCounts | null>(null);
	files = $state<GitFile[]>([]);
	branches = $state<BranchEntry[]>([]);
	commits = $state<CommitEntry[]>([]);
	commitsOffset = $state(0);
	commitsHasMore = $state(false);
	commitsLoading = $state(false);
	reflogs = $state<ReflogEntry[]>([]);
	stash = $state<StashEntry[]>([]);
	diff = $state<DiffFile[]>([]);
	stagedDiff = $state<DiffFile[]>([]);
	commitFiles = $state<CommitFileEntry[]>([]);
	commitFileDiffPath = $state<string | null>(null);
	remotes = $state<RemoteEntry[]>([]);
	tags = $state<TagEntry[]>([]);
	worktrees = $state<WorktreeEntry[]>([]);
	submodules = $state<SubmoduleEntry[]>([]);
	loading = $state(false);
	loadError = $state<string | null>(null);
	noRepo = $state(false);
	liveDirty = $state(false);
	private stopLive: (() => void) | null = null;

	/** Line selection: file path -> hunk index -> sorted content line indices. */
	lineSelection = $state<Record<string, Record<number, number[]>>>({});
	selectedRemote = $state<string | null>(null);
	selectedTag = $state<string | null>(null);
	actionLog = $state<ActionLogEntry[]>([]);
	private actionSeq = 0;
	private diffRequest = 0;

	registry = $state<RegistryEntry[]>([]);
	filters = $state<Record<PanelId, string>>({
		status: '',
		files: '',
		conflicts: '',
		worktrees: '',
		submodules: '',
		branches: '',
		remotes: '',
		tags: '',
		commits: '',
		reflog: '',
		stash: '',
	});

	token = $state('');
	terminalOpen = $state(false);
	layoutFocus = $state<GitLayoutFocus>('none');

	dialogAction = $state<string | null>(null);
	dialogOpen = $state(false);
	dialogValues = $state<Record<string, string>>({});
	dialogConfirm = $state('');
	helpOpen = $state(false);

	tokenResult = $derived(parseToken(this.token, this.panel));
	controls = $derived(visibleControls(this.panel));
	filterQuery = $derived(this.filters[this.panel]);
	visibleFiles = $derived(this.files.filter((f) => f.path.includes(this.filterQuery)));
	visibleConflicts = $derived(
		this.files
			.filter((f) => f.kind === 'conflicted')
			.filter((f) => f.path.includes(this.filterQuery)),
	);
	visibleBranches = $derived(this.branches.filter((b) => b.name.includes(this.filterQuery)));
	visibleRemotes = $derived(
		this.remotes.filter(
			(r) => r.name.includes(this.filterQuery) || r.url.includes(this.filterQuery),
		),
	);
	visibleTags = $derived(
		this.tags.filter(
			(t) => t.name.includes(this.filterQuery) || t.commit.includes(this.filterQuery),
		),
	);
	visibleCommits = $derived(
		this.commits.filter(
			(c) => c.subject.includes(this.filterQuery) || c.shortId.includes(this.filterQuery),
		),
	);
	visibleReflogs = $derived(
		this.reflogs.filter(
			(r) =>
				r.message.includes(this.filterQuery) ||
				r.shortId.includes(this.filterQuery) ||
				r.selector.includes(this.filterQuery),
		),
	);
	visibleStash = $derived(
		this.stash.filter(
			(s) => s.message.includes(this.filterQuery) || s.name.includes(this.filterQuery),
		),
	);
	visibleWorktrees = $derived(
		this.worktrees.filter(
			(w) => w.path.includes(this.filterQuery) || w.branch.includes(this.filterQuery),
		),
	);
	visibleSubmodules = $derived(
		this.submodules.filter(
			(s) => s.path.includes(this.filterQuery) || s.commit.includes(this.filterQuery),
		),
	);
	dialogSpec = $derived(
		this.dialogAction ? this.registry.find((entry) => entry.id === this.dialogAction) : undefined,
	);
	dialogNeedsConfirm = $derived(
		this.dialogSpec ? GUARDED_DANGERS.has(this.dialogSpec.danger) : false,
	);
	dialogTarget = $derived(
		this.dialogAction ? confirmTarget(this.dialogAction, this.dialogValues) : '',
	);
	dialogConfirmMatches = $derived(
		this.dialogConfirm.trim() === this.dialogTarget && this.dialogTarget !== '',
	);
	dialogParams = $derived(this.dialogSpec?.params ?? []);
	dialogCanSubmit = $derived(!this.dialogNeedsConfirm || this.dialogConfirmMatches);
	selection = $derived(gitStore.selectionFor(this.panel));
	selectedFileEntry = $derived(
		gitStore.selectedFile
			? (this.files.find((f) => f.path === gitStore.selectedFile) ?? null)
			: null,
	);
	isSelectedConflicted = $derived(this.selectedFileEntry?.kind === 'conflicted');
	selectionSize = $derived(
		Object.values(this.lineSelection).reduce(
			(total, byHunk) => total + Object.values(byHunk).reduce((sum, rows) => sum + rows.length, 0),
			0,
		),
	);
	isMidOperation = $derived(
		Boolean(this.meta?.mergeHead || this.meta?.rebaseHead || this.meta?.cherryPickHead),
	);

	/** Attach the live subscription, do the initial load and focus the key input. */
	start(): void {
		this.stopLive = gitStore.subscribeLive(() => {
			if (this.loading) this.liveDirty = true;
			else void this.reload();
		});
		void this.reload();
		this.focusKeyInput();
	}

	/** Detach the live subscription. */
	stop(): void {
		this.stopLive?.();
		this.stopLive = null;
	}

	/**
	 * Visible row ids for a panel, derived from the panel's own filter and
	 * data. This is the single source for keyboard row navigation; rendering
	 * order in GitBoard must mirror it.
	 */
	rowIdsFor(panel: PanelId): string[] {
		const query = this.filters[panel];
		const match = (...values: string[]): boolean => values.some((value) => value.includes(query));
		switch (panel) {
			case 'files':
				return this.files.filter((file) => match(file.path)).map((file) => file.path);
			case 'conflicts':
				return this.files
					.filter((file) => file.kind === 'conflicted' && match(file.path))
					.map((file) => file.path);
			case 'worktrees':
				return this.worktrees
					.filter((entry) => match(entry.path, entry.branch))
					.map((entry) => entry.path);
			case 'submodules':
				return this.submodules
					.filter((entry) => match(entry.path, entry.commit))
					.map((entry) => entry.path);
			case 'branches':
				return this.branches.filter((branch) => match(branch.name)).map((branch) => branch.name);
			case 'remotes':
				return this.remotes
					.filter((remote) => match(remote.name, remote.url))
					.map((remote) => remote.name);
			case 'tags':
				return this.tags.filter((tag) => match(tag.name, tag.commit)).map((tag) => tag.name);
			case 'commits':
				return this.commits
					.filter((commit) => match(commit.subject, commit.shortId))
					.map((commit) => commit.id);
			case 'reflog':
				return this.reflogs
					.filter((entry) => match(entry.message, entry.shortId, entry.selector))
					.map((entry) => entry.selector);
			case 'stash':
				return this.stash
					.filter((entry) => match(entry.message, entry.name))
					.map((entry) => entry.name);
			default:
				return [];
		}
	}

	/** Currently selected row id for a panel, including panel-specific fallbacks. */
	selectedIdFor(panel: PanelId): string | null {
		if (panel === 'files' || panel === 'conflicts') return gitStore.selectedFile;
		if (panel === 'commits' || panel === 'reflog')
			return gitStore.selectedCommit ?? gitStore.selectionFor('commits');
		if (panel === 'remotes') return this.selectedRemote;
		if (panel === 'tags') return this.selectedTag;
		return gitStore.selectionFor(panel);
	}

	/** Apply the selection for a board row, routing to the right panel state. */
	async selectRow(panel: PanelId, id: string): Promise<void> {
		if (panel === 'files' || panel === 'conflicts') await this.selectFile(id, false);
		else if (panel === 'commits' || panel === 'reflog') await this.selectCommit(id, false);
		else if (panel === 'remotes') this.selectedRemote = id;
		else if (panel === 'tags') {
			this.selectedTag = id;
			const tag = this.tags.find((entry) => entry.name === id);
			if (tag) await this.selectCommit(tag.commit, false);
		} else gitStore.select(panel, id);
	}

	/** Move the panel selection by one visible row; returns the row id to focus. */
	async stepSelection(delta: number): Promise<string | null> {
		const ids = this.rowIdsFor(this.panel);
		if (ids.length === 0) return null;
		const current = this.selectedIdFor(this.panel);
		const index = current ? ids.indexOf(current) : -1;
		const next =
			index < 0
				? delta > 0
					? 0
					: ids.length - 1
				: Math.min(ids.length - 1, Math.max(0, index + delta));
		const id = ids[next];
		await this.selectRow(this.panel, id);
		return id;
	}

	/** Cycle the active panel through the canonical order; returns the new panel. */
	cyclePanel(delta: number): PanelId {
		const index = PANEL_ORDER.indexOf(this.panel);
		const next = index < 0 ? 0 : (index + delta + PANEL_ORDER.length) % PANEL_ORDER.length;
		this.panel = PANEL_ORDER[next];
		return PANEL_ORDER[next];
	}

	specFor(action: string): RegistryEntry | undefined {
		return this.registry.find((entry) => entry.id === action);
	}

	setViewMode(mode: MainViewMode): void {
		if (!isViewModeValidForPanel(mode, this.panel)) return;
		if (this.viewMode === mode) return;
		this.viewMode = mode;
		this.clearSelection();
		if (mode === 'staging' && gitStore.selectedFile) {
			void this.refreshStagingForFile(gitStore.selectedFile);
		}
		if (mode === 'diff' && this.panel === 'commits' && gitStore.selectedCommit) {
			void this.showCommitDiff(gitStore.selectedCommit);
		}
		if (mode === 'commitFiles' && gitStore.selectedCommit) {
			void this.selectCommit(gitStore.selectedCommit, false);
		}
	}

	focusFilter(): void {
		const scoped = document.getElementById(`git-filter-${this.panel}`);
		if (scoped) {
			scoped.focus();
			return;
		}
		const generic = Array.from(
			document.querySelectorAll<HTMLInputElement>('input[id^="git-filter"]'),
		);
		const visible = generic.find((element) => element.offsetParent !== null) ?? generic[0];
		visible?.focus();
	}

	focusKeyInput(): void {
		document.getElementById('git-input')?.focus();
	}

	async reload(): Promise<void> {
		this.loading = true;
		this.loadError = null;
		try {
			this.meta = await getMeta().catch(() => null);
			if (!this.meta) {
				this.noRepo = true;
				return;
			}
			this.noRepo = false;
			const [f, b, c, rf, s, st, wt, sm, rm, tg] = await Promise.all([
				listFiles().catch(() => []),
				listBranches().catch(() => []),
				listCommits({ limit: 50 }).catch(() => []),
				listReflog().catch(() => []),
				listStash().catch(() => []),
				getStatus().catch(() => null),
				listWorktrees().catch(() => []),
				listSubmodules().catch(() => []),
				listRemotes().catch(() => []),
				listTags().catch(() => []),
			]);
			this.files = f;
			this.branches = b;
			this.commits = c;
			this.commitsOffset = c.length;
			this.commitsHasMore = c.length >= 50;
			this.reflogs = rf;
			this.stash = s;
			this.status = st;
			this.worktrees = wt;
			this.submodules = sm;
			this.remotes = rm;
			this.tags = tg;
			try {
				this.registry = await getRegistry();
				const drift = checkDrift(
					this.registry.filter((entry) => entry.available).map((entry) => entry.id),
				);
				if (drift.unknown.length > 0) {
					toasts.warning('Git bindings drifted', drift.unknown.slice(0, 3).join(', '));
				}
				if (drift.missing.length > 0) {
					toasts.warning('Git bindings incomplete', drift.missing.slice(0, 3).join(', '));
				}
			} catch {
				// Registry drift is advisory only.
			}
			if (gitStore.selectedFile) {
				const path = gitStore.selectedFile;
				if (this.viewMode === 'staging') {
					const [unstaged, staged] = await Promise.all([
						getDiff(path).catch(() => []),
						getStagedDiff(path).catch(() => []),
					]);
					this.diff = unstaged;
					this.stagedDiff = staged;
				} else if (this.panel !== 'commits') {
					this.diff = await getDiff(path).catch(() => []);
					this.stagedDiff = [];
				}
				this.clearSelection();
			}
			if (gitStore.selectedCommit) {
				this.commitFiles = await getCommitFiles(gitStore.selectedCommit).catch(() => []);
				if (this.viewMode === 'diff' && this.panel === 'commits') {
					await this.showCommitDiff(gitStore.selectedCommit, false);
				}
			}
		} catch (e) {
			this.loadError = e instanceof Error ? e.message : 'Git request failed';
		} finally {
			this.loading = false;
			if (this.liveDirty) {
				this.liveDirty = false;
				void this.reload();
			}
		}
	}

	async selectFile(path: string, focusInput = true): Promise<void> {
		gitStore.select('files', path);
		this.clearSelection();
		this.commitFileDiffPath = null;
		const request = ++this.diffRequest;
		if (this.viewMode === 'staging') {
			const [unstaged, staged] = await Promise.all([
				getDiff(path).catch(() => []),
				getStagedDiff(path).catch(() => []),
			]);
			if (request !== this.diffRequest) return;
			this.diff = unstaged;
			this.stagedDiff = staged;
		} else {
			const unstaged = await getDiff(path).catch(() => []);
			if (request !== this.diffRequest) return;
			this.diff = unstaged;
			this.stagedDiff = [];
		}
		if (focusInput) this.focusKeyInput();
	}

	async refreshStagingForFile(path: string): Promise<void> {
		const request = ++this.diffRequest;
		const [unstaged, staged] = await Promise.all([
			getDiff(path).catch(() => []),
			getStagedDiff(path).catch(() => []),
		]);
		if (request !== this.diffRequest) return;
		this.diff = unstaged;
		this.stagedDiff = staged;
	}

	async stageHunk(path: string, hunkIndex: number): Promise<void> {
		await this.dispatchAction('stage.hunk', { path, hunkIndex: String(hunkIndex) }, undefined);
	}

	async unstageHunk(path: string, hunkIndex: number): Promise<void> {
		await this.dispatchAction('unstage.hunk', { path, hunkIndex: String(hunkIndex) }, undefined);
	}

	async stageLines(path: string, hunkIndex: number, lines: number[]): Promise<void> {
		if (lines.length === 0) {
			toasts.error('Empty selection', 'Select at least one added or removed line');
			return;
		}
		await this.dispatchAction(
			'stage.lines',
			{ path, hunkIndex: String(hunkIndex), lines: [...lines].sort((a, b) => a - b).join(',') },
			undefined,
		);
	}

	async unstageLines(path: string, hunkIndex: number, lines: number[]): Promise<void> {
		if (lines.length === 0) {
			toasts.error('Empty selection', 'Select at least one added or removed line');
			return;
		}
		await this.dispatchAction(
			'unstage.lines',
			{ path, hunkIndex: String(hunkIndex), lines: [...lines].sort((a, b) => a - b).join(',') },
			undefined,
		);
	}

	async discardHunk(path: string, hunkIndex: number): Promise<void> {
		this.openParamDialog('discard.hunk', ['path', 'hunkIndex']);
		this.dialogValues = { path, hunkIndex: String(hunkIndex) };
		this.dialogConfirm = '';
	}

	async discardLines(path: string, hunkIndex: number, lines: number[]): Promise<void> {
		if (lines.length === 0) {
			toasts.error('Empty selection', 'Select at least one added or removed line');
			return;
		}
		this.openParamDialog('discard.lines', ['path', 'hunkIndex', 'lines']);
		this.dialogValues = {
			path,
			hunkIndex: String(hunkIndex),
			lines: [...lines].sort((a, b) => a - b).join(','),
		};
		this.dialogConfirm = '';
	}

	async acceptOurs(path: string): Promise<void> {
		await this.dispatchAction('conflict.accept-ours', { path }, undefined);
		this.clearSelection(path);
	}

	async acceptTheirs(path: string): Promise<void> {
		await this.dispatchAction('conflict.accept-theirs', { path }, undefined);
		this.clearSelection(path);
	}

	async resolveConflictBlock(path: string, block: number, side: 'ours' | 'theirs'): Promise<void> {
		await this.dispatchAction(
			'conflict.resolve-block',
			{ path, block: String(block), side },
			undefined,
		);
		this.clearSelection(path);
	}

	clearSelection(path?: string): void {
		if (!path) {
			this.lineSelection = {};
			return;
		}
		if (!(path in this.lineSelection)) return;
		const next = { ...this.lineSelection };
		delete next[path];
		this.lineSelection = next;
	}

	selectedForHunk(path: string, hunkIndex: number): number[] {
		return this.lineSelection[path]?.[hunkIndex] ?? [];
	}

	isLineSelected(path: string, hunkIndex: number, lineIndex: number): boolean {
		return this.selectedForHunk(path, hunkIndex).includes(lineIndex);
	}

	toggleLine(path: string, hunkIndex: number, lineIndex: number): void {
		const current = this.selectedForHunk(path, hunkIndex);
		const next = current.includes(lineIndex)
			? current.filter((v) => v !== lineIndex)
			: [...current, lineIndex].sort((a, b) => a - b);
		const byHunk = { ...(this.lineSelection[path] ?? {}) };
		if (next.length === 0) {
			delete byHunk[hunkIndex];
		} else {
			byHunk[hunkIndex] = next;
		}
		const all = { ...this.lineSelection };
		if (Object.keys(byHunk).length === 0) {
			delete all[path];
		} else {
			all[path] = byHunk;
		}
		this.lineSelection = all;
	}

	toggleHunk(path: string, hunkIndex: number, selectable: number[]): void {
		const current = this.selectedForHunk(path, hunkIndex);
		const allSelected = selectable.length > 0 && selectable.every((v) => current.includes(v));
		const byHunk = { ...(this.lineSelection[path] ?? {}) };
		if (allSelected) {
			delete byHunk[hunkIndex];
		} else {
			byHunk[hunkIndex] = [...selectable].sort((a, b) => a - b);
		}
		const all = { ...this.lineSelection };
		if (Object.keys(byHunk).length === 0) {
			delete all[path];
		} else {
			all[path] = byHunk;
		}
		this.lineSelection = all;
	}

	selectRange(
		path: string,
		hunkIndex: number,
		anchor: number,
		target: number,
		selectable: number[],
	): void {
		const lo = Math.min(anchor, target);
		const hi = Math.max(anchor, target);
		const inRange = selectable.filter((v) => v >= lo && v <= hi);
		if (inRange.length === 0) return;
		const current = new SvelteSet(this.selectedForHunk(path, hunkIndex));
		for (const v of inRange) current.add(v);
		const next = [...current].sort((a, b) => a - b);
		const byHunk = { ...(this.lineSelection[path] ?? {}) };
		byHunk[hunkIndex] = next;
		this.lineSelection = { ...this.lineSelection, [path]: byHunk };
	}

	selectAllForFile(path: string): void {
		const byHunk: Record<number, number[]> = {};
		const collect = (files: DiffFile[]) => {
			for (const file of files) {
				if (file.path !== path) continue;
				const hunks = hunksOfDiffFile(file);
				hunks.forEach((rows, hunkIndex) => {
					const selectable = selectableIndices(rows);
					if (selectable.length === 0) return;
					const existing = new SvelteSet([...(byHunk[hunkIndex] ?? []), ...selectable]);
					byHunk[hunkIndex] = [...existing].sort((a, b) => a - b);
				});
			}
		};
		collect(this.diff);
		collect(this.stagedDiff);
		if (Object.keys(byHunk).length === 0) return;
		this.lineSelection = { ...this.lineSelection, [path]: byHunk };
	}

	private hunksForAction(
		files: DiffFile[],
		path: string,
	): Map<number, { selectable: number[]; selected: number[] }> {
		const out = new SvelteMap<number, { selectable: number[]; selected: number[] }>();
		for (const file of files) {
			if (file.path !== path) continue;
			const hunks = hunksOfDiffFile(file);
			hunks.forEach((rows, hunkIndex) => {
				out.set(hunkIndex, {
					selectable: selectableIndices(rows),
					selected: this.selectedForHunk(path, hunkIndex),
				});
			});
		}
		return out;
	}

	async stageSelected(path: string): Promise<void> {
		const hunks = this.hunksForAction(this.diff, path);
		const jobs: Array<Promise<void>> = [];
		for (const [hunkIndex, info] of hunks) {
			const relevant = info.selected.filter((v) => info.selectable.includes(v));
			if (relevant.length === 0) continue;
			if (relevant.length === info.selectable.length) {
				jobs.push(
					this.dispatchAction(
						'stage.hunk',
						{ path, hunkIndex: String(hunkIndex) },
						undefined,
						false,
					),
				);
			} else {
				jobs.push(
					this.dispatchAction(
						'stage.lines',
						{
							path,
							hunkIndex: String(hunkIndex),
							lines: [...relevant].sort((a, b) => a - b).join(','),
						},
						undefined,
						false,
					),
				);
			}
		}
		if (jobs.length === 0) {
			toasts.error('Nothing to stage', 'Select added or removed lines in the unstaged view');
			return;
		}
		await Promise.all(jobs);
		await this.afterBatchAction();
	}

	async unstageSelected(path: string): Promise<void> {
		const hunks = this.hunksForAction(this.stagedDiff, path);
		const jobs: Array<Promise<void>> = [];
		for (const [hunkIndex, info] of hunks) {
			const relevant = info.selected.filter((v) => info.selectable.includes(v));
			if (relevant.length === 0) continue;
			if (relevant.length === info.selectable.length) {
				jobs.push(
					this.dispatchAction(
						'unstage.hunk',
						{ path, hunkIndex: String(hunkIndex) },
						undefined,
						false,
					),
				);
			} else {
				jobs.push(
					this.dispatchAction(
						'unstage.lines',
						{
							path,
							hunkIndex: String(hunkIndex),
							lines: [...relevant].sort((a, b) => a - b).join(','),
						},
						undefined,
						false,
					),
				);
			}
		}
		if (jobs.length === 0) {
			toasts.error('Nothing to unstage', 'Select added or removed lines in the staged view');
			return;
		}
		await Promise.all(jobs);
		await this.afterBatchAction();
	}

	private async afterBatchAction(): Promise<void> {
		this.clearSelection();
		await this.reload();
	}

	async selectCommit(id: string, focusInput = true): Promise<void> {
		gitStore.select('commits', id);
		this.diff = [];
		this.commitFileDiffPath = null;
		const request = ++this.diffRequest;
		this.commitFiles = await getCommitFiles(id).catch(() => []);
		if (request !== this.diffRequest) return;
		if (this.viewMode === 'diff') {
			await this.showCommitDiff(id, false);
		}
		if (focusInput) this.focusKeyInput();
	}

	async showCommitDiff(commit: string, focusInput = false): Promise<void> {
		const request = ++this.diffRequest;
		try {
			const diff = await getDiffRange(`${commit}^`, commit);
			if (request !== this.diffRequest) return;
			this.diff = diff;
			this.commitFileDiffPath = null;
		} catch (e) {
			toasts.error('Commit diff unavailable', e instanceof Error ? e.message : undefined);
		}
		if (focusInput) this.focusKeyInput();
	}

	async showCommitFileDiff(commit: string, path: string): Promise<void> {
		const request = ++this.diffRequest;
		try {
			const diff = await getDiffRange(`${commit}^`, commit, path);
			if (request !== this.diffRequest) return;
			this.diff = diff;
			this.commitFileDiffPath = path;
		} catch (e) {
			toasts.error('Commit diff unavailable', e instanceof Error ? e.message : undefined);
		}
	}

	async submitToken(): Promise<void> {
		const result = parseToken(this.token, this.panel);
		if (result.kind === 'help') {
			this.helpOpen = true;
			return;
		}
		if (result.kind === 'command') {
			this.token = '';
			this.terminalOpen = true;
			this.layoutFocus = 'terminal';
			toasts.info(
				'Terminal opened',
				`Run "${result.preview ?? ''}" directly in the repository terminal below.`,
			);
			return;
		}
		this.token = '';
		if (result.kind !== 'action' || !result.action) return;
		await this.runActionId(result.action);
	}

	async runActionId(action: string): Promise<void> {
		const spec = this.specFor(action);
		if (spec && !spec.available) {
			toasts.error('Action unavailable', spec.reason ?? action);
			return;
		}
		if (CLIENT_ONLY_ACTIONS.has(action)) {
			this.focusFilter();
			return;
		}
		if (spec && (spec.params.length > 0 || GUARDED_DANGERS.has(spec.danger))) {
			this.openParamDialog(action, spec.params);
			return;
		}
		await this.dispatchAction(action, {}, undefined);
	}

	paramDefault(action: string, key: string): string {
		if (key === 'path') {
			if (this.panel === 'submodules') return gitStore.selectedSubmodule ?? '';
			if (this.panel === 'worktrees') return gitStore.selectedWorktree ?? '';
			return gitStore.selectedFile ?? '';
		}
		if (key === 'index' && this.panel === 'stash') {
			const entry = this.stash.find((s) => s.name === gitStore.selectedStash);
			return entry ? String(entry.index) : '';
		}
		if (key === 'commit') return gitStore.selectedCommit ?? '';
		if (key === 'branch' && this.panel === 'branches') return gitStore.selectedBranch ?? '';
		if (key === 'from' && this.panel === 'branches') return gitStore.selectedBranch ?? '';
		if (key === 'remote') return this.selectedRemote ?? PARAM_DEFAULTS[action]?.[key] ?? '';
		if (key === 'name' && action !== 'remote.add') return this.selectedRemote ?? '';
		if (key === 'tag') return this.selectedTag ?? '';
		return PARAM_DEFAULTS[action]?.[key] ?? '';
	}

	openParamDialog(action: string, params: string[]): void {
		const values: Record<string, string> = {};
		for (const key of params) {
			values[key] = this.paramDefault(action, key);
		}
		this.dialogAction = action;
		this.dialogValues = values;
		this.dialogConfirm = '';
		this.dialogOpen = true;
	}

	async confirmDialog(): Promise<void> {
		const action = this.dialogAction;
		if (!action) return;
		const params: Record<string, string> = {};
		for (const [key, value] of Object.entries(this.dialogValues)) {
			const trimmed = value.trim();
			if (trimmed) params[key] = trimmed;
		}
		let confirm: string | undefined;
		if (this.dialogNeedsConfirm) {
			if (!this.dialogCanSubmit) {
				toasts.error('Confirmation mismatch', `Type the exact target: ${this.dialogTarget}`);
				return;
			}
			confirm = this.dialogTarget;
		}
		this.dialogAction = null;
		this.dialogOpen = false;
		await this.dispatchAction(action, params, confirm);
	}

	async dispatchAction(
		action: string,
		params: Record<string, string>,
		confirm?: string,
		reload = true,
	): Promise<void> {
		const spec = this.specFor(action);
		if (spec && !spec.available) {
			toasts.error('Action unavailable', spec.reason ?? action);
			return;
		}
		if (spec?.params.includes('path') && !params.path) {
			const selected =
				this.panel === 'submodules'
					? gitStore.selectedSubmodule
					: this.panel === 'worktrees'
						? gitStore.selectedWorktree
						: gitStore.selectedFile;
			if (!selected) {
				toasts.error('No file selected', 'Select a file in the Files panel first');
				return;
			}
			params = { ...params, path: selected };
		}
		gitStore.beginAction(action);
		try {
			const outcome = await runAction(action, params, confirm);
			this.reportOutcome(action, spec, outcome.message || action, outcome.echo, params);
			if (reload) {
				this.clearSelection(params.path);
				await this.reload();
			}
		} catch (e) {
			const message = e instanceof Error ? e.message : 'Git action failed';
			this.pushActionLog(action, params, `failed: ${message}`);
			gitStore.endAction(message);
			toasts.push({
				title: 'Git action failed',
				description: message,
				tone: 'danger',
				timeout: 8000,
				action: {
					label: 'Open terminal',
					run: () => {
						this.terminalOpen = true;
						this.layoutFocus = 'terminal';
					},
				},
			});
			return;
		}
		gitStore.endAction();
	}

	/**
	 * Success feedback for a completed write action. History-moving
	 * recoverable actions get an 8-second toast with an Undo entry backed
	 * by the reflog undo stack; undo itself offers Redo in the same window.
	 * Staging moves offer the reverse direction instead of history undo.
	 * Every outcome is also appended to the bounded command log.
	 */
	private reportOutcome(
		action: string,
		spec: RegistryEntry | undefined,
		message: string,
		echo?: string,
		params?: Record<string, string>,
	): void {
		this.pushActionLog(action, params ?? {}, echo ? `${message} - ${echo}` : message);
		if (action === 'undo') {
			toasts.push({
				title: message,
				description: echo,
				tone: 'success',
				timeout: 8000,
				action: {
					label: 'Redo',
					run: () => {
						void this.runActionId('redo');
					},
				},
			});
			return;
		}
		if (action === 'redo') {
			toasts.success(message, echo);
			return;
		}
		const reverse = STAGING_REVERSE[action];
		if (reverse && params?.path) {
			const reverseParams = { ...params };
			toasts.push({
				title: message,
				description: echo,
				tone: 'success',
				timeout: 8000,
				action: {
					label: 'Reverse',
					run: () => {
						void this.dispatchAction(reverse, reverseParams, undefined);
					},
				},
			});
			return;
		}
		if (spec?.danger === 'recoverable' && UNDO_TOAST_ACTIONS.has(action)) {
			toasts.push({
				title: message,
				description: echo,
				tone: 'success',
				timeout: 8000,
				action: {
					label: 'Undo',
					run: () => {
						void this.runActionId('undo');
					},
				},
			});
			return;
		}
		toasts.success(message, echo);
	}

	private pushActionLog(action: string, params: Record<string, string>, result: string): void {
		const summary = Object.entries(params ?? {})
			.filter(([, value]) => value !== '')
			.map(([key, value]) => `${key}=${value}`)
			.join(' ');
		this.actionSeq += 1;
		this.actionLog = [...this.actionLog, { seq: this.actionSeq, action, summary, result }].slice(
			-8,
		);
	}

	async toggleTerminal(): Promise<void> {
		this.terminalOpen = !this.terminalOpen;
		this.layoutFocus = this.terminalOpen ? 'terminal' : 'none';
	}

	async loadMoreCommits(): Promise<void> {
		if (this.commitsLoading || !this.commitsHasMore) return;
		this.commitsLoading = true;
		try {
			const more = await listCommits({
				limit: 50,
				offset: this.commitsOffset,
			});
			this.commits = [...this.commits, ...more];
			this.commitsOffset += more.length;
			this.commitsHasMore = more.length >= 50;
		} catch (e) {
			toasts.error('More commits unavailable', e instanceof Error ? e.message : undefined);
		} finally {
			this.commitsLoading = false;
		}
	}
}
