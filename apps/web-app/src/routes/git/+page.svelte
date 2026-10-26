<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import GitBoard from '#lib/components/domain/git/GitBoard.svelte';
	import GitGroupsPanel from '#lib/components/domain/git/GitGroupsPanel.svelte';
	import GitMainView from '#lib/components/domain/git/GitMainView.svelte';
	import GitKeyInput from '#lib/components/domain/git/GitKeyInput.svelte';
	import GitStatusBar from '#lib/components/domain/git/GitStatusBar.svelte';

	import GitActionDialogs from '#lib/components/domain/git/GitActionDialogs.svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { GitWorkbench, PANELS } from '#lib/git/workbench.svelte';
	import type { PanelId } from '#lib/git/workbench.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { parseToken } from '#lib/git/parse';
	import { t } from '#lib/i18n';

	const requested = parseListParams(page.url as unknown as URL).panel;
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

	const BOARD_ORDER: PanelId[] = ['files', 'branches', 'commits', 'stash'];

	function editableTarget(target: EventTarget | null): boolean {
		const element = target as HTMLElement | null;
		if (!element || typeof element.closest !== 'function') return false;
		return element.closest('input, textarea, select, [contenteditable="true"], .xterm') !== null;
	}

	function boardVisible(): boolean {
		return document.getElementById('git-board')?.offsetParent !== null;
	}

	function selectionIdFor(panel: PanelId): string | null {
		if (panel === 'files') return gitStore.selectedFile;
		if (panel === 'commits') return gitStore.selectedCommit;
		if (panel === 'branches') return gitStore.selectedBranch;
		if (panel === 'stash') return gitStore.selectedStash;
		return null;
	}

	async function selectBoardRow(panel: PanelId, id: string): Promise<void> {
		if (panel === 'files') await workbench.selectFile(id, false);
		else if (panel === 'commits') await workbench.selectCommit(id, false);
		else if (panel === 'branches') gitStore.select('branches', id);
		else if (panel === 'stash') gitStore.select('stash', id);
	}

	function focusBoardRow(panel: PanelId, id: string): void {
		document
			.querySelector<HTMLButtonElement>(
				`#git-board [data-board-panel="${panel}"][data-board-id="${CSS.escape(id)}"]`,
			)
			?.focus();
	}

	async function stepBoardRow(panel: PanelId, delta: number): Promise<void> {
		const rows = Array.from(
			document.querySelectorAll<HTMLButtonElement>(`#git-board [data-board-panel="${panel}"]`),
		);
		if (rows.length === 0) return;
		const ids = rows.map((row) => row.dataset.boardId ?? '');
		const current = selectionIdFor(panel);
		const index = current ? ids.indexOf(current) : -1;
		const next =
			index < 0
				? delta > 0
					? 0
					: rows.length - 1
				: Math.min(rows.length - 1, Math.max(0, index + delta));
		await selectBoardRow(panel, ids[next]);
		focusBoardRow(panel, ids[next]);
	}

	function cycleBoardPanel(delta: number): void {
		const index = BOARD_ORDER.indexOf(workbench.panel);
		const next = index < 0 ? 0 : (index + delta + BOARD_ORDER.length) % BOARD_ORDER.length;
		workbench.panel = BOARD_ORDER[next];
		const first = document.querySelector<HTMLButtonElement>(
			`#git-board [data-board-panel="${BOARD_ORDER[next]}"]`,
		);
		first?.focus();
	}

	async function onPageKeydown(event: KeyboardEvent): Promise<void> {
		if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
		if (workbench.dialogOpen || workbench.helpOpen) return;
		if (editableTarget(event.target)) return;
		const target = event.target as HTMLElement | null;
		const inBoard = target?.closest?.('#git-board') != null;
		const panel = workbench.panel;

		if (event.key === 'Tab' && inBoard && BOARD_ORDER.includes(panel)) {
			event.preventDefault();
			cycleBoardPanel(event.shiftKey ? -1 : 1);
			return;
		}
		if (
			(event.key === 'j' ||
				event.key === 'k' ||
				event.key === 'ArrowDown' ||
				event.key === 'ArrowUp') &&
			inBoard &&
			BOARD_ORDER.includes(panel) &&
			boardVisible()
		) {
			event.preventDefault();
			const delta = event.key === 'j' || event.key === 'ArrowDown' ? 1 : -1;
			await stepBoardRow(panel, delta);
			return;
		}
		if (event.key === '?') {
			workbench.helpOpen = true;
			return;
		}
		if (event.key === '/') {
			event.preventDefault();
			workbench.focusFilter();
			return;
		}
		if (event.key.length === 1 || event.key === ' ') {
			const key = event.key === ' ' ? ' ' : event.key;
			const result = parseToken(key, panel);
			if (result.kind === 'action' && result.action) {
				event.preventDefault();
				await workbench.runActionId(result.action);
			}
		}
	}

	onMount(() => {
		workbench.start();
		window.addEventListener('keydown', onPageKeydown);
	});
	onDestroy(() => {
		window.removeEventListener('keydown', onPageKeydown);
		workbench.stop();
	});

	function gotoOpenInIde(selection: string | null): void {
		const target = selection ? `/ide?path=${encodeURIComponent(selection)}` : '/ide';
		void goto(target);
	}

	$effect(() => {
		void gotoWithParams(page.url as unknown as URL, {
			panel: workbench.panel === 'status' ? '' : workbench.panel,
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

	<Segmented
		items={PANELS.map((p) => ({ id: p.id, label: p.label }))}
		bind:value={workbench.panel}
		class="px-4"
		panelId="git-panel"
	/>

	<div
		id="git-panel"
		role="tabpanel"
		aria-label={t('page.git.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		<PageState
			loading={workbench.loading}
			error={workbench.loadError}
			empty={workbench.noRepo}
			emptyTitle={t('page.git.noRepoTitle')}
			emptyDescription={t('page.git.noRepoDesc')}
			onretry={() => void workbench.reload()}
		>
			<div class="grid gap-2 xl:grid-cols-5">
				<div class="hidden xl:col-span-3 xl:block">
					<GitBoard {workbench} />
				</div>
				<div class="xl:hidden">
					<GitGroupsPanel {workbench} bind:panel={workbench.panel} />
				</div>
				<div class="space-y-2 xl:col-span-2">
					<GitMainView {workbench} />
					<GitKeyInput {workbench} />
					{#if workbench.terminalOpen}
						{#await import('#lib/components/domain/git/GitTerminalPanel.svelte') then module}
							<module.default {workbench} />
						{/await}
					{/if}
				</div>
			</div>
			<div class="mt-2">
				<GitStatusBar {workbench} />
			</div>
		</PageState>
	</div>
</div>

<GitActionDialogs {workbench} />
