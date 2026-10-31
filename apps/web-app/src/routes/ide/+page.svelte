<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import DiffView from '#lib/ui/components/DiffView.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
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
	import type { DiffFile } from '#lib/types/diff';
	import { parseIdeQuery } from '#lib/utils/ide-url';
	import { registerIdeShortcuts } from '#lib/ide/ide-shortcuts';
	import { t } from '#lib/i18n';

	let loading = $state(true);
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

	let diffDialogPath = $state<string | null>(null);
	let diffDialogFiles = $state<DiffFile[]>([]);
	let diffDialogLoading = $state(false);

	async function reload(): Promise<void> {
		loading = true;
		loadError = null;
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
			if (
				message.includes('no payload') ||
				message.includes('user-git') ||
				message.includes('workspace')
			) {
				noRepo = true;
			} else {
				loadError = message;
			}
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void reload();
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
		const next = (((changeIndex < 0 ? 0 : changeIndex) + direction) % changeLines.length + changeLines.length) % changeLines.length;
		changeIndex = next;
		const line = changeLines[next];
		if (line !== undefined) void ide.openFile(path, false, line, 1);
	}

	async function openDiffDialog(path: string): Promise<void> {
		diffDialogPath = path;
		diffDialogFiles = [];
		diffDialogLoading = true;
		try {
			diffDialogFiles = await getDiff(path);
		} catch (e) {
			toasts.error(t('page.ide.diffTitle', { path }), e instanceof Error ? e.message : undefined);
			diffDialogPath = null;
		} finally {
			diffDialogLoading = false;
		}
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
				ide.terminalOpen = !ide.terminalOpen;
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
						toasts.error(t('page.ide.paste'), t('page.ide.pastePartial', { ok, failed: failed.length }));
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
				const current = ide.active();
				if (current) void openDiffDialog(current.path);
			},
		}));
	});

	$effect(() => {
		if (loading || noRepo || loadError) return;
		const requested = parseIdeQuery(page.url.searchParams);
		const key = requested ? `${requested.path}:${requested.line ?? ''}:${requested.col ?? ''}` : null;
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
			case 'viewDiff':
				if (current) void openDiffDialog(current.path);
				break;
			case 'newFile':
				ide.sidebarOpen = true;
				ide.requestRootCreate(false);
				break;
			case 'newFolder':
				ide.sidebarOpen = true;
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
	<div class="flex items-center border-b border-border px-2">
		<Segmented
			items={[
				{ id: 'tree', label: t('page.ide.sidebarFiles') },
				{ id: 'search', label: t('page.ide.sidebarSearch') },
			]}
			bind:value={ide.sidebarView}
			size="sm"
			class="flex-1 border-b-0"
		/>
	</div>
	<div class="min-h-0 flex-1 {ide.sidebarView === 'tree' ? '' : 'hidden'}">
		<FileTree onFocusSearch={() => focusSearch('files')} onViewDiff={(path) => void openDiffDialog(path)} />
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
			{#if ide.dirtyPaths.length > 0}
				<Badge variant="warning"
					>{t('page.ide.unsavedCount', { count: ide.dirtyPaths.length })}</Badge
				>
			{/if}
			<IconButton icon="refresh" label={t('common.refresh')} onclick={() => void reload()} />
			<Button
				size="sm"
				variant="outline"
				onclick={() => ide.setHistoryOpen(!ide.historyOpen)}
				aria-pressed={ide.historyOpen}
			>
				{t('page.ide.historyTitle')}
			</Button>
			<Button
				size="sm"
				variant="outline"
				onclick={() => {
					ide.terminalOpen = !ide.terminalOpen;
				}}
			>
				{ide.terminalOpen ? t('page.ide.hideTerminal') : t('page.ide.showTerminal')}
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
							class="flex w-64 shrink-0 flex-col border-r border-border bg-muted/20 max-md:hidden"
						>
							{@render sidebar()}
						</aside>
					{/if}
					<section
						class="flex min-w-0 flex-1 flex-col"
						role="presentation"
						ondragover={onEditorDragOver}
						ondrop={(e) => void onEditorDrop(e)}
					>
						{#if dropNotice}
							<div class="flex items-center gap-2 border-b border-warning/30 bg-warning/10 px-3 py-1 text-micro">
								<span class="min-w-0 flex-1">{dropNotice}</span>
								<button type="button" class="cursor-pointer underline" onclick={() => (dropNotice = null)}>
									{t('common.close')}
								</button>
							</div>
						{/if}
						<FileEditor
							onRequestClose={requestClose}
							{gutterMarks}
							{diffOverflow}
							onOpenFullDiff={() => {
								const current = ide.active();
								if (current) void openDiffDialog(current.path);
							}}
						/>
					</section>
					{#if ide.historyOpen}
						<aside
							class="flex w-72 shrink-0 flex-col border-l border-border bg-muted/20 max-md:hidden"
						>
							<IdeHistoryPanel
								onViewFileDiff={(path) => void openDiffDialog(path)}
								onOpenInGit={() => void goto('/git')}
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
		<div class="absolute top-0 bottom-0 left-0 flex w-72 max-w-[85vw] flex-col overflow-hidden bg-card shadow-lg">
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
		<div class="absolute top-0 right-0 bottom-0 flex w-72 max-w-[85vw] flex-col overflow-hidden bg-card shadow-lg">
			<IdeHistoryPanel
				onViewFileDiff={(path) => {
					ide.setHistoryOpen(false);
					void openDiffDialog(path);
				}}
				onOpenInGit={() => void goto('/git')}
			/>
		</div>
	</div>
{/if}

<Dialog
	open={diffDialogPath !== null}
	title={diffDialogPath ? t('page.ide.diffTitle', { path: diffDialogPath }) : t('page.ide.viewDiff')}
	width="48rem"
	onclose={() => {
		diffDialogPath = null;
		diffDialogFiles = [];
	}}
>
	<div class="max-h-[60vh] space-y-2 overflow-y-auto p-3">
		{#if diffDialogLoading}
			<p class="py-4 text-center text-caption text-muted-foreground">{t('common.loading')}</p>
		{:else if diffDialogFiles.length === 0}
			<p class="py-4 text-center text-caption text-muted-foreground">{t('page.ide.diffEmpty')}</p>
		{:else}
			{#each diffDialogFiles as file (file.path)}
				<DiffView lines={file.lines} title={file.path} />
			{/each}
		{/if}
	</div>
	<div class="flex justify-end gap-2 border-t border-border p-3">
		<Button size="sm" variant="outline" onclick={() => void goto('/git')}>
			{t('page.ide.openInGit')}
		</Button>
		<Button size="sm" variant="ghost" onclick={() => (diffDialogPath = null)}>
			{t('common.close')}
		</Button>
	</div>
</Dialog>

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
