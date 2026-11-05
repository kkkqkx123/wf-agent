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
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { parseToken } from '#lib/git/parse';
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

	const BOARD_WIDTH_KEY = 'git-board-width';
	const BOARD_WIDTH_DEFAULT = 320;
	const BOARD_WIDTH_MIN = 240;
	const BOARD_WIDTH_MAX = 600;
	let boardWidth = $state(BOARD_WIDTH_DEFAULT);

	function clampBoardWidth(value: number): number {
		return Math.min(BOARD_WIDTH_MAX, Math.max(BOARD_WIDTH_MIN, Math.round(value)));
	}

	function startBoardResize(event: PointerEvent): void {
		const handle = event.currentTarget as HTMLElement | null;
		if (!handle) return;
		event.preventDefault();
		handle.setPointerCapture(event.pointerId);
		const startX = event.clientX;
		const startWidth = boardWidth;
		const onMove = (moveEvent: PointerEvent): void => {
			boardWidth = clampBoardWidth(startWidth + moveEvent.clientX - startX);
		};
		const onUp = (upEvent: PointerEvent): void => {
			try {
				localStorage.setItem(BOARD_WIDTH_KEY, String(boardWidth));
			} catch {
				// Storage unavailable; keep the in-memory width.
			}
			handle.releasePointerCapture(upEvent.pointerId);
			handle.removeEventListener('pointermove', onMove);
			handle.removeEventListener('pointerup', onUp);
			handle.removeEventListener('pointercancel', onUp);
		};
		handle.addEventListener('pointermove', onMove);
		handle.addEventListener('pointerup', onUp);
		handle.addEventListener('pointercancel', onUp);
	}

	function onBoardDividerKeydown(event: KeyboardEvent): void {
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			boardWidth = clampBoardWidth(boardWidth + (event.key === 'ArrowRight' ? 20 : -20));
			try {
				localStorage.setItem(BOARD_WIDTH_KEY, String(boardWidth));
			} catch {
				// Storage unavailable; keep the in-memory width.
			}
		} else if (event.key === 'Home') {
			event.preventDefault();
			boardWidth = BOARD_WIDTH_DEFAULT;
			try {
				localStorage.removeItem(BOARD_WIDTH_KEY);
			} catch {
				// Storage unavailable; keep the in-memory width.
			}
		}
	}

	const BOARD_ORDER: PanelId[] = [
		'status',
		'files',
		'conflicts',
		'worktrees',
		'submodules',
		'branches',
		'remotes',
		'tags',
		'commits',
		'reflog',
		'stash',
	];

	function editableTarget(target: EventTarget | null): boolean {
		const element = target as HTMLElement | null;
		if (!element || typeof element.closest !== 'function') return false;
		return element.closest('input, textarea, select, [contenteditable="true"], .xterm') !== null;
	}

	function boardVisible(): boolean {
		return document.getElementById('git-board')?.offsetParent !== null;
	}

	function selectionIdFor(panel: PanelId): string | null {
		if (panel === 'files' || panel === 'conflicts') return gitStore.selectedFile;
		if (panel === 'commits' || panel === 'reflog')
			return gitStore.selectedCommit ?? gitStore.selectionFor('commits');
		if (panel === 'branches') return gitStore.selectedBranch;
		if (panel === 'stash') return gitStore.selectedStash;
		if (panel === 'worktrees') return gitStore.selectedWorktree;
		if (panel === 'submodules') return gitStore.selectedSubmodule;
		if (panel === 'remotes') return workbench.selectedRemote;
		if (panel === 'tags') return workbench.selectedTag;
		return null;
	}

	async function selectBoardRow(panel: PanelId, id: string): Promise<void> {
		if (panel === 'files' || panel === 'conflicts') await workbench.selectFile(id, false);
		else if (panel === 'commits' || panel === 'reflog') await workbench.selectCommit(id, false);
		else if (panel === 'branches') gitStore.select('branches', id);
		else if (panel === 'stash') gitStore.select('stash', id);
		else if (panel === 'worktrees') gitStore.select('worktrees', id);
		else if (panel === 'submodules') gitStore.select('submodules', id);
		else if (panel === 'remotes') workbench.selectedRemote = id;
		else if (panel === 'tags') {
			const tag = workbench.tags.find((entry) => entry.name === id);
			workbench.selectedTag = id;
			if (tag) await workbench.selectCommit(tag.commit, false);
		}
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
		if (first) {
			first.focus();
			return;
		}
		document.getElementById(`git-filter-${BOARD_ORDER[next]}`)?.focus();
	}

	async function onPageKeydown(event: KeyboardEvent): Promise<void> {
		if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key === '/') {
			event.preventDefault();
			workbench.focusKeyInput();
			return;
		}
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
		if (event.key === 'Enter') {
			const element = event.target as HTMLElement | null;
			if (element?.closest?.('button, input, select, textarea, [contenteditable="true"], .xterm'))
				return;
			if (panel === 'files' || panel === 'conflicts') {
				event.preventDefault();
				workbench.setViewMode('staging');
				return;
			}
			if (panel === 'commits') {
				event.preventDefault();
				workbench.setViewMode(workbench.viewMode === 'commitFiles' ? 'diff' : 'commitFiles');
				return;
			}
			return;
		}
		if (event.key === 'Escape') {
			if ((panel === 'files' || panel === 'conflicts') && workbench.viewMode === 'staging') {
				workbench.setViewMode('diff');
			} else if (panel === 'commits' && workbench.viewMode === 'commitFiles') {
				workbench.setViewMode('diff');
			}
			(event.target as HTMLElement | null)?.blur?.();
			const fallback =
				document.querySelector<HTMLButtonElement>('#git-board [data-board-row]') ??
				document.querySelector<HTMLButtonElement>('#git-panel button');
			fallback?.focus();
			return;
		}
		if (event.key === '0') {
			event.preventDefault();
			document.querySelector<HTMLButtonElement>('#git-main button, #git-main input')?.focus();
			return;
		}
		if (event.key.length === 1 || event.key === ' ') {
			if (event.key === ' ') {
				const element = event.target as HTMLElement | null;
				const row = element?.closest?.('[data-stage-row]');
				if (row instanceof HTMLElement) {
					const path = row.dataset.path;
					const hunk = row.dataset.hunk;
					const line = row.dataset.line;
					if (path !== undefined && hunk !== undefined && line !== undefined) {
						event.preventDefault();
						workbench.toggleLine(path, Number(hunk), Number(line));
						return;
					}
				}
				if (element?.closest?.('button')) return;
			}
			const key = event.key === ' ' ? ' ' : event.key;
			const result = parseToken(key, panel);
			if (result.kind === 'action' && result.action) {
				event.preventDefault();
				await workbench.runActionId(result.action);
			}
		}
	}

	onMount(() => {
		try {
			const stored = localStorage.getItem(BOARD_WIDTH_KEY);
			if (stored !== null) {
				const parsed = Number(stored);
				if (Number.isFinite(parsed)) boardWidth = clampBoardWidth(parsed);
			}
		} catch {
			// Storage unavailable; keep the default width.
		}
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
				style:--git-board-width={`${boardWidth}px`}
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
					class="hidden w-2 shrink-0 cursor-col-resize items-stretch justify-center xl:flex"
					onpointerdown={startBoardResize}
					onkeydown={onBoardDividerKeydown}
					ondblclick={() => (boardWidth = BOARD_WIDTH_DEFAULT)}
				>
					<span class="w-px bg-border transition-colors hover:bg-primary/40"></span>
				</div>
				<div class="min-h-0 min-w-0 flex-1 xl:overflow-y-auto xl:py-3">
					<GitMainView {workbench} />
				</div>
			</div>
			<div class="mt-2 shrink-0 space-y-2 border-t border-border pt-2">
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
			width: var(--git-board-width, 320px);
		}
	}
</style>
