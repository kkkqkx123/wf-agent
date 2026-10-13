<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { git as gitStore } from '$lib/stores/git.svelte';
	import {
		PANELS,
		GitWorkbench,
		type PanelId,
	} from '$lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
		panel: PanelId;
	}

	let { workbench, panel = $bindable('status') }: Props = $props();
</script>

<Card title="Groups" description="Status, files, branches, commits, stash">
	<div class="flex flex-col gap-1">
		{#each PANELS as item (item.id)}
			<button
				type="button"
				onclick={() => {
					panel = item.id;
					workbench.focusKeyInput();
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
			bind:value={workbench.filters[panel]}
			placeholder="Filter this panel"
		/>
	{/if}
	{#if panel === 'status' && workbench.status}
		<ul class="mt-2 space-y-1 text-caption">
			<li class="flex justify-between">
				<span>Staged</span><span class="font-mono"
					>{workbench.status.staged}</span
				>
			</li>
			<li class="flex justify-between">
				<span>Unstaged</span><span class="font-mono"
					>{workbench.status.unstaged}</span
				>
			</li>
			<li class="flex justify-between">
				<span>Untracked</span><span class="font-mono"
					>{workbench.status.untracked}</span
				>
			</li>
			<li class="flex justify-between">
				<span>Conflicted</span>
				<span
					class="font-mono {workbench.status.conflicted > 0
						? 'text-destructive'
						: ''}">{workbench.status.conflicted}</span
				>
			</li>
		</ul>
	{:else if panel === 'files'}
		<ul class="mt-2 divide-y divide-border">
			{#each workbench.visibleFiles as file (file.path)}
				<li>
					<button
						type="button"
						onclick={() => void workbench.selectFile(file.path)}
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
		{#if workbench.visibleFiles.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				No files match the filter.
			</p>
		{/if}
		{#if workbench.worktrees.length > 0 || workbench.submodules.length > 0}
			<div class="mt-3 space-y-2 border-t border-border pt-2">
				{#if workbench.worktrees.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						Worktrees (read-only)
					</p>
					<ul class="space-y-0.5">
						{#each workbench.worktrees as worktree (worktree.path)}
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
				{#if workbench.submodules.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						Submodules (read-only)
					</p>
					<ul class="space-y-0.5">
						{#each workbench.submodules as submodule (submodule.path)}
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
			{#each workbench.visibleBranches as branch (branch.name)}
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
		{#if workbench.visibleBranches.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				No branches match the filter.
			</p>
		{/if}
		{#if workbench.remotes.length > 0 || workbench.tags.length > 0}
			<div class="mt-3 space-y-2 border-t border-border pt-2">
				{#if workbench.remotes.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						Remotes
					</p>
					<ul class="space-y-0.5">
						{#each workbench.remotes as remote (remote.name)}
							<li
								class="truncate font-mono text-micro text-muted-foreground"
								title={remote.url}
							>
								{remote.name} → {remote.url}
							</li>
						{/each}
					</ul>
				{/if}
				{#if workbench.tags.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						Tags
					</p>
					<ul class="space-y-0.5">
						{#each workbench.tags as tag (tag.name)}
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
			{#each workbench.visibleCommits as commit (commit.id)}
				<li>
					<button
						type="button"
						onclick={() => void workbench.selectCommit(commit.id)}
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
		{#if workbench.visibleCommits.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				No commits match the filter.
			</p>
		{/if}
		{#if workbench.commitsHasMore && workbench.visibleCommits.length === workbench.commits.length}
			<Button
				variant="ghost"
				size="sm"
				disabled={workbench.commitsLoading}
				onclick={() => void workbench.loadMoreCommits()}
			>
				{workbench.commitsLoading
					? 'Loading…'
					: `Show more (${workbench.commits.length} shown)`}
			</Button>
		{/if}
	{:else if panel === 'stash'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleStash as entry (entry.name)}
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
		{#if workbench.visibleStash.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				No stash entries match the filter.
			</p>
		{/if}
	{/if}
</Card>
