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
		getDiff,
		getMeta,
		getRegistry,
		listBranches,
		listCommits,
		listFiles,
		listStash,
		readTerminalOutput,
		runAction,
		sendTerminalInput,
		type BranchEntry,
		type CommitEntry,
		type DiffFile,
		type GitFile,
		type RepoMeta,
		type StashEntry,
	} from '$lib/services/git';
	import { checkDrift, visibleControls } from '$lib/git/bindings';
	import { parseToken } from '$lib/git/parse';

	const PANELS = [
		{ id: 'status', label: 'Status' },
		{ id: 'files', label: 'Files' },
		{ id: 'branches', label: 'Branches' },
		{ id: 'commits', label: 'Commits' },
		{ id: 'stash', label: 'Stash' },
	] as const;

	const requested = parseListParams(page.url).panel;
	let panel = $state(
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
	let files = $state<GitFile[]>([]);
	let branches = $state<BranchEntry[]>([]);
	let commits = $state<CommitEntry[]>([]);
	let commitsOffset = $state(0);
	let commitsHasMore = $state(false);
	let commitsLoading = $state(false);
	let stash = $state<StashEntry[]>([]);
	let diff = $state<DiffFile[]>([]);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let noRepo = $state(false);
	let liveDirty = $state(false);
	let stopLive: (() => void) | null = null;

	let token = $state('');
	let terminalOpen = $state(false);
	let terminalId = $state<string | null>(null);
	let terminalText = $state('');
	let terminalInput = $state('');
	let terminalOffset = $state(0);

	let dialogAction = $state<string | null>(null);
	let dialogOpen = $state(false);
	let dialogParam = $state('');
	let dialogConfirm = $state('');

	const tokenResult = $derived(parseToken(token, panel));
	const controls = $derived(visibleControls(panel));

	onMount(() => {
		stopLive = gitStore.subscribeLive(() => {
			if (loading) liveDirty = true;
			else void reload();
		});
		void reload();
		const el = document.getElementById('git-input');
		el?.focus();
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
			const [f, b, c, s] = await Promise.all([
				listFiles().catch(() => []),
				listBranches().catch(() => []),
				listCommits({ limit: 50 }).catch(() => []),
				listStash().catch(() => []),
			]);
			files = f;
			branches = b;
			commits = c;
			commitsOffset = c.length;
			commitsHasMore = c.length >= 50;
			stash = s;
			try {
				const registry = await getRegistry();
				const drift = checkDrift(registry.map((r) => r.id));
				if (drift.unknown.length > 0) {
					toasts.warning('Git bindings drifted', drift.unknown.slice(0, 3).join(', '));
				}
			} catch {
				// Registry drift is advisory only.
			}
			if (gitStore.selectedFile) {
				diff = await getDiff(gitStore.selectedFile).catch(() => []);
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
		document.getElementById('git-input')?.focus();
	}

	async function submitToken(): Promise<void> {
		const result = parseToken(token, panel);
		if (result.kind === 'command') {
			token = '';
			if (!terminalOpen) await toggleTerminal();
			if (result.preview) {
				terminalInput = result.preview;
				await sendTerminal();
			}
			return;
		}
		if (result.kind !== 'action' || !result.action) return;
		const needsParam =
			result.action === 'commit' ||
			result.action === 'branch.create' ||
			result.action === 'branch.checkout' ||
			result.action === 'branch.delete' ||
			result.action === 'discard';
		if (needsParam) {
			dialogAction = result.action;
			dialogOpen = true;
			dialogParam = '';
			dialogConfirm = '';
			return;
		}
		await dispatchAction(result.action, {}, undefined);
		token = '';
	}

	async function dispatchAction(
		action: string,
		params: Record<string, string>,
		confirm?: string,
	): Promise<void> {
		gitStore.beginAction(action);
		try {
			const outcome = await runAction(action, params, confirm);
			toasts.success(outcome.message || action, outcome.echo);
			await reload();
		} catch (e) {
			const message = e instanceof Error ? e.message : 'Git action failed';
			gitStore.endAction(message);
			toasts.error('Git action failed', message);
			return;
		}
		gitStore.endAction();
	}

	async function confirmDialog(): Promise<void> {
		const action = dialogAction;
		if (!action) return;
		const params: Record<string, string> = {};
		let confirm: string | undefined;
		if (action === 'commit') params.message = dialogParam;
		else if (action === 'branch.create' || action === 'branch.checkout') params.branch = dialogParam;
		else if (action === 'branch.delete') {
			params.branch = dialogParam;
			confirm = dialogConfirm;
		} else if (action === 'discard') {
			params.path = dialogParam || gitStore.selectedFile || '';
			confirm = dialogConfirm;
		}
		dialogAction = null;
		dialogOpen = false;
		await dispatchAction(action, params, confirm);
		token = '';
	}

	async function toggleTerminal(): Promise<void> {
		terminalOpen = !terminalOpen;
		if (terminalOpen && !terminalId) {
			try {
				const session = await createTerminal();
				terminalId = session.id;
				terminalText = '';
				terminalOffset = 0;
			} catch (e) {
				toasts.error('Terminal unavailable', e instanceof Error ? e.message : undefined);
				terminalOpen = false;
			}
		}
		if (!terminalOpen && terminalId) {
			const id = terminalId;
			terminalId = null;
			await closeTerminal(id).catch(() => {});
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
			toasts.error('More commits unavailable', e instanceof Error ? e.message : undefined);
		} finally {
			commitsLoading = false;
		}
	}

	async function sendTerminal(): Promise<void> {
		if (!terminalId || !terminalInput.trim()) return;
		const input = terminalInput;
		terminalInput = '';
		try {
			await sendTerminalInput(terminalId, input);
			const output = await readTerminalOutput(terminalId, terminalOffset);
			terminalText += output;
			terminalOffset += output.length;
			await reload();
		} catch (e) {
			toasts.error('Terminal input failed', e instanceof Error ? e.message : undefined);
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title="Git" description="User repository workbench: status, files, branches, commits and stash.">
		{#snippet actions()}
			{#if meta}
				<Badge variant="neutral">{meta.branch || '(no branch)'}</Badge>
				{#if meta.upstream}
					<Badge variant="neutral">{meta.upstream} +{meta.ahead}/-{meta.behind}</Badge>
				{/if}
				{#if meta.mergeHead || meta.rebaseHead || meta.cherryPickHead}
					<Badge variant="warning">mid-operation</Badge>
				{/if}
			{/if}
			<IconButton icon="refresh" label="Refresh" onclick={() => void reload()} />
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

	<div id="git-panel" role="tabpanel" aria-label="Git panels" class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		<PageState
			{loading}
			error={loadError}
			empty={noRepo}
			emptyTitle="No repository"
			emptyDescription="Configure user_git.repo_root with user_git.enabled=true to attach the user repository."
			onretry={() => void reload()}
		>
			<div class="grid gap-3 xl:grid-cols-[minmax(0,280px)_minmax(0,1fr)]">
				<Card title="Groups" description="Status, files, branches, commits, stash">
					<div class="flex flex-col gap-1">
						{#each PANELS as item}
							<button
								type="button"
								onclick={() => {
									panel = item.id;
									document.getElementById('git-input')?.focus();
								}}
								aria-pressed={panel === item.id}
								class="rounded-md px-2 py-1.5 text-left text-caption hover:bg-accent/50 {panel === item.id ? 'bg-accent/70' : ''}"
							>
								{item.label}
							</button>
						{/each}
					</div>
					{#if panel === 'files'}
						<ul class="mt-2 divide-y divide-border">
							{#each files as file (file.path)}
								<li>
									<button
										type="button"
										onclick={() => void selectFile(file.path)}
										aria-pressed={gitStore.selectedFile === file.path}
										class="flex w-full items-center justify-between gap-2 px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectedFile === file.path ? 'bg-accent/70' : ''}"
									>
										<span class="truncate font-mono text-caption">{file.path}</span>
										<span class="font-mono text-micro text-muted-foreground">{file.staged}{file.unstaged}</span>
									</button>
								</li>
							{/each}
						</ul>
					{:else if panel === 'branches'}
						<ul class="mt-2 space-y-1">
							{#each branches as branch (branch.name)}
								<li>
									<button
										type="button"
										onclick={() => gitStore.select('branches', branch.name)}
										class="flex w-full items-center justify-between gap-2 px-2 py-1 text-left text-caption hover:bg-accent/50"
									>
										<span class="truncate font-mono">{branch.current ? '*' : ' '} {branch.name}</span>
										<span class="text-micro text-muted-foreground">{branch.subject.slice(0, 40)}</span>
									</button>
								</li>
							{/each}
						</ul>
					{:else if panel === 'commits'}
						<ul class="mt-2 space-y-1">
							{#each commits as commit (commit.id)}
								<li>
									<button
										type="button"
										onclick={() => gitStore.select('commits', commit.id)}
										class="block w-full px-2 py-1 text-left hover:bg-accent/50"
									>
										<p class="truncate font-mono text-caption">{commit.shortId} {commit.subject}</p>
										<p class="text-micro text-muted-foreground">{commit.author}</p>
									</button>
								</li>
							{/each}
						</ul>
						{#if commitsHasMore}
							<Button
								variant="ghost"
								size="sm"
								disabled={commitsLoading}
								onclick={() => void loadMoreCommits()}
							>
								{commitsLoading ? 'Loading…' : `Show more (${commits.length} shown)`}
							</Button>
						{/if}
					{:else if panel === 'stash'}
						<ul class="mt-2 space-y-1">
							{#each stash as entry (entry.name)}
								<li class="px-2 py-1 text-caption">
									<span class="font-mono">{entry.name}</span>
									<span class="text-muted-foreground"> {entry.message}</span>
								</li>
							{/each}
						</ul>
					{:else}
						<p class="mt-2 text-caption text-muted-foreground">
							Select a group; files and commits render in the main view.
						</p>
					{/if}
				</Card>

				<div class="space-y-3">
					<Card title="Main view" description="Selected diff and quick actions">
						{#if diff.length === 0}
							<p class="text-caption text-muted-foreground">Select a file to preview its hunk diff.</p>
						{:else}
							{#each diff as file}
								<p class="mb-1 font-mono text-caption">{file.path} (+{file.additions}/-{file.deletions})</p>
								{#each file.hunks as hunk}
									<p class="font-mono text-micro text-muted-foreground">{hunk.header}</p>
									<pre class="mb-2 max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap">{hunk.lines.join('\n')}</pre>
								{/each}
							{/each}
						{/if}
						{#snippet footer()}
							<div class="flex flex-wrap gap-2">
								{#each controls as control}
									<Button size="sm" variant="outline" disabled={gitStore.busy} onclick={() => { token = control.aliases[0] ?? control.id; void submitToken(); }}>
										{control.name}
									</Button>
								{/each}
							</div>
						{/snippet}
					</Card>

					<Card title="Key input" description="Action-first; single chars are aliases. : forwards to terminal, ? lists actions.">
						<div class="flex gap-2">
							<Input id="git-input" label="Git key" size="sm" bind:value={token} placeholder="c commit · space stage · ? all" />
							<Button size="sm" disabled={gitStore.busy} onclick={() => void submitToken()}>Run</Button>
						</div>
						{#if tokenResult.kind === 'action'}
							<p class="mt-1 text-caption">
								Will run <span class="font-mono">{tokenResult.action}</span>
								{#if tokenResult.danger !== 'readonly' && tokenResult.danger !== 'recoverable'}
									<Badge variant="warning">{tokenResult.danger}</Badge>
								{/if}
							</p>
						{:else if tokenResult.kind === 'prefix' && tokenResult.candidates?.length}
							<p class="mt-1 text-micro text-muted-foreground">{tokenResult.candidates.slice(0, 8).join(', ')}</p>
						{:else if tokenResult.kind === 'help' && tokenResult.candidates?.length}
							<p class="mt-1 text-micro text-muted-foreground">{tokenResult.candidates.join(', ')}</p>
						{/if}
						{#if gitStore.actionError}
							<p class="mt-1 text-caption text-destructive">{gitStore.actionError}</p>
						{/if}
					</Card>

					{#if terminalOpen}
						<Card title="Terminal" description="Repo-locked shell; output polls the session buffer.">
							<pre class="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap">{terminalText || '(empty)'}</pre>
							<div class="mt-2 flex gap-2">
								<Input label="Terminal input" size="sm" bind:value={terminalInput} placeholder="git status --short --branch" />
								<Button size="sm" onclick={() => void sendTerminal()}>Send</Button>
							</div>
						</Card>
					{/if}
				</div>
			</div>
		</PageState>
	</div>
</div>

<Dialog bind:open={dialogOpen} title="Git action" description="Parameterised actions run through the unified entry.">
	{#if dialogAction}
		<p class="mb-2 font-mono text-caption">{dialogAction}</p>
		<Input label="Target / message" bind:value={dialogParam} placeholder="message, branch or path" />
		{#if dialogAction === 'branch.delete' || dialogAction === 'discard'}
			<div class="mt-2">
				<Input label="Confirm (type the target again)" bind:value={dialogConfirm} placeholder="manual target confirmation" />
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => { dialogOpen = false; dialogAction = null; }}>Cancel</Button>
		<Button size="sm" disabled={gitStore.busy} onclick={() => void confirmDialog()}>Confirm</Button>
	{/snippet}
</Dialog>
