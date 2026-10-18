<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import GitGroupsPanel from '$lib/components/domain/git/GitGroupsPanel.svelte';
	import GitMainView from '$lib/components/domain/git/GitMainView.svelte';
	import GitKeyInput from '$lib/components/domain/git/GitKeyInput.svelte';
	import GitTerminalPanel from '$lib/components/domain/git/GitTerminalPanel.svelte';
	import GitActionDialogs from '$lib/components/domain/git/GitActionDialogs.svelte';
	import { page } from '$app/state';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { GitWorkbench, PANELS } from '$lib/git/workbench.svelte';
	import type { PanelId } from '$lib/git/workbench.svelte';

	const requested = parseListParams(page.url).panel;
	const workbench = new GitWorkbench();
	workbench.panel =
		requested === 'files' ||
		requested === 'worktrees' ||
		requested === 'submodules' ||
		requested === 'branches' ||
		requested === 'commits' ||
		requested === 'stash'
			? (requested as PanelId)
			: 'status';

	onMount(() => workbench.start());
	onDestroy(() => workbench.stop());

	$effect(() => {
		void gotoWithParams(page.url, {
			panel: workbench.panel === 'status' ? '' : workbench.panel,
		});
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Git"
		description="User repository workbench: status, files, branches, commits and stash."
	>
		{#snippet actions()}
			{#if workbench.meta}
				<Badge variant="neutral">{workbench.meta.branch || '(no branch)'}</Badge
				>
				{#if workbench.meta.upstream}
					<Badge variant="neutral"
						>{workbench.meta.upstream} +{workbench.meta.ahead}/-{workbench.meta
							.behind}</Badge
					>
				{/if}
				{#if workbench.meta.mergeHead || workbench.meta.rebaseHead || workbench.meta.cherryPickHead}
					<Badge variant="warning">mid-operation</Badge>
				{/if}
			{/if}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void workbench.reload()}
			/>
			<Button
				size="sm"
				variant="outline"
				onclick={() => void workbench.toggleTerminal()}
			>
				{workbench.terminalOpen ? 'Hide terminal' : 'Terminal'}
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={PANELS.map((p) => ({ id: p.id, label: p.label }))}
		bind:value={workbench.panel}
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
			loading={workbench.loading}
			error={workbench.loadError}
			empty={workbench.noRepo}
			emptyTitle="No repository"
			emptyDescription="Configure user_git.repo_root with user_git.enabled=true to attach the user repository."
			onretry={() => void workbench.reload()}
		>
			<div class="grid gap-3 xl:grid-cols-[minmax(0,280px)_minmax(0,1fr)]">
				<GitGroupsPanel {workbench} bind:panel={workbench.panel} />
				<div class="space-y-3">
					<GitMainView {workbench} />
					<GitKeyInput {workbench} />
					<GitTerminalPanel {workbench} />
				</div>
			</div>
		</PageState>
	</div>
</div>

<GitActionDialogs {workbench} />
