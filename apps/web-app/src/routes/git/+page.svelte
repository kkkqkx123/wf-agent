<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import GitBoard from '#lib/components/domain/git/GitBoard.svelte';
	import GitCommandLog from '#lib/components/domain/git/GitCommandLog.svelte';
	import GitMainView from '#lib/components/domain/git/GitMainView.svelte';
	import GitKeyInput from '#lib/components/domain/git/GitKeyInput.svelte';
	import GitStatusBar from '#lib/components/domain/git/GitStatusBar.svelte';

	import GitActionDialogs from '#lib/components/domain/git/GitActionDialogs.svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { buildIdeUrl } from '#lib/utils/ide-url';
	import {
		GitWorkbench,
		defaultViewModeForPanel,
		isViewModeValidForPanel,
	} from '#lib/git/workbench.svelte';
	import type { MainViewMode, PanelId } from '#lib/git/workbench.svelte';
	import { BoardResize } from '#lib/git/board-resize.svelte';
	import { createPageKeyHandler } from '#lib/git/keyboard';
	import { t } from '#lib/i18n';

	const requested = parseListParams(page.url as unknown as URL).panel;
	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	const workbench = new GitWorkbench();
	workbench.panel =
		requested === 'files' ||
		requested === 'conflicts' ||
		requested === 'worktrees' ||
		requested === 'remotes' ||
		requested === 'tags' ||
		requested === 'submodules' ||
		requested === 'branches' ||
		requested === 'commits' ||
		requested === 'reflog' ||
		requested === 'stash'
			? (requested as PanelId)
			: 'status';
	const requestedMode = requestedTab as MainViewMode;
	if (
		(requestedMode === 'diff' || requestedMode === 'staging' || requestedMode === 'commitFiles') &&
		isViewModeValidForPanel(requestedMode, workbench.panel)
	) {
		workbench.viewMode = requestedMode;
	} else {
		workbench.viewMode = defaultViewModeForPanel(workbench.panel);
	}
	let lastPanel = $state(workbench.panel);

	const boardResize = new BoardResize({
		storageKey: 'git-board-width',
		default: 1 / 3,
		min: 0.2,
		max: 0.5,
	});

	function focusBoardRow(panel: PanelId, id: string): void {
		document
			.querySelector<HTMLButtonElement>(
				`#git-board [data-board-panel="${panel}"][data-board-id="${CSS.escape(id)}"]`,
			)
			?.focus();
	}

	function focusFirstBoardRow(panel: PanelId): void {
		const first = document.querySelector<HTMLButtonElement>(
			`#git-board [data-board-panel="${panel}"]`,
		);
		if (first) {
			first.focus();
			return;
		}
		document.getElementById(`git-filter-${panel}`)?.focus();
	}

	const onPageKeydown = createPageKeyHandler(workbench, { focusBoardRow, focusFirstBoardRow });

	onMount(() => {
		workbench.start();
		window.addEventListener('keydown', onPageKeydown);
	});
	onDestroy(() => {
		window.removeEventListener('keydown', onPageKeydown);
		workbench.stop();
	});

	function gotoOpenInIde(selection: string | null): void {
		if (!selection) {
			void goto(resolve('/ide'));
			return;
		}
		void goto(buildIdeUrl(selection));
	}

	$effect(() => {
		const currentPanel = workbench.panel;
		if (currentPanel !== lastPanel) {
			lastPanel = currentPanel;
			workbench.setViewMode(defaultViewModeForPanel(currentPanel));
		}
	});

	$effect(() => {
		void gotoWithParams(page.url as unknown as URL, {
			panel: workbench.panel === 'status' ? '' : workbench.panel,
			tab: workbench.viewMode,
		});
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.git.title')} description={t('page.git.description')}>
		{#snippet actions()}
			{#if workbench.meta}
				<Badge variant="neutral">{workbench.meta.branch || '(no branch)'}</Badge>
				{#if workbench.meta.upstream}
					<Badge variant="neutral"
						>{workbench.meta.upstream} +{workbench.meta.ahead}/-{workbench.meta.behind}</Badge
					>
				{/if}
				{#if workbench.meta.mergeHead || workbench.meta.rebaseHead || workbench.meta.cherryPickHead}
					<Badge variant="warning">{t('page.git.midOperation')}</Badge>
				{/if}
			{/if}
			<IconButton
				icon="refresh"
				label={t('common.refresh')}
				onclick={() => void workbench.reload()}
			/>
			<Button size="sm" variant="outline" onclick={() => void workbench.toggleTerminal()}>
				{workbench.terminalOpen ? t('page.git.hideTerminal') : t('page.git.terminal')}
			</Button>
			<Button size="sm" variant="outline" onclick={() => void gotoOpenInIde(workbench.selection)}>
				{t('page.git.openInIde')}
			</Button>
		{/snippet}
	</PageHeader>

	<div
		id="git-panel"
		role="tabpanel"
		aria-label={t('page.git.sections')}
		data-layout-focus={workbench.layoutFocus}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3 xl:overflow-hidden"
	>
		<PageState
			loading={workbench.loading}
			error={workbench.loadError}
			empty={workbench.noRepo}
			emptyTitle={t('page.git.noRepoTitle')}
			emptyDescription={t('page.git.noRepoDesc')}
			onretry={() => void workbench.reload()}
		>
			<div
				class="flex flex-col gap-2 xl:h-full xl:flex-row"
				style:--git-board-width={boardResize.cssValue}
			>
				<div class="git-board-pane min-h-0 shrink-0 xl:overflow-y-auto xl:py-3">
					<GitBoard {workbench} />
				</div>
				<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<div
					role="separator"
					aria-orientation="vertical"
					aria-label={t('page.git.sections')}
					tabindex="0"
					title={t('page.git.sections')}
					class="git-board-separator hidden w-2 shrink-0 cursor-col-resize items-stretch justify-center xl:flex"
					onpointerdown={(event) => boardResize.startDrag(event)}
					onkeydown={(event) => boardResize.onKeydown(event)}
					ondblclick={() => boardResize.reset()}
				>
					<span class="w-px bg-border transition-colors hover:bg-primary/40"></span>
				</div>
				<div class="git-main-pane min-h-0 min-w-0 flex-1 xl:overflow-y-auto xl:py-3">
					<GitMainView {workbench} />
				</div>
			</div>
			<div class="git-bottom mt-2 shrink-0 space-y-2 border-t border-border pt-2">
				<GitKeyInput {workbench} />
				<GitStatusBar {workbench} />
				{#if workbench.terminalOpen}
					{#await import('#lib/components/domain/git/GitTerminalPanel.svelte') then module}
						<module.default {workbench} />
					{/await}
				{/if}
				{#if workbench.actionLog.length > 0}
					<GitCommandLog {workbench} />
				{/if}
			</div>
		</PageState>
	</div>
</div>

<GitActionDialogs {workbench} />

<style>
	.git-board-pane {
		width: auto;
	}
	@media (min-width: 1280px) {
		.git-board-pane {
			width: var(--git-board-width, 33.333%);
		}
		#git-panel[data-layout-focus='board'] .git-board-pane {
			width: min(45%, calc(var(--git-board-width, 33.333%) * 1.4));
		}
		#git-panel[data-layout-focus='main'] .git-board-pane,
		#git-panel[data-layout-focus='main'] .git-board-separator {
			display: none;
		}
		#git-panel[data-layout-focus='terminal'] .git-bottom :global(.h-72) {
			height: min(50vh, 480px);
		}
	}
</style>
