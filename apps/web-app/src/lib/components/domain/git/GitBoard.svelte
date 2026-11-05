<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import BoardListPanel, { type BoardRow } from '#lib/components/domain/git/BoardListPanel.svelte';
	import type { GitWorkbench, PanelId } from '#lib/git/workbench.svelte';
	import { PANELS, PANEL_GROUPS, groupForPanel } from '#lib/git/workbench.svelte';
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

	let fileRows = $derived<BoardRow[]>(
		workbench.files
			.filter((file) => file.path.includes(workbench.filters.files))
			.map((file) => ({
				id: file.path,
				label: file.renamedFrom ? `${file.path} → ${file.renamedFrom}` : file.path,
				labelClass: fileRowClass(file),
				markers: [
					{ text: file.staged, class: markerClass(file.staged, 'staged') },
					{ text: file.unstaged, class: markerClass(file.unstaged, 'unstaged') },
				],
			})),
	);
	let conflictRows = $derived<BoardRow[]>(
		workbench.files
			.filter(
				(file) => file.kind === 'conflicted' && file.path.includes(workbench.filters.conflicts),
			)
			.map((file) => ({
				id: file.path,
				label: file.path,
				labelClass: fileRowClass(file),
				markers: [{ text: '!!', class: 'text-destructive' }],
			})),
	);
	let worktreeRows = $derived<BoardRow[]>(
		workbench.worktrees
			.filter(
				(entry) =>
					entry.path.includes(workbench.filters.worktrees) ||
					entry.branch.includes(workbench.filters.worktrees),
			)
			.map((entry) => ({
				id: entry.path,
				label: entry.path,
				detail: `${entry.branch || '(detached)'}${entry.bare ? ' · bare' : ''}`,
			})),
	);
	let submoduleRows = $derived<BoardRow[]>(
		workbench.submodules
			.filter(
				(entry) =>
					entry.path.includes(workbench.filters.submodules) ||
					entry.commit.includes(workbench.filters.submodules),
			)
			.map((entry) => ({
				id: entry.path,
				label: entry.path,
				detail: entry.commit,
			})),
	);
	let branchRows = $derived<BoardRow[]>(
		workbench.branches
			.filter((branch) => branch.name.includes(workbench.filters.branches))
			.map((branch) => ({
				id: branch.name,
				label: branch.name,
				labelClass: branchRowClass(branch),
				markers: [{ text: branch.current ? '*' : ' ' }],
				badge: branch.upstream ? `+${branch.ahead}/-${branch.behind}` : undefined,
			})),
	);
	let remoteRows = $derived<BoardRow[]>(
		workbench.remotes
			.filter(
				(remote) =>
					remote.name.includes(workbench.filters.remotes) ||
					remote.url.includes(workbench.filters.remotes),
			)
			.map((remote) => ({
				id: remote.name,
				label: remote.name,
				detail: remote.url,
			})),
	);
	let tagRows = $derived<BoardRow[]>(
		workbench.tags
			.filter(
				(tag) =>
					tag.name.includes(workbench.filters.tags) || tag.commit.includes(workbench.filters.tags),
			)
			.map((tag) => ({
				id: tag.name,
				label: tag.name,
				detail: tag.commit,
			})),
	);
	let commitRows = $derived<BoardRow[]>(
		workbench.commits
			.filter(
				(commit) =>
					commit.subject.includes(workbench.filters.commits) ||
					commit.shortId.includes(workbench.filters.commits),
			)
			.map((commit) => ({
				id: commit.id,
				label: commit.subject,
				labelClass: commitRowClass(commit),
				markers: [{ text: commit.shortId, class: 'text-info' }],
				badge: commit.refs.length > 0 ? commit.refs.join(' ') : undefined,
			})),
	);
	let reflogRows = $derived<BoardRow[]>(
		workbench.reflogs
			.filter(
				(entry) =>
					entry.message.includes(workbench.filters.reflog) ||
					entry.shortId.includes(workbench.filters.reflog) ||
					entry.selector.includes(workbench.filters.reflog),
			)
			.map((entry) => ({
				id: entry.selector,
				label: entry.message,
				markers: [{ text: entry.shortId, class: 'text-info' }],
				detail: entry.selector,
			})),
	);
	let stashRows = $derived<BoardRow[]>(
		workbench.stash
			.filter(
				(entry) =>
					entry.message.includes(workbench.filters.stash) ||
					entry.name.includes(workbench.filters.stash),
			)
			.map((entry) => ({
				id: entry.name,
				label: entry.message,
				labelClass: stashRowClass(),
				markers: [{ text: entry.name }],
			})),
	);

	let lanes = $derived(
		assignGraphLanes(
			commitRows.map((row) => {
				const commit = workbench.commits.find((entry) => entry.id === row.id);
				return { id: row.id, parents: commit?.parents ?? [] };
			}),
		),
	);
	let laneById = $derived(new Map(lanes.map((row) => [row.id, row] as const)));

	function panelLabel(id: PanelId): string {
		return PANELS.find((entry) => entry.id === id)?.label ?? id;
	}

	/** One visible panel per group; the active panel wins, otherwise the group head shows. */
	function shownPanel(groupId: string): PanelId {
		const group = PANEL_GROUPS.find((entry) => entry.id === groupId);
		if (!group) return workbench.panel;
		return group.panels.includes(workbench.panel) ? workbench.panel : group.panels[0];
	}

	function boardVisibility(panel: PanelId): string {
		return shownPanel(groupForPanel(panel).id) === panel ? '' : 'hidden';
	}
</script>

<div id="git-board" class="flex flex-col gap-2">
	<div class={boardVisibility('status')} onfocusin={() => (workbench.panel = 'status')}>
		<TuiPanel title="Status" focused={workbench.panel === 'status'}>
			{#if workbench.status}
				<ul class="mt-1 font-mono text-micro">
					<li class="flex justify-between px-1 py-0.5">
						<span class="text-muted-foreground">{t('domain.git.staged')}</span><span
							class="text-success">{workbench.status.staged}</span
						>
					</li>
					<li class="flex justify-between px-1 py-0.5">
						<span class="text-muted-foreground">{t('domain.git.unstaged')}</span><span
							class="text-warning">{workbench.status.unstaged}</span
						>
					</li>
					<li class="flex justify-between px-1 py-0.5">
						<span class="text-muted-foreground">{t('domain.git.untracked')}</span><span
							>{workbench.status.untracked}</span
						>
					</li>
					<li class="flex justify-between px-1 py-0.5">
						<span class="text-muted-foreground">{t('domain.git.conflicted')}</span>
						<span class={workbench.status.conflicted > 0 ? 'font-semibold text-destructive' : ''}
							>{workbench.status.conflicted}</span
						>
					</li>
				</ul>
			{/if}
		</TuiPanel>
	</div>

	<div role="tablist" aria-label={t('domain.git.workspacePanels')} class="flex flex-wrap gap-1 px-1">
		{#each PANEL_GROUPS.find((entry) => entry.id === 'workspace')?.panels ?? [] as id (id)}
			<button
				type="button"
				role="tab"
				onclick={() => (workbench.panel = id)}
				aria-selected={workbench.panel === id}
				class="rounded-sm px-1.5 py-0.5 font-mono text-micro {workbench.panel === id ? 'bg-accent font-semibold text-foreground' : 'text-muted-foreground hover:bg-accent/50'}"
			>
				{panelLabel(id)}
			</button>
		{/each}
	</div>
	<div class={boardVisibility('files')} onfocusin={() => (workbench.panel = 'files')}>
		<BoardListPanel
			{workbench}
			panel="files"
			title="Files"
			rows={fileRows}
			selectedId={workbench.selectedIdFor('files')}
			emptyText={t('domain.git.noFilesMatch')}
			filterId="git-filter-files"
			bind:filterValue={workbench.filters.files}
		>
			{#snippet footer()}
				{#if workbench.status}
					<span>
						{t('domain.git.staged')}: {workbench.status.staged} · {t('domain.git.unstaged')}:
						{workbench.status.unstaged} · {t('domain.git.untracked')}: {workbench.status.untracked}
						· {t('domain.git.conflicted')}: {workbench.status.conflicted}
					</span>
				{/if}
			{/snippet}
		</BoardListPanel>
	</div>

	<div class={boardVisibility('conflicts')} onfocusin={() => (workbench.panel = 'conflicts')}>
		<BoardListPanel
			{workbench}
			panel="conflicts"
			title="Conflicts"
			rows={conflictRows}
			selectedId={workbench.selectedIdFor('conflicts')}
			emptyText={t('domain.git.noConflicted')}
			filterId="git-filter-conflicts"
			bind:filterValue={workbench.filters.conflicts}
		/>
	</div>

	<div class={boardVisibility('worktrees')} onfocusin={() => (workbench.panel = 'worktrees')}>
		<BoardListPanel
			{workbench}
			panel="worktrees"
			title="Worktrees"
			rows={worktreeRows}
			selectedId={workbench.selectedIdFor('worktrees')}
			emptyText={t('domain.git.noWorktreesMatch')}
			filterId="git-filter-worktrees"
			bind:filterValue={workbench.filters.worktrees}
		/>
	</div>

	<div class={boardVisibility('submodules')} onfocusin={() => (workbench.panel = 'submodules')}>
		<BoardListPanel
			{workbench}
			panel="submodules"
			title="Submodules"
			rows={submoduleRows}
			selectedId={workbench.selectedIdFor('submodules')}
			emptyText={t('domain.git.noSubmodulesMatch')}
			filterId="git-filter-submodules"
			bind:filterValue={workbench.filters.submodules}
		/>
	</div>

	<div role="tablist" aria-label={t('domain.git.referencePanels')} class="flex flex-wrap gap-1 px-1">
		{#each PANEL_GROUPS.find((entry) => entry.id === 'refs')?.panels ?? [] as id (id)}
			<button
				type="button"
				role="tab"
				onclick={() => (workbench.panel = id)}
				aria-selected={workbench.panel === id}
				class="rounded-sm px-1.5 py-0.5 font-mono text-micro {workbench.panel === id ? 'bg-accent font-semibold text-foreground' : 'text-muted-foreground hover:bg-accent/50'}"
			>
				{panelLabel(id)}
			</button>
		{/each}
	</div>
	<div class={boardVisibility('branches')} onfocusin={() => (workbench.panel = 'branches')}>
		<BoardListPanel
			{workbench}
			panel="branches"
			title="Branches"
			rows={branchRows}
			selectedId={workbench.selectedIdFor('branches')}
			emptyText={t('domain.git.noBranchesMatch')}
			filterId="git-filter-branches"
			bind:filterValue={workbench.filters.branches}
		/>
	</div>

	<div class={boardVisibility('remotes')} onfocusin={() => (workbench.panel = 'remotes')}>
		<BoardListPanel
			{workbench}
			panel="remotes"
			title="Remotes"
			rows={remoteRows}
			selectedId={workbench.selectedIdFor('remotes')}
			emptyText={t('domain.git.noRemotesMatch')}
			filterId="git-filter-remotes"
			bind:filterValue={workbench.filters.remotes}
		/>
	</div>

	<div class={boardVisibility('tags')} onfocusin={() => (workbench.panel = 'tags')}>
		<BoardListPanel
			{workbench}
			panel="tags"
			title="Tags"
			rows={tagRows}
			selectedId={workbench.selectedIdFor('tags')}
			emptyText={t('domain.git.noTagsMatch')}
			filterId="git-filter-tags"
			bind:filterValue={workbench.filters.tags}
		/>
	</div>

	<div role="tablist" aria-label={t('domain.git.historyPanels')} class="flex flex-wrap gap-1 px-1">
		{#each PANEL_GROUPS.find((entry) => entry.id === 'history')?.panels ?? [] as id (id)}
			<button
				type="button"
				role="tab"
				onclick={() => (workbench.panel = id)}
				aria-selected={workbench.panel === id}
				class="rounded-sm px-1.5 py-0.5 font-mono text-micro {workbench.panel === id ? 'bg-accent font-semibold text-foreground' : 'text-muted-foreground hover:bg-accent/50'}"
			>
				{panelLabel(id)}
			</button>
		{/each}
	</div>
	<div class={boardVisibility('commits')} onfocusin={() => (workbench.panel = 'commits')}>
		<BoardListPanel
			{workbench}
			panel="commits"
			title="Commits"
			rows={commitRows}
			selectedId={workbench.selectedIdFor('commits')}
			emptyText={t('domain.git.noCommitsMatch')}
			filterId="git-filter-commits"
			bind:filterValue={workbench.filters.commits}
		>
			{#snippet leading(row)}
				{@const lane = laneById.get(row.id)}
				{#if lane}
					<span class="flex shrink-0 items-stretch self-stretch" aria-hidden="true">
						{#each lane.active as active, laneIndex (laneIndex)}
							<span class="relative w-2.5">
								{#if active}
									<span class="absolute top-0 bottom-0 left-1/2 w-px bg-muted-foreground/60"></span>
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
			{/snippet}
			{#snippet footer()}
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
			{/snippet}
		</BoardListPanel>
	</div>

	<div class={boardVisibility('reflog')} onfocusin={() => (workbench.panel = 'reflog')}>
		<BoardListPanel
			{workbench}
			panel="reflog"
			title="Reflog"
			rows={reflogRows}
			selectedId={workbench.selectedIdFor('reflog')}
			emptyText={t('domain.git.noReflogsMatch')}
			filterId="git-filter-reflog"
			bind:filterValue={workbench.filters.reflog}
		/>
	</div>

	<div class={boardVisibility('stash')} onfocusin={() => (workbench.panel = 'stash')}>
		<BoardListPanel
			{workbench}
			panel="stash"
			title="Stash"
			rows={stashRows}
			selectedId={workbench.selectedIdFor('stash')}
			emptyText={t('domain.git.noStashMatch')}
			filterId="git-filter-stash"
			bind:filterValue={workbench.filters.stash}
		/>
	</div>
</div>
