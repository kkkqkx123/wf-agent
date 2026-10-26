<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import {
		createFsEntry,
		deleteFsEntry,
		renameFsEntry,
		languageForPath,
	} from '#lib/services/fs';
	import { t } from '#lib/i18n';
	import { invalidateFsTreeIndex } from '#lib/utils/fs-tree-index';
	import { IDE_TREE_ROW_HEIGHT_PX } from '#lib/utils/file-tree-window';
	import type { GitBadgeLevel } from '#lib/utils/git-badges';

	let filter = $state('');
	let expanded = new SvelteSet<string>(['']);
	let creating = $state<{ parent: string; isDir: boolean } | null>(null);
	let newName = $state('');
	let renaming = $state<string | null>(null);
	let renameValue = $state('');
	let actionError = $state<string | null>(null);
	let deleteOpen = $state(false);
	let pendingDelete = $state<string | null>(null);

	const MAX_DIR_ENTRIES = 200;
	// Fixed row height shared with the future windowed list; exposed as a CSS
	// variable so the replacement render container keeps identical metrics.
	const treeRowHeightStyle = `--ide-tree-row-height: ${IDE_TREE_ROW_HEIGHT_PX}px`;

	const BADGE_DOT: Record<GitBadgeLevel, string> = {
		added: 'bg-emerald-500',
		modified: 'bg-amber-500',
		deleted: 'bg-red-500',
		renamed: 'bg-sky-500',
		conflicted: 'bg-red-600 ring-2 ring-red-600/40',
		untracked: 'bg-zinc-400',
	};

	function badgeLabel(level: GitBadgeLevel): string {
		switch (level) {
			case 'added': return t('page.ide.badgeAdded');
			case 'modified': return t('page.ide.badgeModified');
			case 'deleted': return t('page.ide.badgeDeleted');
			case 'renamed': return t('page.ide.badgeRenamed');
			case 'conflicted': return t('page.ide.badgeConflicted');
			case 'untracked': return t('page.ide.badgeUntracked');
		}
	}

	onMount(() => {
		void ide.loadDir('');
	});

	$effect(() => {
		const target = ide.revealRequest;
		if (!target) return;
		const parts = target.split('/').filter(Boolean);
		let prefix = '';
		for (let i = 0; i < parts.length - 1; i += 1) {
			prefix = prefix ? `${prefix}/${parts[i]}` : parts[i];
			if (!expanded.has(prefix)) {
				expanded.add(prefix);
				void ide.loadDir(prefix);
			}
		}
		ide.consumeReveal();
	});

	function parentOf(path: string): string {
		return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
	}

	function toggle(path: string): void {
		if (expanded.has(path)) expanded.delete(path);
		else {
			expanded.add(path);
			void ide.loadDir(path);
		}
	}

	function visibleEntries(path: string): { shown: typeof entries; overflow: number } {
		const state = ide.dirState(path);
		const entries = state.entries ?? [];
		const needle = filter.trim().toLowerCase();
		const matched = needle
			? entries.filter((e) => e.path.toLowerCase().includes(needle))
			: entries;
		if (matched.length <= MAX_DIR_ENTRIES) return { shown: matched, overflow: 0 };
		return { shown: matched.slice(0, MAX_DIR_ENTRIES), overflow: matched.length - MAX_DIR_ENTRIES };
	}

	function rowBadge(path: string, isDir: boolean): GitBadgeLevel | null {
		if (!ide.gitBadgesAvailable) return null;
		if (!isDir) return ide.gitBadges.get(path) ?? null;
		return ide.dirBadge(path);
	}

	async function openEntry(path: string, isDir: boolean, preview = true): Promise<void> {
		if (isDir) {
			toggle(path);
			return;
		}
		actionError = null;
		try {
			await ide.openFile(path, preview);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function startCreate(parent: string, isDir: boolean): void {
		creating = { parent, isDir };
		newName = '';
		actionError = null;
	}

	async function commitCreate(): Promise<void> {
		const draft = creating;
		if (!draft || !newName.trim()) {
			creating = null;
			return;
		}
		const target = draft.parent ? `${draft.parent}/${newName.trim()}` : newName.trim();
		creating = null;
		try {
			await createFsEntry(target, draft.isDir);
			ide.invalidateParents(target);
			invalidateFsTreeIndex();
			await ide.loadDir(draft.parent, true);
			void ide.refreshGitStatus();
			if (!draft.isDir) await ide.openFile(target, false);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function startRename(path: string): void {
		renaming = path;
		renameValue = path.split('/').pop() ?? path;
		actionError = null;
	}

	function retargetTabs(from: string, to: string): void {
		const prefix = `${from}/`;
		const previousActive = ide.activePath;
		for (const tab of ide.tabs) {
			if (tab.path === from) {
				tab.path = to;
				tab.language = languageForPath(to);
			} else if (tab.path.startsWith(prefix)) {
				tab.path = `${to}${tab.path.slice(from.length)}`;
				tab.language = languageForPath(tab.path);
			}
		}
		if (previousActive === from) ide.activePath = to;
		else if (previousActive?.startsWith(prefix)) {
			ide.activePath = `${to}${previousActive.slice(from.length)}`;
		}
	}

	async function commitRename(path: string): Promise<void> {
		if (renaming !== path) return;
		renaming = null;
		const base = parentOf(path);
		if (!renameValue.trim()) return;
		const target = base ? `${base}/${renameValue.trim()}` : renameValue.trim();
		if (target === path) return;
		try {
			await renameFsEntry(path, target);
			ide.invalidateParents(path);
			ide.invalidateParents(target);
			invalidateFsTreeIndex();
			invalidateFsTreeIndex();
			if (ide.dirState(base).loaded) await ide.loadDir(base, true);
			void ide.refreshGitStatus();
			retargetTabs(path, target);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function requestRemove(path: string): void {
		pendingDelete = path;
		deleteOpen = true;
		actionError = null;
	}

	function cancelRemove(): void {
		pendingDelete = null;
		deleteOpen = false;
	}

	async function confirmRemove(): Promise<void> {
		const target = pendingDelete;
		pendingDelete = null;
		deleteOpen = false;
		if (!target) return;
		try {
			await deleteFsEntry(target);
			ide.invalidateParents(target);
			invalidateFsTreeIndex();
			await ide.loadDir(parentOf(target), true);
			void ide.refreshGitStatus();
			const prefix = `${target}/`;
			const affected = ide.tabs.filter(
				(entry) => entry.path === target || entry.path.startsWith(prefix),
			);
			for (const tab of affected) ide.forceClose(tab.path);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function entryIcon(isDir: boolean): 'folder' | 'file' {
		return isDir ? 'folder' : 'file';
	}
</script>

{#snippet node(dirPath: string, level: number)}
	{#if creating && creating.parent === dirPath}
		<div class="flex items-center gap-1 px-1 py-0.5">
			<Icon name={creating.isDir ? 'folder' : 'file'} size={14} class="text-muted-foreground" />
			<input
				class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-body"
				bind:value={newName}
				placeholder={creating.isDir ? t('page.ide.folderName') : t('page.ide.fileName')}
				onkeydown={(e) => {
					if (e.key === 'Enter') void commitCreate();
					else if (e.key === 'Escape') creating = null;
				}}
			/>
		</div>
	{/if}
	{@const listed = visibleEntries(dirPath)}
	{#each listed.shown as entry (entry.path)}
		{@const isOpen = expanded.has(entry.path)}
		{@const dirty = entry.is_dir ? false : ide.isDirty(entry.path)}
		{@const badge = rowBadge(entry.path, entry.is_dir)}
		<div
			role="treeitem"
			aria-selected={ide.activePath === entry.path}
			aria-expanded={entry.is_dir ? isOpen : undefined}
			aria-level={level + 1}
		>
			<div
				class="group flex w-full items-center gap-1 rounded-md px-1 py-1 text-body hover:bg-accent/60 {ide.activePath ===
				entry.path
					? 'bg-accent text-accent-foreground'
					: ''}"
			>
				{#if entry.is_dir}
					<button
						type="button"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
						onclick={() => toggle(entry.path)}
						ondblclick={() => toggle(entry.path)}
					>
						<Icon
							name={isOpen ? 'chevron-down' : 'chevron-right'}
							size={13}
							class="shrink-0 text-muted-foreground"
						/>
						<Icon name="folder" size={14} class="shrink-0 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{entry.name || '/'}</span>
						{#if badge}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full {BADGE_DOT[badge]}"
								title={badgeLabel(badge)}
							></span>
						{/if}
					</button>
				{:else}
					<button
						type="button"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
						onclick={() => void openEntry(entry.path, false, true)}
						ondblclick={() => void openEntry(entry.path, false, false)}
					>
						<span class="w-4 shrink-0"></span>
						<Icon
							name={entryIcon(false)}
							size={14}
							class="shrink-0 text-muted-foreground"
						/>
						<span class="min-w-0 flex-1 truncate">{entry.name}</span>
						{#if dirty}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
								title={t('page.ide.unsaved')}
							></span>
						{/if}
						{#if badge}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full {BADGE_DOT[badge]}"
								title={badgeLabel(badge)}
							></span>
						{/if}
					</button>
				{/if}
				<span class="hidden shrink-0 items-center group-hover:flex">
					{#if entry.is_dir}
						<IconButton
							icon="plus"
							label={t('page.ide.newFile')}
							compact
							variant="ghost"
							onclick={() => {
								if (!isOpen) toggle(entry.path);
								startCreate(entry.path, false);
							}}
						/>
					{/if}
					<IconButton
						icon="pencil"
						label={t('page.ide.rename')}
						compact
						variant="ghost"
						onclick={() => startRename(entry.path)}
					/>
					<IconButton
						icon="trash"
						label={t('page.ide.delete')}
						compact
						variant="ghost"
						onclick={() => requestRemove(entry.path)}
					/>
				</span>
			</div>

			{#if renaming === entry.path}
				<div class="flex items-center gap-1 py-0.5 pl-8 pr-1">
					<input
						class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-body"
						bind:value={renameValue}
						onkeydown={(e) => {
							if (e.key === 'Enter') void commitRename(entry.path);
							else if (e.key === 'Escape') renaming = null;
						}}
					/>
				</div>
			{/if}

			{#if entry.is_dir && isOpen}
				{@const childState = ide.dirState(entry.path)}
				{#if !childState.loaded && !childState.loading}
					{void ide.loadDir(entry.path)}
				{/if}
				<div class="ml-3 border-l border-border pl-1">
					{#if childState.loading && !childState.entries}
						<p class="px-2 py-1 text-micro text-muted-foreground">{t('common.loading')}</p>
					{:else if childState.error}
						<p class="px-2 py-1 text-micro text-destructive">{childState.error}</p>
					{:else}
						{@render node(entry.path, level + 1)}
					{/if}
				</div>
			{/if}
		</div>
	{/each}
	{#if listed.overflow > 0}
		<p class="px-2 py-1 text-micro text-muted-foreground">
			{t('page.ide.dirOverflow', { count: listed.overflow })}
		</p>
	{/if}
{/snippet}

<div class="flex h-full min-h-0 flex-col">
	<div class="flex items-center gap-1 border-b border-border px-2 py-1.5">
		<div class="min-w-0 flex-1">
			<Input
				bind:value={filter}
				placeholder={t('page.ide.filterFiles')}
				aria-label={t('page.ide.filterFiles')}
			/>
		</div>
		<IconButton
			icon="plus"
			label={t('page.ide.newFile')}
			compact
			variant="ghost"
			onclick={() => startCreate('', false)}
		/>
		<IconButton
			icon="folder"
			label={t('page.ide.newFolder')}
			compact
			variant="ghost"
			onclick={() => startCreate('', true)}
		/>
		<IconButton
			icon="refresh"
			label={t('common.refresh')}
			compact
			variant="ghost"
			onclick={() => {
				void ide.loadDir('', true);
				void ide.refreshGitStatus();
			}}
		/>
	</div>

	{#if actionError}
		<p
			class="border-b border-destructive/30 bg-destructive/10 px-3 py-1 text-micro text-destructive"
		>
			{actionError}
		</p>
	{/if}

	<div
		class="min-h-0 flex-1 overflow-y-auto px-1 py-1"
		role="tree"
		aria-label={t('page.ide.fileTree')}
		style={treeRowHeightStyle}
	>
		{@render node('', 0)}
	</div>
</div>

<Dialog
	bind:open={deleteOpen}
	title={t('page.ide.delete')}
	description={pendingDelete ? t('page.ide.confirmDelete', { path: pendingDelete }) : undefined}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={cancelRemove}>
			{t('common.cancel')}
		</Button>
		<Button size="sm" variant="default" onclick={() => void confirmRemove()}>
			{t('common.delete')}
		</Button>
	</div>
</Dialog>
