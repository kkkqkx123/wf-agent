<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import ResizeHandle from '#lib/components/layout/ResizeHandle.svelte';
	import FileTree from '#lib/components/domain/ide/FileTree.svelte';
	import FileEditor from '#lib/components/domain/ide/FileEditor.svelte';
	import IdeSearchPanel from '#lib/components/domain/ide/IdeSearchPanel.svelte';
	import IdeHistoryPanel from '#lib/components/domain/ide/IdeHistoryPanel.svelte';
	import IdeTerminalPanel from '#lib/components/domain/ide/IdeTerminalPanel.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import { getMeta, getDiff } from '#lib/services/git';
	import { statFs, readFsRaw, downloadFsBlob } from '#lib/services/fs';
	import { parentOf, pasteClipboard, selectAllLoaded, uploadFiles } from '#lib/ide/clipboard';
	import { diffToLineMap, type GutterMark } from '#lib/ide/git-diff-lines';
	import type { DiffTarget } from '#lib/ide/diff-tabs';
	import { parseIdeQuery } from '#lib/utils/ide-url';
	import { preloadIdeEditor } from '#lib/ide/editor-models';
	import { registerIdeShortcuts } from '#lib/ide/ide-shortcuts';
	import { t } from '#lib/i18n';

	let loading = $state(true);
	/** Background refetch keeps the editor mounted; only the first load swaps in the skeleton. */
	let refreshing = $state(false);
	let loadError = $state<string | null>(null);
	let noRepo = $state(false);
	let pendingClose = $state<string | null>(null);
	let ideRoot: HTMLElement | null = $state(null);
	let lastQueryKey = $state<string | null>(null);
	let dropNotice = $state<string | null>(null);

	let gutterMarks = $state<GutterMark[]>([]);
	let changeLines = $state<number[]>([]);
	let changeIndex = $state(-1);
	let diffOverflow = $state(false);

	async function reload(): Promise<void> {
		// Manual refreshes keep the editor, tree and terminal mounted; the
		// skeleton only shows before the first successful load settles.
		const background = !loading && (ide.root !== '' || ide.tabs.length > 0);
		if (background) {
			refreshing = true;
		} else {
			loading = true;
			loadError = null;
		}
		try {
			const meta = await getMeta();
			ide.setRoot(meta.root);
			noRepo = false;
			await ide.loadDir('', true);
			void ide.refreshGitStatus();
			const requested = parseIdeQuery(page.url.searchParams);
			if (requested) {
				lastQueryKey = `${requested.path}:${requested.line ?? ''}:${requested.col ?? ''}`;
				await ide.openFile(requested.path, false, requested.line, requested.col);
			} else {
				lastQueryKey = null;
				if (ide.activePath) await ide.loadFile(ide.activePath);
			}
		} catch (e) {
			const message = e instanceof Error ? e.message : String(e);
			const noRepoHint =
				message.includes('no payload') ||
				message.includes('user-git') ||
				message.includes('workspace');
			if (background) {
				toasts.error(t('page.ide.title'), message);
				return;
			}
			if (noRepoHint) {
				noRepo = true;
			} else {
				loadError = message;
			}
		} finally {
			loading = false;
			refreshing = false;
		}
	}

	onMount(() => {
		void reload();
		// Warm the editor runtime while idle so the first file open never
		// pays the module transform.
		const win = window as Window & {
			requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
		};
		if (typeof win.requestIdleCallback === 'function') {
			win.requestIdleCallback(() => preloadIdeEditor(), { timeout: 3000 });
		} else {
			window.setTimeout(() => preloadIdeEditor(), 1500);
		}
	});

	function editingFocused(): boolean {
		const el = document.activeElement as HTMLElement | null;
		if (!el) return false;
		if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return true;
		if (el.isContentEditable) return true;
		if (typeof el.closest === 'function' && el.closest('.monaco-editor')) return true;
		return false;
	}

	function focusSearch(tab: 'files' | 'content'): void {
		ide.setSidebarOpen(true);
		ide.sidebarView = 'search';
		ide.searchTab = tab;
	}

	function refreshTree(): void {
		void ide.loadDir('', true);
		void ide.refreshGitStatus();
	}

	function clipboardTargets(): string[] {
		if (ide.selectedPaths.length > 0) return [...ide.selectedPaths];
		const active = ide.active();
		return active ? [active.path] : [];
	}

	function pasteDestination(): string {
		const selected = ide.selectedPaths;
		if (selected.length === 1) {
			const only = selected[0] ?? '';
			const state = ide.dirState(only);
			if (state.entries !== null || only === '') return only;
			return parentOf(only);
		}
		const active = ide.active();
		return active ? parentOf(active.path) : '';
	}

	async function refreshActiveDiff(): Promise<void> {
		const path = ide.activePath;
		if (!path) {
			gutterMarks = [];
			changeLines = [];
			changeIndex = -1;
			diffOverflow = false;
			return;
		}
		try {
			const diffs = await getDiff(path);
			const map = diffToLineMap(diffs);
			if (ide.activePath !== path) return;
			gutterMarks = map.marks;
			changeLines = map.changeLines;
			diffOverflow = map.overflow;
			changeIndex = map.changeLines.length > 0 ? 0 : -1;
		} catch {
			if (ide.activePath !== path) return;
			gutterMarks = [];
			changeLines = [];
			changeIndex = -1;
			diffOverflow = false;
		}
	}

	$effect(() => {
		void ide.activePath;
		void ide.gitRefreshSeq;
		void refreshActiveDiff();
	});

	function stepChange(direction: 1 | -1): void {
		const path = ide.activePath;
		if (!path || changeLines.length === 0) return;
		const next =
			((((changeIndex < 0 ? 0 : changeIndex) + direction) % changeLines.length) +
				changeLines.length) %
			changeLines.length;
		changeIndex = next;
		const line = changeLines[next];
		if (line !== undefined) void ide.openFile(path, false, line, 1);
	}

	function openDiffTab(target: DiffTarget): void {
		ide.openDiff(target);
	}

	async function downloadTargets(paths: string[]): Promise<void> {
		if (paths.length === 0) return;
		for (const path of paths) {
			try {
				const raw = await readFsRaw(path);
				downloadFsBlob(raw.filename, raw.mime, raw.content_base64);
			} catch (e) {
				toasts.error(t('page.ide.download'), e instanceof Error ? e.message : undefined);
				return;
			}
		}
	}

	function onEditorDragOver(event: DragEvent): void {
		if (Array.from(event.dataTransfer?.types ?? []).includes('Files')) {
			event.preventDefault();
		}
	}

	async function onEditorDrop(event: DragEvent): Promise<void> {
		const files = event.dataTransfer ? Array.from(event.dataTransfer.files ?? []) : [];
		if (files.length === 0) return;
		event.preventDefault();
		const active = ide.active();
		const dir = active ? parentOf(active.path) : '';
		dropNotice = null;
		const outcome = await uploadFiles(dir, files, {});
		if (outcome.conflicts.length > 0) {
			dropNotice = t('page.ide.uploadPartial', {
				ok: outcome.uploaded.length,
				failed: outcome.failed.length,
				conflicts: outcome.conflicts.length,
			});
		} else if (outcome.failed.length > 0) {
			dropNotice = t('page.ide.uploadPartial', {
				ok: outcome.uploaded.length,
				failed: outcome.failed.length,
				conflicts: 0,
			});
		}
	}

	$effect(() => {
		const node = ideRoot;
		if (!node) return;
		return registerIdeShortcuts(node, () => ({
			saveFile: () => {
				const current = ide.active();
				if (current && !current.isBinary && !current.saving) {
					void ide.saveFile(current.path).then((ok) => {
						if (ok) {
							void ide.refreshGitStatus();
							ide.bumpGitRefresh();
						}
					});
				}
			},
			saveAll: () => {
				void ide.saveAll().then(() => {
					void ide.refreshGitStatus();
					ide.bumpGitRefresh();
				});
			},
			closeTab: () => {
				const current = ide.active();
				if (current) requestClose(current.path);
			},
			toggleSidebar: () => ide.setSidebarOpen(!ide.sidebarOpen),
			toggleTerminal: () => {
				ide.setTerminalOpen(!ide.terminalOpen);
			},
			quickOpen: () => focusSearch('files'),
			contentSearch: () => focusSearch('content'),
			revealInTree: () => {
				const current = ide.active();
				if (current) ide.requestReveal(current.path);
			},
			copyPath: () => {
				const current = ide.active();
				if (!current) return;
				try {
					void navigator.clipboard.writeText(current.path);
				} catch {
					// Clipboard is best-effort.
				}
			},
			cut: () => {
				if (editingFocused()) return;
				const targets = clipboardTargets();
				if (targets.length === 0) return;
				ide.setClipboard({ mode: 'cut', paths: targets });
			},
			copy: () => {
				if (editingFocused()) return;
				const targets = clipboardTargets();
				if (targets.length === 0) return;
				ide.setClipboard({ mode: 'copy', paths: targets });
			},
			paste: () => {
				if (editingFocused()) return;
				if (!ide.clipboard) return;
				void pasteClipboard(pasteDestination()).then(({ ok, failed }) => {
					if (failed.length > 0) {
						toasts.error(
							t('page.ide.paste'),
							t('page.ide.pastePartial', { ok, failed: failed.length }),
						);
					} else if (ok > 0) {
						toasts.success(t('page.ide.pasteDone', { ok }));
					}
				});
			},
			selectAll: () => {
				if (editingFocused()) return;
				selectAllLoaded();
			},
			clearSelection: () => {
				ide.clearSelection();
				if (ide.clipboard?.mode === 'cut') ide.clearClipboard();
			},
			upload: () => {
				const active = ide.active();
				ide.requestUpload(active ? parentOf(active.path) : '');
			},
			download: () => {
				void downloadTargets(clipboardTargets());
			},
			toggleHistory: () => ide.setHistoryOpen(!ide.historyOpen),
			nextChange: () => stepChange(1),
			prevChange: () => stepChange(-1),
			viewDiff: () => {
				const path = ide.diffTargetPath() ?? ide.active()?.path;
				if (path) ide.openDiff({ path, base: { kind: 'unstaged' } });
			},
		}));
	});

	$effect(() => {
		if (loading || refreshing || noRepo || loadError) return;
		const requested = parseIdeQuery(page.url.searchParams);
		const key = requested
			? `${requested.path}:${requested.line ?? ''}:${requested.col ?? ''}`
			: null;
		if (key === lastQueryKey) return;
		lastQueryKey = key;
		if (requested) void ide.openFile(requested.path, false, requested.line, requested.col);
	});

	$effect(() => {
		const command = ide.consumeCommand();
		if (!command) return;
		const current = ide.active();
		switch (command) {
			case 'quickOpen':
				focusSearch('files');
				break;
			case 'contentSearch':
			case 'focusSearch':
				focusSearch('content');
				break;
			case 'saveFile':
				if (current && !current.isBinary && !current.saving) {
					void ide.saveFile(current.path).then((ok) => {
						if (ok) {
							void ide.refreshGitStatus();
							ide.bumpGitRefresh();
						}
					});
				}
				break;
			case 'saveAll':
				void ide.saveAll().then(() => {
					void ide.refreshGitStatus();
					ide.bumpGitRefresh();
				});
				break;
			case 'reveal':
				if (current) ide.requestReveal(current.path);
				break;
			case 'copyPath':
				if (!current) break;
				try {
					void navigator.clipboard.writeText(current.path);
				} catch {
					// Clipboard is best-effort.
				}
				break;
			case 'cut': {
				const targets = clipboardTargets();
				if (targets.length > 0) ide.setClipboard({ mode: 'cut', paths: targets });
				break;
			}
			case 'copy': {
				const targets = clipboardTargets();
				if (targets.length > 0) ide.setClipboard({ mode: 'copy', paths: targets });
				break;
			}
			case 'paste':
				if (ide.clipboard) void pasteClipboard(pasteDestination());
				break;
			case 'selectAll':
				selectAllLoaded();
				break;
			case 'clearSelection':
				ide.clearSelection();
				break;
			case 'upload':
				ide.requestUpload(current ? parentOf(current.path) : '');
				break;
			case 'download':
				void downloadTargets(clipboardTargets());
				break;
			case 'toggleHistory':
				ide.setHistoryOpen(!ide.historyOpen);
				break;
			case 'nextChange':
				stepChange(1);
				break;
			case 'prevChange':
				stepChange(-1);
				break;
			case 'viewDiff': {
				const path = ide.diffTargetPath() ?? current?.path;
				if (path) ide.openDiff({ path, base: { kind: 'unstaged' } });
				break;
			}
			case 'newFile':
				ide.setSidebarOpen(true);
				ide.requestRootCreate(false);
				break;
			case 'newFolder':
				ide.setSidebarOpen(true);
				ide.requestRootCreate(true);
				break;
		}
	});

	$effect(() => {
		if (typeof window === 'undefined') return;
		const onFocus = (): void => {
			const current = ide.active();
			if (!current || current.loading || current.saving) return;
			if (current.content !== current.original || current.isBinary || current.truncated) return;
			if (current.hash === null) {
				void ide.loadFile(current.path, true);
				return;
			}
			void statFs(current.path)
				.then((stat) => {
					if (stat.hash && stat.hash !== current.hash) void ide.loadFile(current.path, true);
				})
				.catch(() => {
					// External-change probing is best-effort.
				});
		};
		window.addEventListener('focus', onFocus);
		return () => window.removeEventListener('focus', onFocus);
	});

	$effect(() => {
		if (typeof window === 'undefined') return;
		const onKey = (event: KeyboardEvent): void => {
			if (event.key === 'Escape' && ide.clipboard?.mode === 'cut' && !editingFocused()) {
				ide.clearClipboard();
			}
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});

	function requestClose(path: string): void {
		if (!ide.isDirty(path)) {
			ide.forceClose(path);
			return;
		}
		pendingClose = path;
	}

	function confirmClose(save: boolean): void {
		const path = pendingClose;
		pendingClose = null;
		if (!path) return;
		if (save) {
			void ide.saveFile(path).then((ok) => {
				if (ok) {
					ide.forceClose(path);
					void ide.refreshGitStatus();
					ide.bumpGitRefresh();
				}
			});
		} else {
			ide.forceClose(path);
		}
	}
</script>

{#snippet sidebar()}
	<div class="flex items-center gap-1 border-b border-border px-2">
		<Segmented
			items={[
				{ id: 'tree', label: t('page.ide.sidebarFiles') },
				{ id: 'search', label: t('page.ide.sidebarSearch') },
			]}
			bind:value={ide.sidebarView}
			size="sm"
			class="flex-1 border-b-0"
		/>
		<IconButton
			icon="refresh"
			label={t('common.refresh')}
			compact
			variant="ghost"
			onclick={() => refreshTree()}
		/>
	</div>
	<div class="min-h-0 flex-1 {ide.sidebarView === 'tree' ? '' : 'hidden'}">
		<FileTree onViewDiff={(path) => ide.openDiff({ path, base: { kind: 'unstaged' } })} />
	</div>
	<div class="min-h-0 flex-1 {ide.sidebarView === 'search' ? '' : 'hidden'}">
		<IdeSearchPanel />
	</div>
{/snippet}

<div class="flex h-full min-h-0 flex-col" bind:this={ideRoot}>
	<PageHeader title={t('page.ide.title')} description={t('page.ide.description')}>
		{#snippet actions()}
			{#if ide.root}
				<Badge variant="neutral">{ide.root}</Badge>
			{/if}
			{#if refreshing}
				<span class="flex items-center gap-1 text-muted-foreground" role="status">
					<Icon name="loader" size={12} class="animate-spin" />
				</span>
			{/if}
			{#if ide.dirtyPaths.length > 0}
				<Badge variant="warning"
					>{t('page.ide.unsavedCount', { count: ide.dirtyPaths.length })}</Badge
				>
			{/if}
			<IconButton icon="refresh" label={t('common.refresh')} onclick={() => void reload()} />
			<Button
				size="sm"
				variant={ide.sidebarOpen ? 'secondary' : 'outline'}
				active={ide.sidebarOpen}
				onclick={() => ide.setSidebarOpen(!ide.sidebarOpen)}
				aria-pressed={ide.sidebarOpen}
			>
				{t('page.ide.sidebarFiles')}
			</Button>
			<Button
				size="sm"
				variant={ide.historyOpen ? 'secondary' : 'outline'}
				active={ide.historyOpen}
				onclick={() => ide.setHistoryOpen(!ide.historyOpen)}
				aria-pressed={ide.historyOpen}
			>
				{t('page.ide.historyTitle')}
			</Button>
			<Button
				size="sm"
				variant={ide.terminalOpen ? 'secondary' : 'outline'}
				active={ide.terminalOpen}
				onclick={() => {
					ide.setTerminalOpen(!ide.terminalOpen);
				}}
				aria-pressed={ide.terminalOpen}
			>
				{t('page.ide.showTerminal')}
			</Button>
			<Button
				size="sm"
				variant="outline"
				onclick={() => void ide.saveAll()}
				disabled={ide.dirtyPaths.length === 0}
			>
				{t('page.ide.saveAll')}
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1">
		<PageState
			{loading}
			error={loadError}
			empty={noRepo}
			emptyTitle={t('page.ide.noRepoTitle')}
			emptyDescription={t('page.ide.noRepoDesc')}
			onretry={() => void reload()}
		>
			<div class="relative flex h-full min-h-0 flex-col bg-card">
				<div class="flex min-h-0 flex-1">
					{#if ide.sidebarOpen}
						<aside
							class="flex shrink-0 flex-col border-r border-border bg-muted/20 max-md:hidden"
							style="width: {ide.sidebarWidth}px"
						>
							{@render sidebar()}
						</aside>
						<ResizeHandle
							label={t('layout.sidebar.resize')}
							axis="x"
							class="w-2 shrink-0 text-muted-foreground hover:text-foreground max-md:hidden"
							pillClass="w-1 h-16 bg-border"
							onResize={(delta) => ide.setSidebarWidth(ide.sidebarWidth + delta)}
							onReset={() => ide.resetSidebarWidth()}
						/>
					{/if}
					<section
						class="flex min-w-0 flex-1 flex-col"
						role="presentation"
						ondragover={onEditorDragOver}
						ondrop={(e) => void onEditorDrop(e)}
					>
						{#if dropNotice}
							<div
								class="flex items-center gap-2 border-b border-warning/30 bg-warning/10 px-3 py-1 text-micro"
							>
								<span class="min-w-0 flex-1">{dropNotice}</span>
								<button
									type="button"
									class="cursor-pointer underline"
									onclick={() => (dropNotice = null)}
								>
									{t('common.close')}
								</button>
							</div>
						{/if}
						<FileEditor
							onRequestClose={requestClose}
							{gutterMarks}
							{diffOverflow}
							onOpenFullDiff={() => {
								const path = ide.diffTargetPath() ?? ide.active()?.path;
								if (path) ide.openDiff({ path, base: { kind: 'unstaged' } });
							}}
							onOpenInGit={() => void goto(resolve('/git'))}
						/>
					</section>
					{#if ide.historyOpen}
						<ResizeHandle
							label={t('layout.sidebar.resize')}
							axis="x"
							invert
							class="w-2 shrink-0 text-muted-foreground hover:text-foreground max-md:hidden"
							pillClass="w-1 h-16 bg-border"
							onResize={(delta) => ide.setHistoryWidth(ide.historyWidth + delta)}
							onReset={() => ide.resetHistoryWidth()}
						/>
						<aside
							class="flex shrink-0 flex-col border-l border-border bg-muted/20 max-md:hidden"
							style="width: {ide.historyWidth}px"
						>
							<IdeHistoryPanel
								onViewFileDiff={(target) => openDiffTab(target)}
								onOpenInGit={() => void goto(resolve('/git'))}
							/>
						</aside>
					{/if}
				</div>
				{#if ide.terminalOpen}
					<div class="shrink-0 border-t border-border">
						<IdeTerminalPanel repo={ide.root} />
					</div>
				{/if}
			</div>
		</PageState>
	</div>
</div>

{#if ide.sidebarOpen}
	<div class="fixed inset-0 z-30 md:hidden">
		<button
			type="button"
			aria-label={t('common.close')}
			class="absolute inset-0 bg-black/40"
			onclick={() => ide.setSidebarOpen(false)}
		></button>
		<div
			class="absolute top-0 bottom-0 left-0 flex w-72 max-w-[85vw] flex-col overflow-hidden bg-card shadow-lg"
		>
			{@render sidebar()}
		</div>
	</div>
{/if}

{#if ide.historyOpen}
	<div class="fixed inset-0 z-30 md:hidden">
		<button
			type="button"
			aria-label={t('common.close')}
			class="absolute inset-0 bg-black/40"
			onclick={() => ide.setHistoryOpen(false)}
		></button>
		<div
			class="absolute top-0 right-0 bottom-0 flex w-72 max-w-[85vw] flex-col overflow-hidden bg-card shadow-lg"
		>
			<IdeHistoryPanel
				onViewFileDiff={(target) => {
					ide.setHistoryOpen(false);
					openDiffTab(target);
				}}
				onOpenInGit={() => void goto(resolve('/git'))}
			/>
		</div>
	</div>
{/if}

<Dialog
	open={pendingClose !== null}
	title={t('page.ide.unsavedTitle')}
	description={t('page.ide.unsavedDesc')}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={() => (pendingClose = null)}>
			{t('common.cancel')}
		</Button>
		<Button size="sm" variant="outline" onclick={() => confirmClose(false)}>
			{t('page.ide.discard')}
		</Button>
		<Button size="sm" variant="default" onclick={() => confirmClose(true)}>
			{t('page.ide.saveAndClose')}
		</Button>
	</div>
</Dialog>
