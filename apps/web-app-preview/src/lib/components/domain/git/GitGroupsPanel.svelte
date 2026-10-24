<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { PANELS } from '#lib/git/workbench.svelte';
	import type { GitWorkbench, PanelId } from '#lib/git/workbench.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
		panel?: PanelId;
	}

	let { workbench, panel = $bindable('status') }: Props = $props();
</script>

<Card
	title={t('domain.git.groupsTitle')}
	description={t('domain.git.groupsDescription')}
>
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
			label={t('domain.git.filter')}
			size="sm"
			bind:value={workbench.filters[panel]}
			placeholder={t('domain.git.filterPlaceholder')}
		/>
	{/if}
	{#if panel === 'status' && workbench.status}
		<ul class="mt-2 space-y-1 text-caption">
			<li class="flex justify-between">
				<span>{t('domain.git.staged')}</span><span class="font-mono"
					>{workbench.status.staged}</span
				>
			</li>
			<li class="flex justify-between">
				<span>{t('domain.git.unstaged')}</span><span class="font-mono"
					>{workbench.status.unstaged}</span
				>
			</li>
			<li class="flex justify-between">
				<span>{t('domain.git.untracked')}</span><span class="font-mono"
					>{workbench.status.untracked}</span
				>
			</li>
			<li class="flex justify-between">
				<span>{t('domain.git.conflicted')}</span>
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
						<span class="truncate font-mono text-caption">{file.path}</span>
						<span class="font-mono text-micro text-muted-foreground"
							>{file.staged}{file.unstaged}</span
						>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleFiles.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noFilesMatch')}
			</p>
		{/if}
		{#if workbench.worktrees.length > 0 || workbench.submodules.length > 0}
			<div class="mt-3 space-y-2 border-t border-border pt-2">
				{#if workbench.worktrees.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						{t('domain.git.worktreesReadOnly')}
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
						{t('domain.git.submodulesReadOnly')}
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
					{t('domain.git.worktreeHint')}
				</p>
			</div>
		{/if}
	{:else if panel === 'conflicts'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleConflicts as file (file.path)}
				<li>
					<button
						type="button"
						onclick={() => void workbench.selectFile(file.path)}
						aria-pressed={gitStore.selectionFor('files') === file.path}
						class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
							'files',
						) === file.path
							? 'bg-accent/70'
							: ''}"
					>
						<p class="truncate font-mono text-caption">{file.path}</p>
						<p class="text-micro text-muted-foreground">
							{file.staged}{file.unstaged}
						</p>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleConflicts.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noConflicted')}
			</p>
		{/if}
	{:else if panel === 'worktrees'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleWorktrees as worktree (worktree.path)}
				<li>
					<button
						type="button"
						onclick={() => gitStore.select('worktrees', worktree.path)}
						aria-pressed={gitStore.selectionFor('worktrees') === worktree.path}
						class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
							'worktrees',
						) === worktree.path
							? 'bg-accent/70'
							: ''}"
					>
						<p class="truncate font-mono text-caption">
							{worktree.path}
						</p>
						<p class="text-micro text-muted-foreground">
							{worktree.branch || '(detached)'}{worktree.bare ? ' · bare' : ''} ·
							{worktree.commit}
						</p>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleWorktrees.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noWorktreesMatch')}
			</p>
		{/if}
	{:else if panel === 'submodules'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleSubmodules as submodule (submodule.path)}
				<li>
					<button
						type="button"
						onclick={() => gitStore.select('submodules', submodule.path)}
						aria-pressed={gitStore.selectionFor('submodules') ===
							submodule.path}
						class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
							'submodules',
						) === submodule.path
							? 'bg-accent/70'
							: ''}"
					>
						<p class="truncate font-mono text-caption">
							{submodule.path}
						</p>
						<p class="text-micro text-muted-foreground">
							{submodule.commit} · {submodule.status}
						</p>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleSubmodules.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noSubmodulesMatch')}
			</p>
		{/if}
	{:else if panel === 'remotes'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleRemotes as remote (remote.name)}
				<li>
					<p class="truncate font-mono text-caption">{remote.name}</p>
					<p class="truncate text-micro text-muted-foreground">{remote.url}</p>
					{#if remote.branches.length > 0}
						<p class="text-micro text-muted-foreground">
							{remote.branches.join(', ')}
						</p>
					{/if}
				</li>
			{/each}
		</ul>
		{#if workbench.visibleRemotes.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noRemotesMatch')}
			</p>
		{/if}
	{:else if panel === 'tags'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleTags as tag (tag.name)}
				<li>
					<button
						type="button"
						onclick={() => gitStore.select('commits', tag.commit)}
						class="block w-full px-2 py-1 text-left hover:bg-accent/50"
					>
						<p class="truncate font-mono text-caption">{tag.name}</p>
						<p class="text-micro text-muted-foreground">{tag.commit}</p>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleTags.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noTagsMatch')}
			</p>
		{/if}
	{:else if panel === 'branches'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleBranches as branch (branch.name)}
				<li>
					<button
						type="button"
						onclick={() => gitStore.select('branches', branch.name)}
						aria-pressed={gitStore.selectionFor('branches') === branch.name}
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
				{t('domain.git.noBranchesMatch')}
			</p>
		{/if}
		{#if workbench.remotes.length > 0 || workbench.tags.length > 0}
			<div class="mt-3 space-y-2 border-t border-border pt-2">
				{#if workbench.remotes.length > 0}
					<p class="text-micro font-medium text-muted-foreground">
						{t('domain.git.remotes')}
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
						{t('domain.git.tags')}
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
						aria-pressed={gitStore.selectionFor('commits') === commit.id}
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
	{:else if panel === 'reflog'}
		<ul class="mt-2 space-y-1">
			{#each workbench.visibleReflogs as entry (entry.selector)}
				<li>
					<button
						type="button"
						onclick={() => void workbench.selectCommit(entry.id)}
						aria-pressed={gitStore.selectionFor('commits') === entry.id}
						class="block w-full px-2 py-1 text-left hover:bg-accent/50 {gitStore.selectionFor(
							'commits',
						) === entry.id
							? 'bg-accent/70'
							: ''}"
					>
						<p class="truncate font-mono text-caption">
							{entry.shortId}
							{entry.message}
						</p>
						<p class="text-micro text-muted-foreground">
							{entry.selector}
						</p>
					</button>
				</li>
			{/each}
		</ul>
		{#if workbench.visibleReflogs.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noReflogsMatch')}
			</p>
		{/if}
		{#if workbench.visibleCommits.length === 0}
			<p class="mt-2 text-caption text-muted-foreground">
				{t('domain.git.noCommitsMatch')}
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
					? t('common.loading')
					: t('domain.git.showMoreCommits', {
							shown: workbench.commits.length,
						})}
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
				{t('domain.git.noStashMatch')}
			</p>
		{/if}
	{/if}
</Card>
