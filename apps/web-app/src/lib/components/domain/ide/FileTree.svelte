<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { createFsEntry, deleteFsEntry, renameFsEntry } from '#lib/services/fs';
	import { t } from '#lib/i18n';

	let filter = $state('');
	let expanded = new SvelteSet<string>(['']);
	let creating = $state<{ parent: string; isDir: boolean } | null>(null);
	let newName = $state('');
	let renaming = $state<string | null>(null);
	let renameValue = $state('');

	onMount(() => {
		void ide.loadDir('');
	});

	function toggle(path: string): void {
		if (expanded.has(path)) expanded.delete(path);
		else {
			expanded.add(path);
			void ide.loadDir(path);
		}
	}

	function visibleEntries(path: string) {
		const state = ide.dirState(path);
		if (!state.entries) return [];
		if (!filter.trim()) return state.entries;
		const needle = filter.trim().toLowerCase();
		return state.entries.filter((e) => e.path.toLowerCase().includes(needle));
	}

	async function openEntry(path: string, isDir: boolean, preview = true): Promise<void> {
		if (isDir) {
			toggle(path);
			return;
		}
		await ide.openFile(path, preview);
	}

	function startCreate(parent: string, isDir: boolean): void {
		creating = { parent, isDir };
		newName = '';
	}

	async function commitCreate(): Promise<void> {
		if (!creating || !newName.trim()) {
			creating = null;
			return;
		}
		const target = creating.parent ? `${creating.parent}/${newName.trim()}` : newName.trim();
		try {
			await createFsEntry(target, creating.isDir);
			ide.invalidateParents(target);
			await ide.loadDir(creating.parent, true);
			if (!creating.isDir) await ide.openFile(target, false);
		} catch {
			// Creation errors surface on the next reload; keep the tree stable.
		} finally {
			creating = null;
		}
	}

	function startRename(path: string): void {
		renaming = path;
		renameValue = path.split('/').pop() ?? path;
	}

	async function commitRename(path: string): Promise<void> {
		if (renaming !== path) return;
		renaming = null;
		const base = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
		const target = base ? `${base}/${renameValue.trim()}` : renameValue.trim();
		if (!renameValue.trim() || target === path) return;
		try {
			await renameFsEntry(path, target);
			ide.invalidateParents(path);
			ide.invalidateParents(target);
			if (ide.dirState(base || '').loaded) await ide.loadDir(base || '', true);
			const tab = ide.tabs.find((entry) => entry.path === path);
			if (tab) {
				tab.path = target;
				if (ide.activePath === path) ide.activePath = target;
			}
		} catch {
			// Keep the old name on failure.
		}
	}

	async function removeEntry(path: string): Promise<void> {
		if (!confirm(t('page.ide.confirmDelete', { path }))) return;
		try {
			await deleteFsEntry(path);
			ide.invalidateParents(path);
			const base = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
			await ide.loadDir(base || '', true);
			ide.forceClose(path);
		} catch {
			// Deletion errors keep the entry visible.
		}
	}

	function entryIcon(isDir: boolean, open: boolean): 'folder' | 'file' | 'code' {
		if (isDir) return 'folder';
		return open ? 'code' : 'file';
	}
</script>

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
			onclick={() => void ide.loadDir('', true)}
		/>
	</div>

	<div
		class="min-h-0 flex-1 overflow-y-auto px-1 py-1"
		role="tree"
		aria-label={t('page.ide.fileTree')}
	>
		{#if creating && creating.parent === ''}
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
		{#await Promise.resolve(visibleEntries('')) then entries}
			{#each entries as entry (entry.path)}
				{@const isOpen = expanded.has(entry.path)}
				{@const dirty = entry.is_dir ? false : ide.isDirty(entry.path)}
				<div
					role="treeitem"
					aria-selected={ide.activePath === entry.path}
					aria-expanded={entry.is_dir ? isOpen : undefined}
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
								<Icon
									name={entryIcon(true, isOpen)}
									size={14}
									class="shrink-0 text-muted-foreground"
								/>
								<span class="min-w-0 flex-1 truncate">{entry.name || '/'}</span>
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
									name={entryIcon(false, false)}
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
							{:else}
								<IconButton
									icon="pencil"
									label={t('page.ide.rename')}
									compact
									variant="ghost"
									onclick={() => startRename(entry.path)}
								/>
							{/if}
							<IconButton
								icon="trash"
								label={t('page.ide.delete')}
								compact
								variant="ghost"
								onclick={() => void removeEntry(entry.path)}
							/>
						</span>
					</div>

					{#if renaming === entry.path && !entry.is_dir}
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
								{#each visibleEntries(entry.path) as child (child.path)}
									{@const childDirty = child.is_dir ? false : ide.isDirty(child.path)}
									<div
										class="flex w-full items-center gap-1 rounded-md px-1 py-1 text-body hover:bg-accent/60 {ide.activePath ===
										child.path
											? 'bg-accent text-accent-foreground'
											: ''}"
									>
										{#if child.is_dir}
											<button
												type="button"
												class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
												onclick={() => toggle(child.path)}
											>
												<Icon
													name={expanded.has(child.path) ? 'chevron-down' : 'chevron-right'}
													size={13}
													class="shrink-0 text-muted-foreground"
												/>
												<Icon name="folder" size={14} class="shrink-0 text-muted-foreground" />
												<span class="min-w-0 flex-1 truncate">{child.name}</span>
											</button>
										{:else}
											<button
												type="button"
												class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
												onclick={() => void openEntry(child.path, false, true)}
												ondblclick={() => void openEntry(child.path, false, false)}
											>
												<span class="w-4 shrink-0"></span>
												<Icon name="file" size={14} class="shrink-0 text-muted-foreground" />
												<span class="min-w-0 flex-1 truncate">{child.name}</span>
												{#if childDirty}
													<span
														class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
														title={t('page.ide.unsaved')}
													></span>
												{/if}
											</button>
										{/if}
										<span class="hidden shrink-0 items-center group-hover:flex">
											<IconButton
												icon="trash"
												label={t('page.ide.delete')}
												compact
												variant="ghost"
												onclick={() => void removeEntry(child.path)}
											/>
										</span>
									</div>
									{#if child.is_dir && expanded.has(child.path)}
										<div class="ml-3 border-l border-border pl-1">
											<p class="px-2 py-1 text-micro text-muted-foreground">
												{t('page.ide.expandHint')}
											</p>
										</div>
									{/if}
								{/each}
							{/if}
							{#if creating && creating.parent === entry.path}
								<div class="flex items-center gap-1 px-1 py-0.5">
									<input
										class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-body"
										bind:value={newName}
										placeholder={t('page.ide.fileName')}
										onkeydown={(e) => {
											if (e.key === 'Enter') void commitCreate();
											else if (e.key === 'Escape') creating = null;
										}}
									/>
								</div>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		{/await}
	</div>
</div>
