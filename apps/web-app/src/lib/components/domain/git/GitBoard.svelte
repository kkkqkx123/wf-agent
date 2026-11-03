<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
	import { assignGraphLanes } from '#lib/git/graph';
	import {
		branchRowClass,
		commitRowClass,
		fileRowClass,
		markerClass,
		stashRowClass,
	} from '#lib/git/tone';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let files = $derived(
		workbench.files.filter((file) => file.path.includes(workbench.filters.files)),
	);
	let conflicts = $derived(
		workbench.files.filter(
			(file) => file.kind === 'conflicted' && file.path.includes(workbench.filters.conflicts),
		),
	);
	let worktrees = $derived(
		workbench.worktrees.filter(
			(entry) =>
				entry.path.includes(workbench.filters.worktrees) ||
				entry.branch.includes(workbench.filters.worktrees),
		),
	);
	let submodules = $derived(
		workbench.submodules.filter(
			(entry) =>
				entry.path.includes(workbench.filters.submodules) ||
				entry.commit.includes(workbench.filters.submodules),
		),
	);
	let branches = $derived(
		workbench.branches.filter((branch) => branch.name.includes(workbench.filters.branches)),
	);
	let remotes = $derived(
		workbench.remotes.filter(
			(remote) =>
				remote.name.includes(workbench.filters.remotes) ||
				remote.url.includes(workbench.filters.remotes),
		),
	);
	let tags = $derived(
		workbench.tags.filter(
			(tag) =>
				tag.name.includes(workbench.filters.tags) || tag.commit.includes(workbench.filters.tags),
		),
	);
	let commits = $derived(
		workbench.commits.filter(
			(commit) =>
				commit.subject.includes(workbench.filters.commits) ||
				commit.shortId.includes(workbench.filters.commits),
		),
	);
	let reflogs = $derived(
		workbench.reflogs.filter(
			(entry) =>
				entry.message.includes(workbench.filters.reflog) ||
				entry.shortId.includes(workbench.filters.reflog) ||
				entry.selector.includes(workbench.filters.reflog),
		),
	);
	let stash = $derived(
		workbench.stash.filter(
			(entry) =>
				entry.message.includes(workbench.filters.stash) ||
				entry.name.includes(workbench.filters.stash),
		),
	);

	let lanes = $derived(
		assignGraphLanes(commits.map((commit) => ({ id: commit.id, parents: commit.parents }))),
	);
	let laneById = $derived(new Map(lanes.map((row) => [row.id, row] as const)));

	function visibility(panel: string): string {
		return workbench.panel === panel ? '' : 'hidden xl:block';
	}
</script>

<div id="git-board" class="grid content-start gap-2 md:grid-cols-2">
	<div class={visibility('status')} onfocusin={() => (workbench.panel = 'status')}>
		<TuiPanel title="Status" focused={workbench.panel === 'status'}>
			{#if workbench.status}
				<ul class="mt-1 font-mono text-micro">
					<li class="flex justify-between px-1 py-px">
						<span class="text-muted-foreground">{t('domain.git.staged')}</span><span
							class="text-success">{workbench.status.staged}</span
						>
					</li>
					<li class="flex justify-between px-1 py-px">
						<span class="text-muted-foreground">{t('domain.git.unstaged')}</span><span
							class="text-warning">{workbench.status.unstaged}</span
						>
					</li>
					<li class="flex justify-between px-1 py-px">
						<span class="text-muted-foreground">{t('domain.git.untracked')}</span><span
							>{workbench.status.untracked}</span
						>
					</li>
					<li class="flex justify-between px-1 py-px">
						<span class="text-muted-foreground">{t('domain.git.conflicted')}</span>
						<span class={workbench.status.conflicted > 0 ? 'font-semibold text-destructive' : ''}
							>{workbench.status.conflicted}</span
						>
					</li>
				</ul>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('files')} onfocusin={() => (workbench.panel = 'files')}>
		<TuiPanel title="Files" count={files.length} focused={workbench.panel === 'files'}>
			<input
				id="git-filter-files"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.files}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each files as file (file.path)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="files"
							data-board-id={file.path}
							onclick={() => void workbench.selectFile(file.path, false)}
							aria-pressed={gitStore.selectedFile === file.path}
							class="flex w-full items-center gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectedFile ===
							file.path
								? 'bg-accent/70'
								: ''}"
						>
							<span class="shrink-0">
								<span class={markerClass(file.staged, 'staged')}>{file.staged}</span><span
									class={markerClass(file.unstaged, 'unstaged')}>{file.unstaged}</span
								>
							</span>
							<span class="min-w-0 flex-1 truncate {fileRowClass(file)}"
								>{file.path}{file.renamedFrom ? ` → ${file.renamedFrom}` : ''}</span
							>
						</button>
					</li>
				{/each}
			</ul>
			{#if files.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noFilesMatch')}
				</p>
			{/if}
			{#snippet footer()}
				{#if workbench.status}
					<span>
						{t('domain.git.staged')}: {workbench.status.staged} · {t('domain.git.unstaged')}:
						{workbench.status.unstaged} · {t('domain.git.untracked')}: {workbench.status.untracked} ·
						{t('domain.git.conflicted')}: {workbench.status.conflicted}
					</span>
				{/if}
			{/snippet}
		</TuiPanel>
	</div>

	<div class={visibility('conflicts')} onfocusin={() => (workbench.panel = 'conflicts')}>
		<TuiPanel title="Conflicts" count={conflicts.length} focused={workbench.panel === 'conflicts'}>
			<input
				id="git-filter-conflicts"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.conflicts}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each conflicts as file (file.path)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="conflicts"
							data-board-id={file.path}
							onclick={() => void workbench.selectFile(file.path, false)}
							aria-pressed={gitStore.selectedFile === file.path}
							class="flex w-full items-center gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectedFile ===
							file.path
								? 'bg-accent/70'
								: ''}"
						>
							<span class="shrink-0 text-destructive">!!</span>
							<span class="min-w-0 flex-1 truncate {fileRowClass(file)}">{file.path}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if conflicts.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noConflicted')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('worktrees')} onfocusin={() => (workbench.panel = 'worktrees')}>
		<TuiPanel title="Worktrees" count={worktrees.length} focused={workbench.panel === 'worktrees'}>
			<input
				id="git-filter-worktrees"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.worktrees}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each worktrees as entry (entry.path)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="worktrees"
							data-board-id={entry.path}
							onclick={() => gitStore.select('worktrees', entry.path)}
							aria-pressed={gitStore.selectionFor('worktrees') === entry.path}
							class="block w-full truncate px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectionFor(
								'worktrees',
							) === entry.path
								? 'bg-accent/70'
								: ''}"
						>
							{entry.path}
							<span class="text-muted-foreground"
								>{entry.branch || '(detached)'}{entry.bare ? ' · bare' : ''}</span
							>
						</button>
					</li>
				{/each}
			</ul>
			{#if worktrees.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noWorktreesMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('submodules')} onfocusin={() => (workbench.panel = 'submodules')}>
		<TuiPanel
			title="Submodules"
			count={submodules.length}
			focused={workbench.panel === 'submodules'}
		>
			<input
				id="git-filter-submodules"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.submodules}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each submodules as entry (entry.path)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="submodules"
							data-board-id={entry.path}
							onclick={() => gitStore.select('submodules', entry.path)}
							aria-pressed={gitStore.selectionFor('submodules') === entry.path}
							class="block w-full truncate px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectionFor(
								'submodules',
							) === entry.path
								? 'bg-accent/70'
								: ''}"
						>
							{entry.path} <span class="text-muted-foreground">{entry.commit}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if submodules.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noSubmodulesMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('branches')} onfocusin={() => (workbench.panel = 'branches')}>
		<TuiPanel title="Branches" count={branches.length} focused={workbench.panel === 'branches'}>
			<input
				id="git-filter-branches"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.branches}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each branches as branch (branch.name)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="branches"
							data-board-id={branch.name}
							onclick={() => {
								workbench.panel = 'branches';
								gitStore.select('branches', branch.name);
							}}
							aria-pressed={gitStore.selectedBranch === branch.name}
							class="flex w-full items-center gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectedBranch ===
							branch.name
								? 'bg-accent/70'
								: ''}"
						>
							<span class="shrink-0 text-muted-foreground">{branch.current ? '*' : ' '}</span>
							<span class="min-w-0 flex-1 truncate {branchRowClass(branch)}">{branch.name}</span>
							{#if branch.upstream}
								<span class="shrink-0 text-muted-foreground">+{branch.ahead}/-{branch.behind}</span>
							{/if}
						</button>
					</li>
				{/each}
			</ul>
			{#if branches.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noBranchesMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('remotes')} onfocusin={() => (workbench.panel = 'remotes')}>
		<TuiPanel title="Remotes" count={remotes.length} focused={workbench.panel === 'remotes'}>
			<input
				id="git-filter-remotes"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.remotes}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each remotes as remote (remote.name)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="remotes"
							data-board-id={remote.name}
							onclick={() => {
								workbench.selectedRemote = remote.name;
							}}
							aria-pressed={workbench.selectedRemote === remote.name}
							class="block w-full truncate px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {workbench.selectedRemote ===
							remote.name
								? 'bg-accent/70'
								: ''}"
						>
							{remote.name} <span class="text-muted-foreground">{remote.url}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if remotes.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noRemotesMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('tags')} onfocusin={() => (workbench.panel = 'tags')}>
		<TuiPanel title="Tags" count={tags.length} focused={workbench.panel === 'tags'}>
			<input
				id="git-filter-tags"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.tags}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each tags as tag (tag.name)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="tags"
							data-board-id={tag.name}
							onclick={() => {
								workbench.selectedTag = tag.name;
								void workbench.selectCommit(tag.commit, false);
							}}
							aria-pressed={workbench.selectedTag === tag.name}
							class="block w-full truncate px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {workbench.selectedTag ===
							tag.name
								? 'bg-accent/70'
								: ''}"
						>
							{tag.name} <span class="text-muted-foreground">{tag.commit}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if tags.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noTagsMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('commits')} onfocusin={() => (workbench.panel = 'commits')}>
		<TuiPanel title="Commits" count={commits.length} focused={workbench.panel === 'commits'}>
			<input
				id="git-filter-commits"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.commits}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each commits as commit (commit.id)}
					{@const lane = laneById.get(commit.id)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="commits"
							data-board-id={commit.id}
							onclick={() => void workbench.selectCommit(commit.id, false)}
							aria-pressed={gitStore.selectedCommit === commit.id}
							class="flex w-full items-baseline gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectedCommit ===
							commit.id
								? 'bg-accent/70'
								: ''}"
						>
							{#if lane}
								<span class="flex shrink-0 items-stretch self-stretch" aria-hidden="true">
									{#each lane.active as active, laneIndex (laneIndex)}
										<span class="relative w-2.5">
											{#if active}
												<span class="absolute top-0 bottom-0 left-1/2 w-px bg-muted-foreground/60"
												></span>
											{/if}
											{#if laneIndex === lane.lane}
												<span
													class="absolute top-1/2 left-1/2 h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-info"
												></span>
											{/if}
										</span>
									{/each}
								</span>
							{/if}
							<span class="shrink-0 text-info">{commit.shortId}</span>
							<span class="min-w-0 flex-1 truncate {commitRowClass(commit)}">{commit.subject}</span>
							{#if commit.refs.length > 0}
								<span class="max-w-28 shrink-0 truncate text-muted-foreground"
									>{commit.refs.join(' ')}</span
								>
							{/if}
						</button>
					</li>
				{/each}
			</ul>
			{#if commits.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noCommitsMatch')}
				</p>
			{/if}
			{#if workbench.commitsHasMore}
				<div class="mt-1 px-1">
					<Button
						variant="ghost"
						size="sm"
						disabled={workbench.commitsLoading}
						onclick={() => void workbench.loadMoreCommits()}
					>
						{workbench.commitsLoading
							? t('common.loading')
							: t('domain.git.showMoreCommits', {
									shown: workbench.commits.length,
								})}
					</Button>
				</div>
			{/if}
			<p class="mt-1 px-1 font-mono text-micro text-muted-foreground/70">
				{t('domain.git.graphLocal')}
			</p>
			<p class="px-1 font-mono text-micro text-muted-foreground/70">
				{t('domain.git.graphApprox')}
			</p>
		</TuiPanel>
	</div>

	<div class={visibility('reflog')} onfocusin={() => (workbench.panel = 'reflog')}>
		<TuiPanel title="Reflog" count={reflogs.length} focused={workbench.panel === 'reflog'}>
			<input
				id="git-filter-reflog"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.reflog}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each reflogs as entry (entry.selector)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="reflog"
							data-board-id={entry.selector}
							onclick={() => void workbench.selectCommit(entry.id, false)}
							aria-pressed={gitStore.selectionFor('commits') === entry.id}
							class="flex w-full items-baseline gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectionFor(
								'commits',
							) === entry.id
								? 'bg-accent/70'
								: ''}"
						>
							<span class="shrink-0 text-info">{entry.shortId}</span>
							<span class="min-w-0 flex-1 truncate">{entry.message}</span>
							<span class="shrink-0 text-muted-foreground">{entry.selector}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if reflogs.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noReflogsMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div class={visibility('stash')} onfocusin={() => (workbench.panel = 'stash')}>
		<TuiPanel title="Stash" count={stash.length} focused={workbench.panel === 'stash'}>
			<input
				id="git-filter-stash"
				type="text"
				aria-label={t('domain.git.filter')}
				placeholder={t('domain.git.filterPlaceholder')}
				bind:value={workbench.filters.stash}
				class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<ul class="max-h-72 overflow-y-auto">
				{#each stash as entry (entry.name)}
					<li>
						<button
							type="button"
							data-board-row
							data-board-panel="stash"
							data-board-id={entry.name}
							onclick={() => {
								workbench.panel = 'stash';
								gitStore.select('stash', entry.name);
							}}
							aria-pressed={gitStore.selectedStash === entry.name}
							class="flex w-full items-baseline gap-1.5 px-1 py-px text-left font-mono text-micro hover:bg-accent/50 {gitStore.selectedStash ===
							entry.name
								? 'bg-accent/70'
								: ''}"
						>
							<span class="shrink-0 text-muted-foreground">{entry.name}</span>
							<span class="min-w-0 flex-1 truncate {stashRowClass()}">{entry.message}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if stash.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noStashMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>
</div>
