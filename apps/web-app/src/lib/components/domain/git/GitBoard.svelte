<script lang="ts">
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';
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
	let branches = $derived(
		workbench.branches.filter((branch) => branch.name.includes(workbench.filters.branches)),
	);
	let commits = $derived(
		workbench.commits.filter(
			(commit) =>
				commit.subject.includes(workbench.filters.commits) ||
				commit.shortId.includes(workbench.filters.commits),
		),
	);
	let stash = $derived(
		workbench.stash.filter(
			(entry) =>
				entry.message.includes(workbench.filters.stash) ||
				entry.name.includes(workbench.filters.stash),
		),
	);
</script>

<div id="git-board" class="grid content-start gap-2 md:grid-cols-2">
	<div onfocusin={() => (workbench.panel = 'files')}>
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

	<div onfocusin={() => (workbench.panel = 'branches')}>
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

	<div onfocusin={() => (workbench.panel = 'commits')}>
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
							<span class="shrink-0 text-info">{commit.shortId}</span>
							<span class="min-w-0 flex-1 truncate {commitRowClass(commit)}">{commit.subject}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#if commits.length === 0}
				<p class="px-1 py-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.noCommitsMatch')}
				</p>
			{/if}
		</TuiPanel>
	</div>

	<div onfocusin={() => (workbench.panel = 'stash')}>
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
