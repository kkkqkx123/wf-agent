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
	import { ide } from '#lib/stores/ide.svelte';
	import { getMeta } from '#lib/services/git';
	import { parseIdeQuery } from '#lib/utils/ide-url';
	import { t } from '#lib/i18n';

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let noRepo = $state(false);
	let pendingClose = $state<string | null>(null);
	let leftOpen = $state(true);

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
			if (requested) await ide.openFile(requested.path, false, requested.line, requested.col);
			else if (ide.activePath) await ide.loadFile(ide.activePath);
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

	function handleKeys(event: KeyboardEvent): void {
		const mod = event.ctrlKey || event.metaKey;
		if (mod && event.key.toLowerCase() === 's') {
			event.preventDefault();
			const current = ide.active();
			if (current && !current.isBinary && !current.saving) void ide.saveFile(current.path);
			return;
		}
		if (mod && event.key.toLowerCase() === 'b') {
			event.preventDefault();
			leftOpen = !leftOpen;
			return;
		}
		if (mod && event.key.toLowerCase() === 'i') {
			event.preventDefault();
			ide.setInspectorOpen(!ide.inspectorOpen);
		}
	}
</script>

<svelte:window onkeydown={(e) => handleKeys(e)} />

<div class="flex h-full min-h-0 flex-col">
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

	<div class="min-h-0 flex-1 px-4 pb-3">
		<PageState
			{loading}
			error={loadError}
			empty={noRepo}
			emptyTitle={t('page.ide.noRepoTitle')}
			emptyDescription={t('page.ide.noRepoDesc')}
			onretry={() => void reload()}
		>
			<div
				class="flex h-full min-h-[60vh] flex-col overflow-hidden rounded-lg border border-border bg-card"
			>
				<div class="flex min-h-0 flex-1">
					{#if leftOpen}
						<aside
							class="flex w-64 shrink-0 flex-col border-r border-border bg-muted/20 max-md:hidden"
						>
							<FileTree />
						</aside>
					{/if}
					<section class="flex min-w-0 flex-1 flex-col">
						<FileEditor onRequestClose={requestClose} />
					</section>
					{#if ide.inspectorOpen}
						<aside
							class="flex w-72 shrink-0 flex-col border-l border-border bg-muted/20 max-lg:hidden"
						>
							<IdeInspectorPanel onclosetab={requestClose} />
						</aside>
					{/if}
				</div>
				{#if ide.terminalOpen}
					<div class="shrink-0">
						<IdeTerminalPanel repo={ide.root} />
					</div>
				{/if}
			</div>
		</PageState>
	</div>
</div>

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
