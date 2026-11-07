<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import ResizeHandle from '#lib/components/layout/ResizeHandle.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { fileRowClass, markerClass } from '#lib/git/tone';
	import {
		getCommitFiles,
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
	import { EMPTY_TREE_SHA, type DiffTarget } from '#lib/ide/diff-tabs';
	import { t } from '#lib/i18n';

	interface Props {
		onViewFileDiff: (target: DiffTarget) => void;
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
	let branchOpen = $state(false);
	let checkoutTarget = $state<CommitEntry | null>(null);
	let resetTarget = $state<CommitEntry | null>(null);
	let resetMode = $state<'reset.soft' | 'reset.mixed' | 'reset.hard'>('reset.soft');
	let resetConfirmInput = $state('');
	let discardTarget = $state<GitFile | null>(null);
	let busyAction = $state<string | null>(null);
	let actionError = $state<string | null>(null);
	let changesOpen = $state(true);
	let commitsOpen = $state(true);
	let conflictedOpen = $state(true);
	let stagedOpen = $state(true);
	let unstagedOpen = $state(true);
	let commitMessage = $state('');

	const COMMIT_PAGE = 30;
	const COMMIT_FILE_CAP = 50;
	const CHANGES_PAGE = 30;

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
			return;
		}
		expandedCommit = commit.id;
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

	function openChangeDiff(group: 'staged' | 'unstaged' | 'conflicted', file: GitFile): void {
		onViewFileDiff({
			path: file.path,
			base: group === 'staged' ? { kind: 'staged' } : { kind: 'unstaged' },
		});
	}

	function openCommitDiff(commit: CommitEntry, file: CommitFileEntry): void {
		onViewFileDiff({
			path: file.path,
			base: {
				kind: 'commit',
				commit: commit.id,
				parent: commit.parents[0] ?? EMPTY_TREE_SHA,
			},
		});
	}

	function hasStaged(file: GitFile): boolean {
		return file.staged !== '' && file.staged !== '.' && file.staged !== '?' && file.staged !== ' ';
	}

	function hasUnstaged(file: GitFile): boolean {
		return file.unstaged !== '' && file.unstaged !== '.' && file.unstaged !== ' ';
	}

	async function afterWrite(): Promise<void> {
		await ide.refreshGitStatus();
		ide.bumpGitRefresh();
		await reload();
	}

	async function runStaging(
		action: string,
		params: Record<string, string>,
		confirm?: string,
	): Promise<boolean> {
		if (busyAction !== null) return false;
		busyAction = action;
		actionError = null;
		try {
			await runAction(action, params, confirm);
			await afterWrite();
			return true;
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
			return false;
		} finally {
			busyAction = null;
		}
	}

	async function commitStaged(): Promise<void> {
		const message = commitMessage.trim();
		if (busyAction !== null) return;
		if (message === '') {
			actionError = t('page.ide.commitEmpty');
			return;
		}
		if (stagedChanges.length === 0) {
			actionError = t('page.ide.commitNeedStaged');
			return;
		}
		const ok = await runStaging('commit', { message });
		if (ok) commitMessage = '';
	}

	function stageFile(file: GitFile): void {
		void runStaging('stage.toggle', { path: file.path });
	}

	function stageAll(): void {
		void runStaging('stage.all', {});
	}

	function unstageAll(): void {
		void runStaging('unstage.all', {});
	}

	function openDiscard(file: GitFile): void {
		discardTarget = file;
	}

	function confirmDiscard(): void {
		const target = discardTarget;
		if (!target || busyAction !== null) return;
		discardTarget = null;
		void runStaging('discard.unstaged', { path: target.path }, target.path);
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

	const isDetached = $derived(meta ? !branches.some((b) => b.current) : false);
	const conflictedChanges = $derived(changes.filter((file) => file.kind === 'conflicted'));
	const stagedChanges = $derived(
		changes.filter((file) => file.kind !== 'conflicted' && hasStaged(file)),
	);
	const unstagedChanges = $derived(
		changes.filter((file) => file.kind !== 'conflicted' && hasUnstaged(file)),
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
		<span class="min-w-0 flex-1 truncate text-caption font-medium"
			>{t('page.ide.historyTitle')}</span
		>
		{#if meta}
			<span
				class="shrink-0 truncate text-micro text-muted-foreground"
				title={t('page.ide.branchCurrent', { branch: meta.branch || meta.head.slice(0, 7) })}
			>
				{meta.branch || meta.head.slice(0, 7)}
			</span>
		{/if}
		<Button
			size="sm"
			variant="ghost"
			onclick={() => (branchOpen = true)}
			disabled={busyAction !== null}
		>
			{t('page.ide.branchSwitch')}
		</Button>
	</div>

	{#if actionError}
		<p
			class="border-b border-destructive/30 bg-destructive/10 px-2 py-1 text-micro text-destructive"
		>
			{actionError}
		</p>
	{/if}

	{#if !loading && !error}
		<div class="shrink-0 border-b border-border px-2 py-1.5">
			<div class="flex items-center gap-1">
				<input
					bind:value={commitMessage}
					placeholder={t('page.ide.commitPlaceholder')}
					aria-label={t('page.ide.commit')}
					disabled={busyAction !== null}
					class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-caption"
					onkeydown={(e) => {
						if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') void commitStaged();
					}}
				/>
				<Button
					size="sm"
					variant="default"
					title={t('page.ide.commitStagedHint')}
					disabled={busyAction !== null ||
						commitMessage.trim() === '' ||
						stagedChanges.length === 0}
					onclick={() => void commitStaged()}
				>
					{t('page.ide.commit')}
				</Button>
			</div>
			{#if changes.length > 0 && stagedChanges.length === 0}
				<p class="px-1 pt-1 text-micro text-muted-foreground">
					{t('page.ide.commitNeedStaged')}
				</p>
			{/if}
		</div>
	{/if}

	<div class="flex min-h-0 flex-1 flex-col">
		{#if loading}
			<p class="px-3 py-4 text-caption text-muted-foreground">{t('common.loading')}</p>
		{:else if error}
			<div class="space-y-2 px-3 py-4">
				<p class="text-caption text-destructive">{error}</p>
				<Button size="sm" variant="outline" onclick={() => void reload()}
					>{t('common.retry')}</Button
				>
			</div>
		{:else}
			{#if meta && isDetached}
				<div
					class="m-2 shrink-0 rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-micro"
				>
					{t('page.ide.detachedHead', {
						sha: meta.head.slice(0, 7),
						branch: branches[0]?.name ?? 'main',
					})}
				</div>
			{/if}

			{#snippet changeRow(file: GitFile, group: 'staged' | 'unstaged' | 'conflicted')}
				{@const letter = group === 'staged' ? file.staged : file.unstaged}
				{@const busy = busyAction !== null}
				<div>
					<div class="group relative flex items-center gap-1 px-2 py-0.5">
						<button
							type="button"
							class="min-w-0 flex-1 cursor-pointer truncate rounded px-1 py-0.5 text-left text-caption hover:bg-accent/60 {fileRowClass(
								file,
							)}"
							title={file.path}
							onclick={() => openChangeDiff(group, file)}
						>
							{file.path}
						</button>
						<span
							class="w-4 shrink-0 text-center font-mono text-micro {markerClass(
								letter,
								group === 'staged' ? 'staged' : 'unstaged',
							)}"
							title={letter}
						>
							{letter}
						</span>
						<div
							class="pointer-events-none absolute inset-y-0 right-7 flex items-center bg-card/95 opacity-0 transition-opacity group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 max-md:pointer-events-auto max-md:opacity-100"
						>
							{#if group === 'unstaged'}
								<IconButton
									icon="plus"
									label={t('page.ide.stage')}
									compact
									iconSize={14}
									disabled={busy}
									onclick={() => stageFile(file)}
									class="h-6 w-6 max-md:h-7 max-md:w-7"
								/>
								{#if file.kind !== 'untracked'}
									<IconButton
										icon="undo"
										label={t('page.ide.discardChanges')}
										compact
										iconSize={14}
										disabled={busy}
										onclick={() => openDiscard(file)}
										class="h-6 w-6 max-md:h-7 max-md:w-7"
									/>
								{/if}
							{:else if group === 'staged'}
								<IconButton
									icon="minus"
									label={t('page.ide.unstage')}
									compact
									iconSize={14}
									disabled={busy}
									onclick={() => stageFile(file)}
									class="h-6 w-6 max-md:h-7 max-md:w-7"
								/>
							{/if}
							<IconButton
								icon="file"
								label={t('page.ide.menuOpen')}
								compact
								iconSize={14}
								onclick={() => void openFile(file.path)}
								class="h-6 w-6 max-md:h-7 max-md:w-7"
							/>
						</div>
					</div>
				</div>
			{/snippet}

			{#snippet groupHeader(
				section: string,
				count: number,
				open: boolean,
				onToggle: () => void,
				stageLabel?: string,
				onStage?: () => void,
			)}
				<div class="flex items-center gap-1 px-2 pt-1">
					<button
						type="button"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-1 rounded px-1 py-0.5 text-left text-micro font-medium tracking-wide text-muted-foreground uppercase hover:bg-accent/60"
						aria-expanded={open}
						aria-label={open
							? t('page.ide.collapseSection', { section })
							: t('page.ide.expandSection', { section })}
						onclick={onToggle}
					>
						<span class="shrink-0 font-mono">{open ? '▾' : '▸'}</span>
						<span class="min-w-0 flex-1 truncate">{section} · {count}</span>
					</button>
					{#if stageLabel && onStage && open && count > 0}
						<Button size="sm" variant="ghost" disabled={busyAction !== null} onclick={onStage}>
							{stageLabel}
						</Button>
					{/if}
				</div>
			{/snippet}

			{#snippet moreLink(count: number)}
				{#if count > CHANGES_PAGE}
					<button
						type="button"
						class="cursor-pointer px-3 py-0.5 text-left text-micro text-muted-foreground underline"
						onclick={onOpenInGit}
					>
						{t('page.ide.showRemainingChanges', { count: count - CHANGES_PAGE })}
					</button>
				{/if}
			{/snippet}

			<div
				class="flex min-h-0 shrink-0 flex-col border-b border-border {commitsOpen && changesOpen
					? ''
					: 'flex-1'}"
				style={commitsOpen && changesOpen ? `height: ${ide.historyChangesHeight}px` : undefined}
			>
				<section aria-label={t('page.ide.historyChanges')} class="flex min-h-0 flex-1 flex-col">
					<div class="flex items-center gap-1 px-2 pt-2 pb-1">
						<button
							type="button"
							class="flex min-w-0 flex-1 cursor-pointer items-center gap-1 rounded px-1 py-0.5 text-left text-micro font-medium tracking-wide text-muted-foreground uppercase hover:bg-accent/60"
							aria-expanded={changesOpen}
							aria-label={changesOpen
								? t('page.ide.collapseSection', { section: t('page.ide.historyChanges') })
								: t('page.ide.expandSection', { section: t('page.ide.historyChanges') })}
							onclick={() => (changesOpen = !changesOpen)}
						>
							<span class="shrink-0 font-mono">{changesOpen ? '▾' : '▸'}</span>
							<span class="min-w-0 flex-1 truncate">
								{t('page.ide.historyChanges')} · {changes.length}
							</span>
						</button>
						{#if changesOpen && unstagedChanges.length > 0}
							<Button size="sm" variant="ghost" disabled={busyAction !== null} onclick={stageAll}>
								{t('page.ide.stageAll')}
							</Button>
						{/if}
						{#if changesOpen && stagedChanges.length > 0}
							<Button size="sm" variant="ghost" disabled={busyAction !== null} onclick={unstageAll}>
								{t('page.ide.unstageAll')}
							</Button>
						{/if}
					</div>
					{#if changesOpen}
						<div class="min-h-0 flex-1 overflow-y-auto pb-1 [scrollbar-gutter:stable]">
							{#if changes.length === 0}
								<p class="px-3 py-1 text-micro text-muted-foreground">—</p>
							{:else}
								{#if conflictedChanges.length > 0}
									{@render groupHeader(
										t('domain.git.conflicted'),
										conflictedChanges.length,
										conflictedOpen,
										() => (conflictedOpen = !conflictedOpen),
									)}
									{#if conflictedOpen}
										{#each conflictedChanges.slice(0, CHANGES_PAGE) as file (file.path)}
											{@render changeRow(file, 'conflicted')}
										{/each}
										{@render moreLink(conflictedChanges.length)}
									{/if}
								{/if}
								{#if stagedChanges.length > 0}
									{@render groupHeader(
										t('domain.git.staged'),
										stagedChanges.length,
										stagedOpen,
										() => (stagedOpen = !stagedOpen),
										t('page.ide.unstageAll'),
										unstageAll,
									)}
									{#if stagedOpen}
										{#each stagedChanges.slice(0, CHANGES_PAGE) as file (file.path)}
											{@render changeRow(file, 'staged')}
										{/each}
										{@render moreLink(stagedChanges.length)}
									{/if}
								{/if}
								{#if unstagedChanges.length > 0}
									{@render groupHeader(
										t('domain.git.unstaged'),
										unstagedChanges.length,
										unstagedOpen,
										() => (unstagedOpen = !unstagedOpen),
										t('page.ide.stageAll'),
										stageAll,
									)}
									{#if unstagedOpen}
										{#each unstagedChanges.slice(0, CHANGES_PAGE) as file (file.path)}
											{@render changeRow(file, 'unstaged')}
										{/each}
										{@render moreLink(unstagedChanges.length)}
									{/if}
								{/if}
							{/if}
						</div>
					{/if}
				</section>
			</div>

			{#if changesOpen && commitsOpen}
				<ResizeHandle
					label={t('page.ide.resizeHistorySplit')}
					axis="y"
					invert
					class="h-2 shrink-0 text-muted-foreground hover:text-foreground"
					pillClass="h-1 w-16 bg-border"
					onResize={(delta) => ide.setHistoryChangesHeight(ide.historyChangesHeight + delta)}
					onReset={() => ide.resetHistoryChangesHeight()}
				/>
			{/if}

			<div class="flex min-h-0 flex-1 flex-col">
				<section
					aria-label={t('page.ide.historyCommits')}
					class="flex min-h-0 flex-1 flex-col pb-3"
				>
					<div class="flex items-center gap-1 px-2 pt-2 pb-1">
						<button
							type="button"
							class="flex min-w-0 flex-1 cursor-pointer items-center gap-1 rounded px-1 py-0.5 text-left text-micro font-medium tracking-wide text-muted-foreground uppercase hover:bg-accent/60"
							aria-expanded={commitsOpen}
							aria-label={commitsOpen
								? t('page.ide.collapseSection', { section: t('page.ide.historyCommits') })
								: t('page.ide.expandSection', { section: t('page.ide.historyCommits') })}
							onclick={() => (commitsOpen = !commitsOpen)}
						>
							<span class="shrink-0 font-mono">{commitsOpen ? '▾' : '▸'}</span>
							<span class="min-w-0 flex-1 truncate">{t('page.ide.historyCommits')}</span>
						</button>
					</div>
					{#if commitsOpen}
						<div class="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
							{#if commits.length === 0}
								<p class="px-3 py-1 text-micro text-muted-foreground">
									{t('page.ide.historyEmpty')}
								</p>
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
											<span class="shrink-0 font-mono text-micro text-muted-foreground"
												>{commit.shortId}</span
											>
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
														<div>
															<div class="flex items-center gap-1">
																<button
																	type="button"
																	class="min-w-0 flex-1 cursor-pointer truncate rounded px-1 py-0.5 text-left text-micro hover:bg-accent/60"
																	title={file.path}
																	onclick={() => openCommitDiff(commit, file)}
																>
																	{file.path}
																</button>
																<span
																	class="shrink-0 rounded bg-muted px-1 text-micro text-muted-foreground"
																>
																	{file.status}
																</span>
															</div>
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
													<Button
														size="sm"
														variant="ghost"
														onclick={() => (checkoutTarget = commit)}
													>
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
										<Button
											size="sm"
											variant="outline"
											onclick={() => void loadMore()}
											disabled={loadingMore}
										>
											{loadingMore ? t('common.loading') : t('page.ide.loadMore')}
										</Button>
									</div>
								{/if}
							{/if}
						</div>
					{/if}
				</section>
			</div>
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
			<p class="px-2 py-4 text-center text-caption text-muted-foreground">
				{t('page.ide.historyEmpty')}
			</p>
		{/if}
	</div>
</Dialog>

<Dialog
	open={discardTarget !== null}
	title={t('page.ide.discardUnstagedTitle')}
	description={discardTarget
		? t('page.ide.discardUnstagedDesc', { path: discardTarget.path })
		: undefined}
	onclose={() => {
		discardTarget = null;
	}}
>
	<div class="space-y-2 p-4">
		<div class="flex justify-end gap-2 pt-1">
			<Button
				size="sm"
				variant="ghost"
				onclick={() => {
					discardTarget = null;
				}}
			>
				{t('common.cancel')}
			</Button>
			<Button
				size="sm"
				variant="default"
				disabled={busyAction !== null || !discardTarget}
				onclick={confirmDiscard}
			>
				{t('page.ide.discardChanges')}
			</Button>
		</div>
	</div>
</Dialog>

<Dialog
	open={checkoutTarget !== null}
	title={t('page.ide.checkoutCommit')}
	description={checkoutTarget
		? t('page.ide.checkoutDetachedDesc', { sha: checkoutTarget.shortId })
		: undefined}
	onclose={() => (checkoutTarget = null)}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={() => (checkoutTarget = null)}
			>{t('common.cancel')}</Button
		>
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
				<input
					type="radio"
					name="ide-reset-mode"
					value="reset.soft"
					bind:group={resetMode}
					class="accent-primary"
				/>
				{t('page.ide.resetSoft')}
			</label>
			<label class="flex cursor-pointer items-center gap-2 text-body">
				<input
					type="radio"
					name="ide-reset-mode"
					value="reset.mixed"
					bind:group={resetMode}
					class="accent-primary"
				/>
				{t('page.ide.resetMixed')}
			</label>
			<label class="flex cursor-pointer items-center gap-2 text-body">
				<input
					type="radio"
					name="ide-reset-mode"
					value="reset.hard"
					bind:group={resetMode}
					class="accent-primary"
				/>
				{t('page.ide.resetHard')}
			</label>
		</div>
		{#if resetMode === 'reset.hard' && resetTarget}
			<p class="text-caption text-destructive">
				{t('page.ide.resetHardConfirmHint', { sha: resetTarget.shortId })}
			</p>
			<input
				bind:value={resetConfirmInput}
				placeholder={resetTarget.shortId}
				aria-label={t('page.ide.resetHardConfirmHint', { sha: resetTarget.shortId })}
				class="h-8 w-full rounded-md border border-input bg-card px-2 text-body"
			/>
		{/if}
		<div class="flex justify-end gap-2 pt-1">
			<Button size="sm" variant="ghost" onclick={() => (resetTarget = null)}
				>{t('common.cancel')}</Button
			>
			<Button size="sm" variant="default" onclick={() => void confirmReset()}>
				{t('page.ide.resetToHere')}
			</Button>
		</div>
	</div>
</Dialog>
