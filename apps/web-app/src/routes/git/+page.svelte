<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/state';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { toasts } from '$lib/stores/toast.svelte';
	import { git as gitStore } from '$lib/stores/git.svelte';
	import {
		closeTerminal,
		createTerminal,
		getCommitFiles,
		getDiff,
		getDiffRange,
		getFingerprint,
		getMeta,
		getRegistry,
		getStatus,
		listBranches,
		listCommits,
		listFiles,
		listRemotes,
		listStash,
		listSubmodules,
		listTags,
		listWorktrees,
		readTerminalOutput,
		resizeTerminal,
		runAction,
		sendTerminalInput,
		type BranchEntry,
		type CommitEntry,
		type CommitFileEntry,
		type DiffFile,
		type GitFile,
		type RegistryEntry,
		type RemoteEntry,
		type RepoMeta,
		type StashEntry,
		type StatusCounts,
		type SubmoduleEntry,
		type TagEntry,
		type WorktreeEntry,
	} from '$lib/services/git';
	import { bindingsFor, checkDrift, visibleControls } from '$lib/git/bindings';
	import {
		CLIENT_ONLY_ACTIONS,
		confirmTarget,
		parseToken,
	} from '$lib/git/parse';

	const PANELS = [
		{ id: 'status', label: 'Status' },
		{ id: 'files', label: 'Files' },
		{ id: 'branches', label: 'Branches' },
		{ id: 'commits', label: 'Commits' },
		{ id: 'stash', label: 'Stash' },
	] as const;

	type PanelId = (typeof PANELS)[number]['id'];

	const PARAM_LABELS: Record<string, string> = {
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

	const PARAM_DEFAULTS: Record<string, Record<string, string>> = {
		push: { remote: 'origin' },
		pull: { remote: 'origin' },
	};

	const GUARDED_DANGERS = new Set(['destructive', 'history_rewrite']);

	const requested = parseListParams(page.url).panel;
	let panel = $state<PanelId>(
		requested === 'files' ||
			requested === 'branches' ||
			requested === 'commits' ||
			requested === 'stash'
			? requested
			: 'status',
	);

	$effect(() => {
		void gotoWithParams(page.url, { panel: panel === 'status' ? '' : panel });
	});

	let meta = $state<RepoMeta | null>(null);
	let status = $state<StatusCounts | null>(null);
	let files = $state<GitFile[]>([]);
	let branches = $state<BranchEntry[]>([]);
	let commits = $state<CommitEntry[]>([]);
	let commitsOffset = $state(0);
	let commitsHasMore = $state(false);
	let commitsLoading = $state(false);
	let stash = $state<StashEntry[]>([]);
	let diff = $state<DiffFile[]>([]);
	let diffSource = $state<'unstaged' | 'commit'>('unstaged');
	let commitFiles = $state<CommitFileEntry[]>([]);
	let remotes = $state<RemoteEntry[]>([]);
	let tags = $state<TagEntry[]>([]);
	let worktrees = $state<WorktreeEntry[]>([]);
	let submodules = $state<SubmoduleEntry[]>([]);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let noRepo = $state(false);
	let liveDirty = $state(false);
	let stopLive: (() => void) | null = null;

	let registry = $state<RegistryEntry[]>([]);
	let filters = $state<Record<PanelId, string>>({
		status: '',
		files: '',
		branches: '',
		commits: '',
		stash: '',
	});

	let token = $state('');
	let terminalOpen = $state(false);
	let terminalId = $state<string | null>(null);
	let terminalText = $state('');
	let terminalInput = $state('');
	let terminalOffset = $state(0);

	let dialogAction = $state<string | null>(null);
	let dialogOpen = $state(false);
	let dialogValues = $state<Record<string, string>>({});
	let dialogConfirm = $state('');
	let helpOpen = $state(false);

	const tokenResult = $derived(parseToken(token, panel));
	const controls = $derived(visibleControls(panel));
	const filterQuery = $derived(filters[panel]);
	const visibleFiles = $derived(
		files.filter((f) => f.path.includes(filterQuery)),
	);
	const visibleBranches = $derived(
		branches.filter((b) => b.name.includes(filterQuery)),
	);
	const visibleCommits = $derived(
		commits.filter(
			(c) => c.subject.includes(filterQuery) || c.shortId.includes(filterQuery),
		),
	);
	const visibleStash = $derived(
		stash.filter(
			(s) => s.message.includes(filterQuery) || s.name.includes(filterQuery),
		),
	);
	const dialogSpec = $derived(
		dialogAction
			? registry.find((entry) => entry.id === dialogAction)
			: undefined,
	);
	const dialogNeedsConfirm = $derived(
		dialogSpec ? GUARDED_DANGERS.has(dialogSpec.danger) : false,
	);
	const dialogTarget = $derived(
		dialogAction ? confirmTarget(dialogAction, dialogValues) : '',
	);
	const dialogConfirmMatches = $derived(
		dialogConfirm.trim() === dialogTarget && dialogTarget !== '',
	);
	const dialogParams = $derived(dialogSpec?.params ?? []);
	const dialogCanSubmit = $derived(!dialogNeedsConfirm || dialogConfirmMatches);
	const selection = $derived(gitStore.selectionFor(panel));

	function specFor(action: string): RegistryEntry | undefined {
		return registry.find((entry) => entry.id === action);
	}

	function focusFilter(): void {
		document.getElementById('git-filter')?.focus();
	}

	function focusKeyInput(): void {
		document.getElementById('git-input')?.focus();
	}

	onMount(() => {
		stopLive = gitStore.subscribeLive(() => {
			if (loading) liveDirty = true;
			else void reload();
		});
		void reload();
		focusKeyInput();
	});

	onDestroy(() => {
		stopLive?.();
		if (terminalId) void closeTerminal(terminalId).catch(() => {});
	});

	async function reload(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			meta = await getMeta().catch(() => null);
			if (!meta) {
				noRepo = true;
				return;
			}
			noRepo = false;
			const [f, b, c, s, st, wt, sm] = await Promise.all([
				listFiles().catch(() => []),
				listBranches().catch(() => []),
				listCommits({ limit: 50 }).catch(() => []),
				listStash().catch(() => []),
				getStatus().catch(() => null),
				listWorktrees().catch(() => []),
				listSubmodules().catch(() => []),
			]);
			files = f;
			branches = b;
			commits = c;
			commitsOffset = c.length;
			commitsHasMore = c.length >= 50;
			stash = s;
			status = st;
			worktrees = wt;
			submodules = sm;
			if (panel === 'branches') {
				const [rm, tg] = await Promise.all([
					listRemotes().catch(() => []),
					listTags().catch(() => []),
				]);
				remotes = rm;
				tags = tg;
			}
			try {
				registry = await getRegistry();
				const drift = checkDrift(
					registry.filter((entry) => entry.available).map((entry) => entry.id),
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
				diff = await getDiff(gitStore.selectedFile).catch(() => []);
				diffSource = 'unstaged';
			}
			if (gitStore.selectedCommit) {
				commitFiles = await getCommitFiles(gitStore.selectedCommit).catch(
					() => [],
				);
			}
		} catch (e) {
			loadError = e instanceof Error ? e.message : 'Git request failed';
		} finally {
			loading = false;
			if (liveDirty) {
				liveDirty = false;
				void reload();
			}
		}
	}

	async function selectFile(path: string): Promise<void> {
		gitStore.select('files', path);
		diff = await getDiff(path).catch(() => []);
		diffSource = 'unstaged';
		focusKeyInput();
	}

	async function selectCommit(id: string): Promise<void> {
		gitStore.select('commits', id);
		diff = [];
		commitFiles = await getCommitFiles(id).catch(() => []);
		focusKeyInput();
	}

	async function showCommitFileDiff(
		commit: string,
		path: string,
	): Promise<void> {
		diffSource = 'commit';
		try {
			diff = await getDiffRange(`${commit}^`, commit, path);
		} catch (e) {
			toasts.error(
				'Commit diff unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function submitToken(): Promise<void> {
		const result = parseToken(token, panel);
		if (result.kind === 'help') {
			helpOpen = true;
			return;
		}
		if (result.kind === 'command') {
			token = '';
			if (!terminalOpen) await toggleTerminal();
			if (result.preview) {
				terminalInput = result.preview;
				await sendTerminal();
			}
			return;
		}
		token = '';
		if (result.kind !== 'action' || !result.action) return;
		await runActionId(result.action);
	}

	async function runActionId(action: string): Promise<void> {
		const spec = specFor(action);
		if (spec && !spec.available) {
			toasts.error('Action unavailable', spec.reason ?? action);
			return;
		}
		if (CLIENT_ONLY_ACTIONS.has(action)) {
			focusFilter();
			return;
		}
		if (spec && (spec.params.length > 0 || GUARDED_DANGERS.has(spec.danger))) {
			openParamDialog(action, spec.params);
			return;
		}
		await dispatchAction(action, {}, undefined);
	}

	function paramDefault(action: string, key: string): string {
		if (key === 'path') return gitStore.selectedFile ?? '';
		if (key === 'index' && panel === 'stash') {
			const entry = stash.find((s) => s.name === gitStore.selectedStash);
			return entry ? String(entry.index) : '';
		}
		if (key === 'commit') return gitStore.selectedCommit ?? '';
		if (key === 'branch' && panel === 'branches')
			return gitStore.selectedBranch ?? '';
		if (key === 'from' && panel === 'branches')
			return gitStore.selectedBranch ?? '';
		return PARAM_DEFAULTS[action]?.[key] ?? '';
	}

	function openParamDialog(action: string, params: string[]): void {
		const values: Record<string, string> = {};
		for (const key of params) {
			values[key] = paramDefault(action, key);
		}
		dialogAction = action;
		dialogValues = values;
		dialogConfirm = '';
		dialogOpen = true;
	}

	async function confirmDialog(): Promise<void> {
		const action = dialogAction;
		if (!action) return;
		const params: Record<string, string> = {};
		for (const [key, value] of Object.entries(dialogValues)) {
			const trimmed = value.trim();
			if (trimmed) params[key] = trimmed;
		}
		let confirm: string | undefined;
		if (dialogNeedsConfirm) {
			if (!dialogCanSubmit) {
				toasts.error(
					'Confirmation mismatch',
					`Type the exact target: ${dialogTarget}`,
				);
				return;
			}
			confirm = dialogTarget;
		}
		dialogAction = null;
		dialogOpen = false;
		await dispatchAction(action, params, confirm);
	}

	async function dispatchAction(
		action: string,
		params: Record<string, string>,
		confirm?: string,
	): Promise<void> {
		const spec = specFor(action);
		if (spec && !spec.available) {
			toasts.error('Action unavailable', spec.reason ?? action);
			return;
		}
		if (spec?.params.includes('path') && !params.path) {
			const selected = gitStore.selectedFile;
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
			// Echoing the equivalent command into an open terminal teaches
			// the CLI form of the action that just ran.
			if (terminalOpen && terminalId && outcome.echo) {
				terminalText += `> ${outcome.echo}\n`;
			}
			await reload();
		} catch (e) {
			const message = e instanceof Error ? e.message : 'Git action failed';
			gitStore.endAction(message);
			toasts.error('Git action failed', message);
			return;
		}
		gitStore.endAction();
	}

	async function toggleTerminal(): Promise<void> {
		terminalOpen = !terminalOpen;
		if (terminalOpen && !terminalId) {
			try {
				const session = await createTerminal();
				terminalId = session.id;
				terminalText = '';
				terminalOffset = 0;
				await resizeTerminal(session.id, 24, 120).catch(() => {});
			} catch (e) {
				toasts.error(
					'Terminal unavailable',
					e instanceof Error ? e.message : undefined,
				);
				terminalOpen = false;
			}
		}
		if (!terminalOpen && terminalId) {
			const id = terminalId;
			terminalId = null;
			await closeTerminal(id).catch(() => {});
		}
	}

	function clearTerminal(): void {
		// Clearing drops the display buffer only: the server-side output
		// cursor stays where it is, so later slices keep appending.
		terminalText = '';
	}

	async function restartTerminal(): Promise<void> {
		const previous = terminalId;
		terminalId = null;
		terminalText = '';
		terminalOffset = 0;
		if (previous) await closeTerminal(previous).catch(() => {});
		try {
			const session = await createTerminal();
			terminalId = session.id;
			await resizeTerminal(session.id, 24, 120).catch(() => {});
		} catch (e) {
			toasts.error(
				'Terminal unavailable',
				e instanceof Error ? e.message : undefined,
			);
			terminalOpen = false;
		}
	}

	async function loadMoreCommits(): Promise<void> {
		if (commitsLoading || !commitsHasMore) return;
		commitsLoading = true;
		try {
			const more = await listCommits({ limit: 50, offset: commitsOffset });
			commits = [...commits, ...more];
			commitsOffset += more.length;
			commitsHasMore = more.length >= 50;
		} catch (e) {
			toasts.error(
				'More commits unavailable',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			commitsLoading = false;
		}
	}

	async function sendTerminal(): Promise<void> {
		if (!terminalId || !terminalInput.trim()) return;
		const input = terminalInput;
		terminalInput = '';
		try {
			const before = await getFingerprint().catch(() => null);
			await sendTerminalInput(terminalId, input);
			const output = await readTerminalOutput(terminalId, terminalOffset);
			terminalText += output;
			terminalOffset += output.length;
			const after = await getFingerprint().catch(() => null);
			const key = (
				fp: { statusHash: string; head: string; branchSummary: string } | null,
			) => (fp ? `${fp.statusHash}:${fp.head}:${fp.branchSummary}` : '');
			if (key(before) !== key(after)) {
				await reload();
			}
		} catch (e) {
			toasts.error(
				'Terminal input failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Git"
		description="User repository workbench: status, files, branches, commits and stash."
	>
		{#snippet actions()}
			{#if meta}
				<Badge variant="neutral">{meta.branch || '(no branch)'}</Badge>
				{#if meta.upstream}
					<Badge variant="neutral"
						>{meta.upstream} +{meta.ahead}/-{meta.behind}</Badge
					>
				{/if}
				{#if meta.mergeHead || meta.rebaseHead || meta.cherryPickHead}
					<Badge variant="warning">mid-operation</Badge>
				{/if}
			{/if}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button size="sm" variant="outline" onclick={() => void toggleTerminal()}>
				{terminalOpen ? 'Hide terminal' : 'Terminal'}
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={PANELS.map((p) => ({ id: p.id, label: p.label }))}
		bind:value={panel}
		class="px-4"
		panelId="git-panel"
	/>

	<div
		id="git-panel"
		role="tabpanel"
		aria-label="Git panels"
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		<PageState
			{loading}
			error={loadError}
			empty={noRepo}
			emptyTitle="No repository"
			emptyDescription="Configure user_git.repo_root with user_git.enabled=true to attach the user repository."
			onretry={() => void reload()}
		>
			<div class="grid gap-3 xl:grid-cols-[minmax(0,280px)_minmax(0,1fr)]">
				<Card
					title="Groups"
					description="Status, files, branches, commits, stash"
				>
					<div class="flex flex-col gap-1">
						{#each PANELS as item (item.id)}
							<button
								type="button"
								onclick={() => {
									panel = item.id;
									focusKeyInput();
								}}
								aria-pressed={panel === item.id}
								class="rounded-md px-2 py-1.5 text-left text-caption hover:bg-accent/50 {panel ===
								item.id
									? 'bg-accent/70'
									: ''}"
							>
								{item.label}
							</button>
						{/each}
					</div>
					{#if panel !== 'status'}
						<Input
							id="git-filter"
							label="Filter"
							size="sm"
							bind:value={filters[panel]}
							placeholder="Filter this panel"
						/>
					{/if}
					{#if panel === 'status' && status}
						<ul class="mt-2 space-y-1 text-caption">
							<li class="flex justify-between">
								<span>Staged</span><span class="font-mono">{status.staged}</span
								>
							</li>
							<li class="flex justify-between">
								<span>Unstaged</span><span class="font-mono"
									>{status.unstaged}</span
								>
							</li>
							<li class="flex justify-between">
								<span>Untracked</span><span class="font-mono"
									>{status.untracked}</span
								>
							</li>
							<li class="flex justify-between">
								<span>Conflicted</span>
								<span
									class="font-mono {status.conflicted > 0
										? 'text-destructive'
										: ''}">{status.conflicted}</span
								>
							</li>
						</ul>
					{:else if panel === 'files'}
						<ul class="mt-2 divide-y divide-border">
							{#each visibleFiles as file (file.path)}
								<li>
									<button
										type="button"
										onclick={() => void selectFile(file.path)}
										aria-pressed={gitStore.selectionFor('files') === file.path}
										class="flex w-full items-center justify-between gap-2 px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
											'files',
										) === file.path
											? 'bg-accent/70'
											: ''}"
									>
										<span class="truncate font-mono text-caption"
											>{file.path}</span
										>
										<span class="font-mono text-micro text-muted-foreground"
											>{file.staged}{file.unstaged}</span
										>
									</button>
								</li>
							{/each}
						</ul>
						{#if visibleFiles.length === 0}
							<p class="mt-2 text-caption text-muted-foreground">
								No files match the filter.
							</p>
						{/if}
						{#if worktrees.length > 0 || submodules.length > 0}
							<div class="mt-3 space-y-2 border-t border-border pt-2">
								{#if worktrees.length > 0}
									<p class="text-micro font-medium text-muted-foreground">
										Worktrees (read-only)
									</p>
									<ul class="space-y-0.5">
										{#each worktrees as worktree (worktree.path)}
											<li
												class="truncate font-mono text-micro text-muted-foreground"
												title={worktree.path}
											>
												{worktree.path}{worktree.branch
													? ` · ${worktree.branch}`
													: ''}{worktree.bare ? ' · bare' : ''}
											</li>
										{/each}
									</ul>
								{/if}
								{#if submodules.length > 0}
									<p class="text-micro font-medium text-muted-foreground">
										Submodules (read-only)
									</p>
									<ul class="space-y-0.5">
										{#each submodules as submodule (submodule.path)}
											<li
												class="truncate font-mono text-micro text-muted-foreground"
												title={submodule.path}
											>
												{submodule.path} · {submodule.commit}
											</li>
										{/each}
									</ul>
								{/if}
								<p class="text-micro text-muted-foreground">
									Worktree and submodule writes are unavailable here; use the
									terminal. Lists refresh with the page.
								</p>
							</div>
						{/if}
					{:else if panel === 'branches'}
						<ul class="mt-2 space-y-1">
							{#each visibleBranches as branch (branch.name)}
								<li>
									<button
										type="button"
										onclick={() => gitStore.select('branches', branch.name)}
										aria-pressed={gitStore.selectionFor('branches') ===
											branch.name}
										class="flex w-full items-center justify-between gap-2 px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
											'branches',
										) === branch.name
											? 'bg-accent/70'
											: ''}"
									>
										<span class="truncate font-mono"
											>{branch.current ? '*' : ' '} {branch.name}</span
										>
										{#if branch.upstream}
											<span class="font-mono text-micro text-muted-foreground"
												>{branch.upstream} +{branch.ahead}/-{branch.behind}</span
											>
										{:else}
											<span class="font-mono text-micro text-muted-foreground"
												>{branch.subject.slice(0, 24)}</span
											>
										{/if}
									</button>
								</li>
							{/each}
						</ul>
						{#if visibleBranches.length === 0}
							<p class="mt-2 text-caption text-muted-foreground">
								No branches match the filter.
							</p>
						{/if}
						{#if panel === 'branches' && (remotes.length > 0 || tags.length > 0)}
							<div class="mt-3 space-y-2 border-t border-border pt-2">
								{#if remotes.length > 0}
									<p class="text-micro font-medium text-muted-foreground">
										Remotes
									</p>
									<ul class="space-y-0.5">
										{#each remotes as remote (remote.name)}
											<li
												class="truncate font-mono text-micro text-muted-foreground"
												title={remote.url}
											>
												{remote.name} → {remote.url}
											</li>
										{/each}
									</ul>
								{/if}
								{#if tags.length > 0}
									<p class="text-micro font-medium text-muted-foreground">
										Tags
									</p>
									<ul class="space-y-0.5">
										{#each tags as tag (tag.name)}
											<li
												class="truncate font-mono text-micro text-muted-foreground"
												title={tag.message}
											>
												{tag.name}
											</li>
										{/each}
									</ul>
								{/if}
							</div>
						{/if}
					{:else if panel === 'commits'}
						<ul class="mt-2 space-y-1">
							{#each visibleCommits as commit (commit.id)}
								<li>
									<button
										type="button"
										onclick={() => void selectCommit(commit.id)}
										aria-pressed={gitStore.selectionFor('commits') ===
											commit.id}
										class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
											'commits',
										) === commit.id
											? 'bg-accent/70'
											: ''}"
									>
										<p class="truncate font-mono text-caption">
											{commit.shortId}
											{commit.subject}
										</p>
										<p class="text-micro text-muted-foreground">
											{commit.author}
										</p>
									</button>
								</li>
							{/each}
						</ul>
						{#if visibleCommits.length === 0}
							<p class="mt-2 text-caption text-muted-foreground">
								No commits match the filter.
							</p>
						{/if}
						{#if commitsHasMore && visibleCommits.length === commits.length}
							<Button
								variant="ghost"
								size="sm"
								disabled={commitsLoading}
								onclick={() => void loadMoreCommits()}
							>
								{commitsLoading
									? 'Loading…'
									: `Show more (${commits.length} shown)`}
							</Button>
						{/if}
					{:else if panel === 'stash'}
						<ul class="mt-2 space-y-1">
							{#each visibleStash as entry (entry.name)}
								<li>
									<button
										type="button"
										onclick={() => gitStore.select('stash', entry.name)}
										aria-pressed={gitStore.selectionFor('stash') === entry.name}
										class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
											'stash',
										) === entry.name
											? 'bg-accent/70'
											: ''}"
									>
										<span class="font-mono text-caption">{entry.name}</span>
										<span class="text-muted-foreground"> {entry.message}</span>
									</button>
								</li>
							{/each}
						</ul>
						{#if visibleStash.length === 0}
							<p class="mt-2 text-caption text-muted-foreground">
								No stash entries match the filter.
							</p>
						{/if}
					{/if}
				</Card>

				<div class="space-y-3">
					<Card
						title="Main view"
						description={selection
							? `Selected: ${selection}`
							: 'Selected diff and quick actions'}
					>
						{#if panel === 'commits' && commitFiles.length > 0}
							<ul class="mb-2 divide-y divide-border">
								{#each commitFiles as file (file.path)}
									<li>
										<button
											type="button"
											onclick={() =>
												selection &&
												void showCommitFileDiff(selection, file.path)}
											class="flex w-full items-center justify-between gap-2 px-1 py-1 text-left hover:bg-accent/50"
										>
											<span class="truncate font-mono text-caption"
												>{file.path}</span
											>
											<span class="font-mono text-micro text-muted-foreground"
												>{file.status}</span
											>
										</button>
									</li>
								{/each}
							</ul>
						{/if}
						{#if diff.length === 0}
							<p class="text-caption text-muted-foreground">
								{panel === 'commits'
									? 'Select a commit, then pick a file to see its range diff.'
									: 'Select a file to preview its hunk diff.'}
							</p>
						{:else}
							<p class="mb-1 text-micro text-muted-foreground">
								{diffSource === 'commit'
									? 'commit range diff'
									: 'unstaged diff'}
							</p>
							{#each diff as file (file.path)}
								<p class="mb-1 font-mono text-caption">
									{file.path} (+{file.additions}/-{file.deletions})
								</p>
								{#each file.hunks as hunk (hunk.header)}
									<p class="font-mono text-micro text-muted-foreground">
										{hunk.header}
									</p>
									<pre
										class="mb-2 max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap">{hunk.lines.join(
											'\n',
										)}</pre>
								{/each}
							{/each}
						{/if}
						{#snippet footer()}
							<div class="flex flex-wrap gap-2">
								{#each controls as control (control.id)}
									{@const spec = specFor(control.id)}
									<Button
										size="sm"
										variant="outline"
										disabled={gitStore.busy || (spec ? !spec.available : false)}
										title={spec?.reason ?? `${control.name} (${control.hint})`}
										onclick={() => void runActionId(control.id)}
									>
										{control.name}
									</Button>
								{/each}
							</div>
						{/snippet}
					</Card>

					<Card
						title="Key input"
						description="Action-first; single chars are aliases. : forwards to terminal, ? lists actions."
					>
						<div class="flex gap-2">
							<Input
								id="git-input"
								label="Git key"
								size="sm"
								bind:value={token}
								placeholder="c commit · space stage · ? all"
								onkeydown={(event) => {
									if (event.key === 'Enter') void submitToken();
								}}
							/>
							<Button
								size="sm"
								disabled={gitStore.busy}
								onclick={() => void submitToken()}>Run</Button
							>
						</div>
						{#if tokenResult.kind === 'action'}
							<p class="mt-1 text-caption">
								Will run <span class="font-mono">{tokenResult.action}</span>
								{#if tokenResult.danger !== 'readonly' && tokenResult.danger !== 'recoverable'}
									<Badge variant="danger">{tokenResult.danger}</Badge>
								{/if}
							</p>
						{:else if tokenResult.kind === 'prefix' && tokenResult.candidates?.length}
							<p class="mt-1 text-micro text-muted-foreground">
								{tokenResult.candidates.slice(0, 8).join(', ')}
							</p>
						{:else if tokenResult.kind === 'help' && tokenResult.candidates?.length}
							<p class="mt-1 text-micro text-muted-foreground">
								{tokenResult.candidates.join(', ')}
							</p>
						{/if}
						{#if gitStore.actionError}
							<p class="mt-1 text-caption text-destructive">
								{gitStore.actionError}
							</p>
						{/if}
					</Card>

					{#if terminalOpen}
						<Card
							title="Terminal"
							description="Repo-locked shell; output polls the session buffer."
						>
							<div class="mb-2 flex gap-2">
								<Button size="sm" variant="ghost" onclick={clearTerminal}
									>Clear</Button
								>
								<Button
									size="sm"
									variant="ghost"
									onclick={() => void restartTerminal()}>Restart</Button
								>
							</div>
							<pre
								class="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap">{terminalText ||
									'(empty)'}</pre>
							<div class="mt-2 flex gap-2">
								<Input
									label="Terminal input"
									size="sm"
									bind:value={terminalInput}
									placeholder="git status --short --branch"
								/>
								<Button size="sm" onclick={() => void sendTerminal()}
									>Send</Button
								>
							</div>
						</Card>
					{/if}
				</div>
			</div>
		</PageState>
	</div>
</div>

<Dialog
	bind:open={dialogOpen}
	title="Git action"
	description="Parameterised actions run through the unified entry."
>
	{#if dialogAction}
		<p class="mb-2 font-mono text-caption">{dialogAction}</p>
		{#each dialogParams as param (param)}
			<Input
				label={PARAM_LABELS[param] ?? param}
				size="sm"
				bind:value={dialogValues[param]}
				placeholder={PARAM_LABELS[param] ?? param}
			/>
		{/each}
		{#if dialogNeedsConfirm}
			<div class="mt-2">
				<Input
					label={`Confirm (type: ${dialogTarget || 'target'})`}
					size="sm"
					bind:value={dialogConfirm}
					placeholder={dialogTarget || 'target'}
				/>
				{#if dialogConfirm.length > 0 && !dialogConfirmMatches}
					<p class="mt-1 text-micro text-destructive">
						Confirmation does not match the target.
					</p>
				{/if}
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button
			variant="ghost"
			size="sm"
			onclick={() => {
				dialogOpen = false;
				dialogAction = null;
			}}>Cancel</Button
		>
		<Button
			size="sm"
			disabled={gitStore.busy || !dialogCanSubmit}
			onclick={() => void confirmDialog()}>Confirm</Button
		>
	{/snippet}
</Dialog>

<Dialog
	bind:open={helpOpen}
	title="Actions"
	description={`All actions available in the ${panel} panel.`}
>
	<div class="space-y-1">
		{#each bindingsFor(panel) as binding (binding.id)}
			<button
				type="button"
				onclick={() => {
					helpOpen = false;
					void runActionId(binding.id);
				}}
				class="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left text-caption hover:bg-accent/50"
			>
				<span
					>{binding.name}
					<span class="font-mono text-micro text-muted-foreground"
						>{binding.id}</span
					></span
				>
				<span class="flex items-center gap-2">
					{#if GUARDED_DANGERS.has(binding.danger)}
						<Badge variant="danger">{binding.danger}</Badge>
					{:else}
						<Badge variant="neutral">{binding.danger}</Badge>
					{/if}
					<span class="font-mono text-micro text-muted-foreground"
						>{binding.hint}</span
					>
				</span>
			</button>
		{/each}
	</div>
</Dialog>
