<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import DiffView from '#lib/ui/components/DiffView.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import {
		getCommitFiles,
		getDiffRange,
		getMeta,
		listBranches,
		listCommits,
		listFiles,
		runAction,
		type BranchEntry,
		type CommitEntry,
		type CommitFileEntry,
		type GitFile,
		type RepoMeta,
	} from '#lib/services/git';
	import type { DiffFile } from '#lib/types/diff';
	import { t } from '#lib/i18n';

	interface Props {
		onViewFileDiff: (path: string) => void;
		onOpenInGit: () => void;
	}

	let { onViewFileDiff, onOpenInGit }: Props = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let meta = $state<RepoMeta | null>(null);
	let changes = $state<GitFile[]>([]);
	let branches = $state<BranchEntry[]>([]);
	let commits = $state<CommitEntry[]>([]);
	let hasMore = $state(false);
	let loadingMore = $state(false);
	let expandedCommit = $state<string | null>(null);
	let filesByCommit = $state<Record<string, CommitFileEntry[]>>({});
	let filesLoading = $state(false);
	let expandedFile = $state<string | null>(null);
	let diffsByKey = $state<Record<string, DiffFile[]>>({});
	let diffLoading = $state(false);
	let branchOpen = $state(false);
	let checkoutTarget = $state<CommitEntry | null>(null);
	let resetTarget = $state<CommitEntry | null>(null);
	let resetMode = $state<'reset.soft' | 'reset.mixed' | 'reset.hard'>('reset.soft');
	let resetConfirmInput = $state('');
	let busyAction = $state<string | null>(null);
	let actionError = $state<string | null>(null);

	const COMMIT_PAGE = 30;
	const COMMIT_FILE_CAP = 50;

	let lastSeq = -1;

	$effect(() => {
		const seq = ide.gitRefreshSeq;
		if (seq === lastSeq) return;
		lastSeq = seq;
		void reload();
	});

	async function reload(): Promise<void> {
		loading = true;
		error = null;
		try {
			const [m, files, branchList, commitList] = await Promise.all([
				getMeta().catch(() => null),
				listFiles().catch(() => [] as GitFile[]),
				listBranches().catch(() => [] as BranchEntry[]),
				listCommits({ limit: COMMIT_PAGE }).catch(() => [] as CommitEntry[]),
			]);
			meta = m;
			if (!m) {
				error = 'No repository';
				return;
			}
			changes = files;
			branches = branchList;
			commits = commitList;
			hasMore = commitList.length >= COMMIT_PAGE;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (loadingMore || !hasMore) return;
		loadingMore = true;
		try {
			const more = await listCommits({ limit: COMMIT_PAGE, offset: commits.length });
			commits = [...commits, ...more];
			hasMore = more.length >= COMMIT_PAGE;
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			loadingMore = false;
		}
	}

	async function toggleCommit(commit: CommitEntry): Promise<void> {
		if (expandedCommit === commit.id) {
			expandedCommit = null;
			expandedFile = null;
			return;
		}
		expandedCommit = commit.id;
		expandedFile = null;
		if (filesByCommit[commit.id]) return;
		filesLoading = true;
		try {
			filesByCommit[commit.id] = await getCommitFiles(commit.id);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			filesLoading = false;
		}
	}

	async function toggleCommitFile(commit: CommitEntry, file: CommitFileEntry): Promise<void> {
		const key = `${commit.id}:${file.path}`;
		if (expandedFile === key) {
			expandedFile = null;
			return;
		}
		expandedFile = key;
		if (diffsByKey[key]) return;
		diffLoading = true;
		try {
			diffsByKey[key] = await getDiffRange(`${commit.id}^`, commit.id, file.path);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			diffLoading = false;
		}
	}

	function fileStatusLabel(file: GitFile): string {
		if (file.kind) return file.kind;
		const bits = [file.staged, file.unstaged].filter(Boolean).join('/');
		return bits || 'changed';
	}

	async function openFile(path: string): Promise<void> {
		await ide.openFile(path, true);
	}

	async function switchBranch(name: string): Promise<void> {
		branchOpen = false;
		busyAction = 'branch.checkout';
		actionError = null;
		try {
			await runAction('branch.checkout', { branch: name });
			await ide.refreshGitStatus();
			ide.bumpGitRefresh();
			await reload();
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			busyAction = null;
		}
	}

	async function confirmCheckout(): Promise<void> {
		const target = checkoutTarget;
		checkoutTarget = null;
		if (!target) return;
		busyAction = 'checkout';
		actionError = null;
		try {
			await runAction('branch.checkout', { branch: target.id });
			await ide.refreshGitStatus();
			ide.bumpGitRefresh();
			await reload();
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			busyAction = null;
		}
	}

	async function confirmReset(): Promise<void> {
		const target = resetTarget;
		if (!target) return;
		if (resetMode === 'reset.hard' && resetConfirmInput.trim() !== target.shortId) {
			actionError = t('page.ide.resetHardConfirmHint', { sha: target.shortId });
			return;
		}
		resetTarget = null;
		resetConfirmInput = '';
		busyAction = resetMode;
		actionError = null;
		try {
			const confirm = resetMode === 'reset.hard' ? target.shortId : undefined;
			await runAction(resetMode, { commit: target.id }, confirm);
			await ide.refreshGitStatus();
			ide.bumpGitRefresh();
			await reload();
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			busyAction = null;
		}
	}

	const isDetached = $derived(
		meta ? !branches.some((b) => b.current) : false,
	);

	function formatTime(ts: number): string {
		if (!ts) return '';
		try {
			return new Date(ts * 1000).toLocaleString();
		} catch {
			return '';
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex items-center gap-1 border-b border-border px-2 py-1.5">
		<span class="min-w-0 flex-1 truncate text-caption font-medium">{t('page.ide.historyTitle')}</span>
		{#if meta}
			<span class="shrink-0 truncate text-micro text-muted-foreground" title={t('page.ide.branchCurrent', { branch: meta.branch || meta.head.slice(0, 7) })}>
				{meta.branch || meta.head.slice(0, 7)}
			</span>
		{/if}
		<Button size="sm" variant="ghost" onclick={() => (branchOpen = true)} disabled={busyAction !== null}>
			{t('page.ide.branchSwitch')}
		</Button>
	</div>

	{#if actionError}
		<p class="border-b border-destructive/30 bg-destructive/10 px-2 py-1 text-micro text-destructive">
			{actionError}
		</p>
	{/if}

	<div class="min-h-0 flex-1 overflow-y-auto">
		{#if loading}
			<p class="px-3 py-4 text-caption text-muted-foreground">{t('common.loading')}</p>
		{:else if error}
			<div class="space-y-2 px-3 py-4">
				<p class="text-caption text-destructive">{error}</p>
				<Button size="sm" variant="outline" onclick={() => void reload()}>{t('common.retry')}</Button>
			</div>
		{:else}
			{#if meta && isDetached}
				<div class="m-2 rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-micro">
					{t('page.ide.detachedHead', {
						sha: meta.head.slice(0, 7),
						branch: branches[0]?.name ?? 'main',
					})}
				</div>
			{/if}

			<section aria-label={t('page.ide.historyChanges')}>
				<p class="px-3 pt-2 pb-1 text-micro font-medium tracking-wide text-muted-foreground uppercase">
					{t('page.ide.historyChanges')} · {changes.length}
				</p>
				{#if changes.length === 0}
					<p class="px-3 py-1 text-micro text-muted-foreground">—</p>
				{:else}
					{#each changes.slice(0, 30) as file (file.path)}
						<div class="group flex items-center gap-1 px-2 py-0.5">
							<button
								type="button"
								class="min-w-0 flex-1 cursor-pointer truncate rounded px-1 py-0.5 text-left text-caption hover:bg-accent/60"
								title={file.path}
								onclick={() => void openFile(file.path)}
							>
								{file.path}
							</button>
							<span class="shrink-0 rounded bg-muted px-1 text-micro text-muted-foreground">
								{fileStatusLabel(file)}
							</span>
							<button
								type="button"
								class="shrink-0 cursor-pointer rounded px-1 text-micro text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-accent"
								title={t('page.ide.viewDiff')}
								onclick={() => onViewFileDiff(file.path)}
							>
								≠
							</button>
						</div>
					{/each}
				{/if}
			</section>

			<section aria-label={t('page.ide.historyCommits')} class="pb-3">
				<p class="px-3 pt-3 pb-1 text-micro font-medium tracking-wide text-muted-foreground uppercase">
					{t('page.ide.historyCommits')}
				</p>
				{#if commits.length === 0}
					<p class="px-3 py-1 text-micro text-muted-foreground">{t('page.ide.historyEmpty')}</p>
				{:else}
					{#each commits as commit (commit.id)}
						{@const isHead = meta?.head === commit.id}
						{@const isOpen = expandedCommit === commit.id}
						<div class="mx-1 rounded-md {isHead ? 'bg-primary/10' : ''}">
							<button
								type="button"
								class="flex w-full items-baseline gap-1.5 rounded-md px-2 py-1 text-left hover:bg-accent/60"
								aria-expanded={isOpen}
								onclick={() => void toggleCommit(commit)}
							>
								<span class="shrink-0 font-mono text-micro text-muted-foreground">{commit.shortId}</span>
								<span class="min-w-0 flex-1 truncate text-caption">{commit.subject}</span>
							</button>
							<p class="truncate px-2 pb-1 text-micro text-muted-foreground">
								{commit.author} · {formatTime(commit.timestamp)}
							</p>
							{#if isOpen}
								<div class="px-2 pb-1">
									{#if filesLoading && !filesByCommit[commit.id]}
										<p class="py-1 text-micro text-muted-foreground">{t('common.loading')}</p>
									{:else}
										{@const files = filesByCommit[commit.id] ?? []}
										{#each files.slice(0, COMMIT_FILE_CAP) as file (file.path)}
											{@const key = `${commit.id}:${file.path}`}
											<div>
												<div class="flex items-center gap-1">
													<button
														type="button"
														class="min-w-0 flex-1 cursor-pointer truncate rounded px-1 py-0.5 text-left text-micro hover:bg-accent/60"
														title={file.path}
														onclick={() => void toggleCommitFile(commit, file)}
														aria-expanded={expandedFile === key}
													>
														{file.path}
													</button>
													<span class="shrink-0 rounded bg-muted px-1 text-micro text-muted-foreground">
														{file.status}
													</span>
												</div>
												{#if expandedFile === key}
													{#if diffLoading && !diffsByKey[key]}
														<p class="py-1 pl-3 text-micro text-muted-foreground">{t('common.loading')}</p>
													{:else}
														{#each diffsByKey[key] ?? [] as diff (diff.path)}
															<DiffView lines={diff.lines} title={diff.path} class="my-1" />
														{/each}
														{#if (diffsByKey[key] ?? []).length === 0}
															<p class="py-1 pl-3 text-micro text-muted-foreground">{t('page.ide.diffEmpty')}</p>
														{/if}
													{/if}
												{/if}
											</div>
										{/each}
										{#if files.length > COMMIT_FILE_CAP}
											<button
												type="button"
												class="cursor-pointer px-1 py-0.5 text-micro text-muted-foreground underline"
												onclick={onOpenInGit}
											>
												{t('page.ide.historyViewAll')}
											</button>
										{/if}
									{/if}
									<div class="flex flex-wrap gap-1 pt-1">
										<Button size="sm" variant="ghost" onclick={() => (checkoutTarget = commit)}>
											{t('page.ide.checkoutCommit')}
										</Button>
										<Button
											size="sm"
											variant="ghost"
											onclick={() => {
												resetTarget = commit;
												resetMode = 'reset.soft';
												resetConfirmInput = '';
											}}
										>
											{t('page.ide.resetToHere')}
										</Button>
									</div>
								</div>
							{/if}
						</div>
					{/each}
					{#if hasMore}
						<div class="px-3 pt-1">
							<Button size="sm" variant="outline" onclick={() => void loadMore()} disabled={loadingMore}>
								{loadingMore ? t('common.loading') : t('page.ide.loadMore')}
							</Button>
						</div>
					{/if}
				{/if}
			</section>
		{/if}
	</div>
</div>

<Dialog open={branchOpen} title={t('page.ide.branchSwitch')} onclose={() => (branchOpen = false)}>
	<div class="max-h-72 overflow-y-auto p-1">
		{#each branches as branch (branch.name)}
			<button
				type="button"
				class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body hover:bg-accent/60"
				onclick={() => void switchBranch(branch.name)}
			>
				<span class="min-w-0 flex-1 truncate">{branch.name}</span>
				{#if branch.current}
					<span class="shrink-0 text-micro text-muted-foreground">●</span>
				{/if}
			</button>
		{/each}
		{#if branches.length === 0}
			<p class="px-2 py-4 text-center text-caption text-muted-foreground">{t('page.ide.historyEmpty')}</p>
		{/if}
	</div>
</Dialog>

<Dialog
	open={checkoutTarget !== null}
	title={t('page.ide.checkoutCommit')}
	description={checkoutTarget ? t('page.ide.checkoutDetachedDesc', { sha: checkoutTarget.shortId }) : undefined}
	onclose={() => (checkoutTarget = null)}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={() => (checkoutTarget = null)}>{t('common.cancel')}</Button>
		<Button size="sm" variant="default" onclick={() => void confirmCheckout()}>
			{t('page.ide.checkoutCommit')}
		</Button>
	</div>
</Dialog>

<Dialog
	open={resetTarget !== null}
	title={t('page.ide.resetToHere')}
	description={resetTarget ? `${resetTarget.shortId} · ${resetTarget.subject}` : undefined}
	onclose={() => (resetTarget = null)}
>
	<div class="space-y-2 p-4">
		<p class="text-caption text-muted-foreground">{t('page.ide.resetMode')}</p>
		<div class="flex flex-col gap-1">
			<label class="flex cursor-pointer items-center gap-2 text-body">
				<input type="radio" name="ide-reset-mode" value="reset.soft" bind:group={resetMode} class="accent-primary" />
				{t('page.ide.resetSoft')}
			</label>
			<label class="flex cursor-pointer items-center gap-2 text-body">
				<input type="radio" name="ide-reset-mode" value="reset.mixed" bind:group={resetMode} class="accent-primary" />
				{t('page.ide.resetMixed')}
			</label>
			<label class="flex cursor-pointer items-center gap-2 text-body">
				<input type="radio" name="ide-reset-mode" value="reset.hard" bind:group={resetMode} class="accent-primary" />
				{t('page.ide.resetHard')}
			</label>
		</div>
		{#if resetMode === 'reset.hard' && resetTarget}
			<p class="text-caption text-destructive">{t('page.ide.resetHardConfirmHint', { sha: resetTarget.shortId })}</p>
			<input
				bind:value={resetConfirmInput}
				placeholder={resetTarget.shortId}
				aria-label={t('page.ide.resetHardConfirmHint', { sha: resetTarget.shortId })}
				class="h-8 w-full rounded-md border border-input bg-card px-2 text-body"
			/>
		{/if}
		<div class="flex justify-end gap-2 pt-1">
			<Button size="sm" variant="ghost" onclick={() => (resetTarget = null)}>{t('common.cancel')}</Button>
			<Button size="sm" variant="default" onclick={() => void confirmReset()}>
				{t('page.ide.resetToHere')}
			</Button>
		</div>
	</div>
</Dialog>
