<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import FileTree from '#lib/components/domain/ide/FileTree.svelte';
	import FileEditor from '#lib/components/domain/ide/FileEditor.svelte';
	import IdeTerminalPanel from '#lib/components/domain/ide/IdeTerminalPanel.svelte';
	import IdeInspectorPanel from '#lib/components/domain/ide/IdeInspectorPanel.svelte';
	import IdeQuickOpen from '#lib/components/domain/ide/IdeQuickOpen.svelte';
	import IdeContentSearch from '#lib/components/domain/ide/IdeContentSearch.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { getMeta } from '#lib/services/git';
	import { parseIdeQuery } from '#lib/utils/ide-url';
	import { registerIdeShortcuts } from '#lib/ide/ide-shortcuts';
	import { t } from '#lib/i18n';

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let noRepo = $state(false);
	let pendingClose = $state<string | null>(null);
	let quickOpen = $state(false);
	let contentSearch = $state(false);
	let ideRoot: HTMLElement | null = $state(null);
	let lastQueryKey = $state<string | null>(null);

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

	$effect(() => {
		const node = ideRoot;
		if (!node) return;
		return registerIdeShortcuts(node, () => ({
			saveFile: () => {
				const current = ide.active();
				if (current && !current.isBinary && !current.saving) void ide.saveFile(current.path);
			},
			saveAll: () => void ide.saveAll(),
			closeTab: () => {
				const current = ide.active();
				if (current) requestClose(current.path);
			},
			toggleSidebar: () => ide.setSidebarOpen(!ide.sidebarOpen),
			toggleTerminal: () => {
				ide.terminalOpen = !ide.terminalOpen;
			},
			toggleInspector: () => ide.setInspectorOpen(!ide.inspectorOpen),
			quickOpen: () => {
				quickOpen = true;
			},
			contentSearch: () => {
				contentSearch = true;
			},
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
		if (typeof window === 'undefined') return;
		const onFocus = (): void => {
			const current = ide.active();
			if (!current || current.loading || current.saving) return;
			if (current.content === current.original && !current.isBinary && !current.truncated) {
				void ide.loadFile(current.path, true);
			}
		};
		window.addEventListener('focus', onFocus);
		return () => window.removeEventListener('focus', onFocus);
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
				if (ok) ide.forceClose(path);
			});
		} else {
			ide.forceClose(path);
		}
	}
</script>

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
				onclick={() => {
					ide.terminalOpen = !ide.terminalOpen;
				}}
			>
				{ide.terminalOpen ? t('page.ide.hideTerminal') : t('page.ide.showTerminal')}
			</Button>
			<Button
				size="sm"
				variant="outline"
				onclick={() => ide.setInspectorOpen(!ide.inspectorOpen)}
			>
				{ide.inspectorOpen ? t('page.ide.hideInspector') : t('page.ide.showInspector')}
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
							<FileTree onQuickOpen={() => (quickOpen = true)} />
						</aside>
					{/if}
					<section class="flex min-w-0 flex-1 flex-col">
						<FileEditor onRequestClose={requestClose} />
					</section>
					{#if ide.inspectorOpen}
						<aside
							class="absolute top-0 right-0 bottom-0 z-20 flex w-72 max-w-[85vw] shrink-0 flex-col border-l border-border bg-card shadow-lg"
						>
							<IdeInspectorPanel onclosetab={requestClose} />
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

<IdeQuickOpen open={quickOpen} onclose={() => (quickOpen = false)} />
<IdeContentSearch open={contentSearch} onclose={() => (contentSearch = false)} />

{#if ide.sidebarOpen}
	<div class="fixed inset-0 z-30 md:hidden">
		<button
			type="button"
			aria-label={t('common.close')}
			class="absolute inset-0 bg-black/40"
			onclick={() => ide.setSidebarOpen(false)}
		></button>
		<div class="absolute top-0 bottom-0 left-0 w-72 max-w-[85vw] overflow-hidden bg-card shadow-lg">
			<FileTree onQuickOpen={() => (quickOpen = true)} />
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
