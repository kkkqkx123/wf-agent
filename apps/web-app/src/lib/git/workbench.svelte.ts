import { SvelteSet } from 'svelte/reactivity';
import { toasts } from '$lib/stores/toast.svelte';
import { git as gitStore } from '$lib/stores/git.svelte';
import {
	getCommitFiles,
	getDiff,
	getDiffRange,
	getMeta,
	getRegistry,
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
} from '$lib/services/git';
import { checkDrift, visibleControls } from '$lib/git/bindings';
import type { DiffFile } from '$lib/types/diff';
import { CLIENT_ONLY_ACTIONS, confirmTarget, parseToken } from '$lib/git/parse';

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
};

export const PARAM_DEFAULTS: Record<string, Record<string, string>> = {
	push: { remote: 'origin' },
	pull: { remote: 'origin' },
};

export const GUARDED_DANGERS = new SvelteSet([
	'destructive',
	'history_rewrite',
]);

export class GitWorkbench {
	panel = $state<PanelId>('status');
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
	diffSource = $state<'unstaged' | 'commit'>('unstaged');
	commitFiles = $state<CommitFileEntry[]>([]);
	remotes = $state<RemoteEntry[]>([]);
	tags = $state<TagEntry[]>([]);
	worktrees = $state<WorktreeEntry[]>([]);
	submodules = $state<SubmoduleEntry[]>([]);
	loading = $state(false);
	loadError = $state<string | null>(null);
	noRepo = $state(false);
	liveDirty = $state(false);
	private stopLive: (() => void) | null = null;

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

	dialogAction = $state<string | null>(null);
	dialogOpen = $state(false);
	dialogValues = $state<Record<string, string>>({});
	dialogConfirm = $state('');
	helpOpen = $state(false);

	tokenResult = $derived(parseToken(this.token, this.panel));
	controls = $derived(visibleControls(this.panel));
	filterQuery = $derived(this.filters[this.panel]);
	visibleFiles = $derived(
		this.files.filter((f) => f.path.includes(this.filterQuery)),
	);
	visibleConflicts = $derived(
		this.files
			.filter((f) => f.kind === 'conflicted')
			.filter((f) => f.path.includes(this.filterQuery)),
	);
	visibleBranches = $derived(
		this.branches.filter((b) => b.name.includes(this.filterQuery)),
	);
	visibleRemotes = $derived(
		this.remotes.filter((r) =>
			r.name.includes(this.filterQuery) || r.url.includes(this.filterQuery),
		),
	);
	visibleTags = $derived(
		this.tags.filter((t) =>
			t.name.includes(this.filterQuery) || t.commit.includes(this.filterQuery),
		),
	);
	visibleCommits = $derived(
		this.commits.filter(
			(c) =>
				c.subject.includes(this.filterQuery) ||
				c.shortId.includes(this.filterQuery),
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
			(s) =>
				s.message.includes(this.filterQuery) ||
				s.name.includes(this.filterQuery),
		),
	);
	visibleWorktrees = $derived(
		this.worktrees.filter(
			(w) =>
				w.path.includes(this.filterQuery) ||
				w.branch.includes(this.filterQuery),
		),
	);
	visibleSubmodules = $derived(
		this.submodules.filter(
			(s) =>
				s.path.includes(this.filterQuery) ||
				s.commit.includes(this.filterQuery),
		),
	);
	dialogSpec = $derived(
		this.dialogAction
			? this.registry.find((entry) => entry.id === this.dialogAction)
			: undefined,
	);
	dialogNeedsConfirm = $derived(
		this.dialogSpec ? GUARDED_DANGERS.has(this.dialogSpec.danger) : false,
	);
	dialogTarget = $derived(
		this.dialogAction
			? confirmTarget(this.dialogAction, this.dialogValues)
			: '',
	);
	dialogConfirmMatches = $derived(
		this.dialogConfirm.trim() === this.dialogTarget && this.dialogTarget !== '',
	);
	dialogParams = $derived(this.dialogSpec?.params ?? []);
	dialogCanSubmit = $derived(
		!this.dialogNeedsConfirm || this.dialogConfirmMatches,
	);
	selection = $derived(gitStore.selectionFor(this.panel));

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

	specFor(action: string): RegistryEntry | undefined {
		return this.registry.find((entry) => entry.id === action);
	}

	focusFilter(): void {
		document.getElementById('git-filter')?.focus();
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
					this.registry
						.filter((entry) => entry.available)
						.map((entry) => entry.id),
				);
				if (drift.unknown.length > 0) {
					toasts.warning(
						'Git bindings drifted',
						drift.unknown.slice(0, 3).join(', '),
					);
				}
				if (drift.missing.length > 0) {
					toasts.warning(
						'Git bindings incomplete',
						drift.missing.slice(0, 3).join(', '),
					);
				}
			} catch {
				// Registry drift is advisory only.
			}
			if (gitStore.selectedFile) {
				this.diff = await getDiff(gitStore.selectedFile).catch(() => []);
				this.diffSource = 'unstaged';
			}
			if (gitStore.selectedCommit) {
				this.commitFiles = await getCommitFiles(gitStore.selectedCommit).catch(
					() => [],
				);
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

	async selectFile(path: string): Promise<void> {
		gitStore.select('files', path);
		this.diff = await getDiff(path).catch(() => []);
		this.diffSource = 'unstaged';
		this.focusKeyInput();
	}

	async stageHunk(path: string, hunkIndex: number): Promise<void> {
		await this.dispatchAction(
			'stage.hunk',
			{ path, hunkIndex: String(hunkIndex) },
			undefined,
		);
	}

	async selectCommit(id: string): Promise<void> {
		gitStore.select('commits', id);
		this.diff = [];
		this.commitFiles = await getCommitFiles(id).catch(() => []);
		this.focusKeyInput();
	}

	async showCommitFileDiff(commit: string, path: string): Promise<void> {
		this.diffSource = 'commit';
		try {
			this.diff = await getDiffRange(`${commit}^`, commit, path);
		} catch (e) {
			toasts.error(
				'Commit diff unavailable',
				e instanceof Error ? e.message : undefined,
			);
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
			toasts.info(
				'Open the Terminal panel to run commands directly in the repository.',
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
		if (key === 'branch' && this.panel === 'branches')
			return gitStore.selectedBranch ?? '';
		if (key === 'from' && this.panel === 'branches')
			return gitStore.selectedBranch ?? '';
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
				toasts.error(
					'Confirmation mismatch',
					`Type the exact target: ${this.dialogTarget}`,
				);
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
				toasts.error(
					'No file selected',
					'Select a file in the Files panel first',
				);
				return;
			}
			params = { ...params, path: selected };
		}
		gitStore.beginAction(action);
		try {
			const outcome = await runAction(action, params, confirm);
			toasts.success(outcome.message || action, outcome.echo);
			await this.reload();
		} catch (e) {
			const message = e instanceof Error ? e.message : 'Git action failed';
			gitStore.endAction(message);
			toasts.error('Git action failed', message);
			return;
		}
		gitStore.endAction();
	}

	async toggleTerminal(): Promise<void> {
		this.terminalOpen = !this.terminalOpen;
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
			toasts.error(
				'More commits unavailable',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			this.commitsLoading = false;
		}
	}
}
