<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { ide } from '#lib/stores/ide.svelte';
	import { filterFsTree, getFsTreeIndex } from '#lib/utils/fs-tree-index';
	import { searchFsContent, type FsContentMatch } from '#lib/services/fs';
	import { t } from '#lib/i18n';

	let fileQuery = $state('');
	let fileResults = $state<Array<{ path: string; is_dir: boolean }>>([]);
	let fileTruncated = $state(false);

	let contentQuery = $state('');
	let scope = $state('');
	let include = $state('');
	let caseSensitive = $state(false);
	let wholeWord = $state(false);
	let useRegex = $state(false);
	let matches = $state<FsContentMatch[]>([]);
	let contentTruncated = $state(false);
	let searching = $state(false);
	let contentError = $state<string | null>(null);
	let contentOffset = $state(0);
	const CONTENT_LIMIT = 100;
	let collapsed = new SvelteSet<string>();
	let fileSelected = $state(0);
	let contentSelected = $state(0);

	$effect(() => {
		const needle = fileQuery.trim();
		if (ide.searchTab !== 'files' || needle.length < 2) {
			if (needle.length < 2) {
				fileResults = [];
				fileTruncated = false;
			}
			return;
		}
		let cancelled = false;
		const timer = setTimeout(() => {
			void getFsTreeIndex(ide.root || 'workspace')
				.then((index) => {
					if (cancelled) return;
					fileTruncated = index.truncated;
					fileResults = filterFsTree(index.entries, needle, 30).filter((e) => !e.is_dir);
					fileSelected = 0;
				})
				.catch(() => {
					if (!cancelled) fileResults = [];
				});
		}, 120);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	});

	async function runContentSearch(append = false): Promise<void> {
		const needle = contentQuery.trim();
		if (needle.length < 2) {
			matches = [];
			contentTruncated = false;
			contentError = null;
			contentOffset = 0;
			return;
		}
		searching = true;
		try {
			const result = await searchFsContent(needle, {
				path: scope.trim(),
				include: include.trim() || undefined,
				caseSensitive,
				wholeWord,
				regex: useRegex,
				offset: append ? contentOffset : 0,
				limit: CONTENT_LIMIT,
			});
			matches = append ? [...matches, ...result.matches] : result.matches;
			contentTruncated = result.truncated;
			contentOffset = matches.length;
			contentSelected = 0;
			contentError = null;
		} catch (e) {
			if (!append) matches = [];
			contentError = e instanceof Error ? e.message : String(e);
		} finally {
			searching = false;
		}
	}

	$effect(() => {
		if (ide.searchTab !== 'content') return;
		const needle = contentQuery.trim();
		void scope;
		void include;
		void caseSensitive;
		void wholeWord;
		void useRegex;
		if (needle.length < 2) {
			matches = [];
			contentTruncated = false;
			contentError = null;
			contentOffset = 0;
			return;
		}
		let cancelled = false;
		const timer = setTimeout(() => {
			if (!cancelled) void runContentSearch(false);
		}, 300);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	});

	const grouped = $derived.by(() => {
		const groups: Array<{ path: string; hits: FsContentMatch[] }> = [];
		for (const hit of matches) {
			const last = groups[groups.length - 1];
			if (last && last.path === hit.path) last.hits.push(hit);
			else groups.push({ path: hit.path, hits: [hit] });
		}
		return groups;
	});

	const flatContent = $derived(matches);

	async function openFile(path: string, line?: number | null, col?: number | null): Promise<void> {
		await ide.openFile(path, false, line ?? null, col ?? 1);
	}

	function toggleGroup(path: string): void {
		if (collapsed.has(path)) collapsed.delete(path);
		else collapsed.add(path);
	}

	function onFileKey(event: KeyboardEvent): void {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			fileSelected = fileResults.length ? (fileSelected + 1) % fileResults.length : 0;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			fileSelected = fileResults.length ? (fileSelected - 1 + fileResults.length) % fileResults.length : 0;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const hit = fileResults[fileSelected];
			if (hit) void openFile(hit.path);
		}
	}

	function onContentKey(event: KeyboardEvent): void {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			contentSelected = flatContent.length ? (contentSelected + 1) % flatContent.length : 0;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			contentSelected = flatContent.length ? (contentSelected - 1 + flatContent.length) % flatContent.length : 0;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const hit = flatContent[contentSelected];
			if (hit) void openFile(hit.path, hit.line, hit.column);
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex items-center gap-1 border-b border-border px-2 py-1.5" role="tablist" aria-label={t('page.ide.sidebarSearch')}>
		<button
			type="button"
			role="tab"
			aria-selected={ide.searchTab === 'files'}
			class="flex-1 rounded-md px-2 py-1 text-caption {ide.searchTab === 'files' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/60'}"
			onclick={() => (ide.searchTab = 'files')}
		>
			{t('page.ide.searchFilesTab')}
		</button>
		<button
			type="button"
			role="tab"
			aria-selected={ide.searchTab === 'content'}
			class="flex-1 rounded-md px-2 py-1 text-caption {ide.searchTab === 'content' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/60'}"
			onclick={() => (ide.searchTab = 'content')}
		>
			{t('page.ide.searchContentTab')}
		</button>
	</div>

	{#if ide.searchTab === 'files'}
		<div class="flex min-h-0 flex-1 flex-col" onkeydown={onFileKey} role="presentation">
			<div class="border-b border-border p-2">
				<input
					bind:value={fileQuery}
					placeholder={t('page.ide.quickOpenPlaceholder')}
					aria-label={t('page.ide.quickOpenPlaceholder')}
					class="h-8 w-full rounded-md border border-input bg-card px-2 text-body focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
			</div>
			<div class="min-h-0 flex-1 overflow-y-auto p-1">
				{#if fileResults.length === 0}
					<p class="px-2 py-6 text-center text-caption text-muted-foreground">
						{t('page.ide.quickOpenEmpty')}
					</p>
				{:else}
					{#each fileResults as hit, index (hit.path)}
						{@const name = hit.path.split('/').pop() ?? hit.path}
						<button
							type="button"
							onclick={() => void openFile(hit.path)}
							onmouseenter={() => (fileSelected = index)}
							class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left {index === fileSelected ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/60'}"
						>
							<span class="min-w-0 flex-1 truncate text-body">{name}</span>
							<span class="shrink-0 truncate text-micro text-muted-foreground">{hit.path}</span>
						</button>
					{/each}
				{/if}
				{#if fileTruncated}
					<p class="px-2 py-1 text-micro text-muted-foreground">{t('page.ide.quickOpenTruncated')}</p>
				{/if}
			</div>
		</div>
	{:else}
		<div class="flex min-h-0 flex-1 flex-col" onkeydown={onContentKey} role="presentation">
			<div class="space-y-1.5 border-b border-border p-2">
				<input
					bind:value={contentQuery}
					placeholder={t('page.ide.searchPlaceholder')}
					aria-label={t('page.ide.searchPlaceholder')}
					class="h-8 w-full rounded-md border border-input bg-card px-2 text-body focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
				<input
					bind:value={scope}
					placeholder={t('page.ide.searchPathPlaceholder')}
					aria-label={t('page.ide.searchPathPlaceholder')}
					class="h-7 w-full rounded-md border border-input bg-card px-2 text-caption focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
				<input
					bind:value={include}
					placeholder={t('page.ide.searchIncludePlaceholder')}
					aria-label={t('page.ide.searchIncludePlaceholder')}
					class="h-7 w-full rounded-md border border-input bg-card px-2 text-caption focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
				<div class="flex flex-wrap gap-3 text-caption text-muted-foreground">
					<label class="flex cursor-pointer items-center gap-1">
						<input type="checkbox" bind:checked={caseSensitive} class="accent-primary" />
						{t('page.ide.searchCaseSensitive')}
					</label>
					<label class="flex cursor-pointer items-center gap-1">
						<input type="checkbox" bind:checked={wholeWord} class="accent-primary" />
						{t('page.ide.searchWholeWord')}
					</label>
					<label class="flex cursor-pointer items-center gap-1">
						<input type="checkbox" bind:checked={useRegex} class="accent-primary" />
						{t('page.ide.searchRegex')}
					</label>
				</div>
			</div>
			<div class="min-h-0 flex-1 overflow-y-auto p-1">
				{#if contentError}
					<p class="px-2 py-3 text-caption text-destructive">{contentError}</p>
				{:else if searching && matches.length === 0}
					<p class="px-2 py-6 text-center text-caption text-muted-foreground">{t('common.loading')}</p>
				{:else if matches.length === 0}
					<p class="px-2 py-6 text-center text-caption text-muted-foreground">
						{t('page.ide.searchEmpty')}
					</p>
				{:else}
					{#each grouped as group (group.path)}
						{@const isCollapsed = collapsed.has(group.path)}
						<div class="rounded-md">
							<button
								type="button"
								onclick={() => toggleGroup(group.path)}
								class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-caption font-medium hover:bg-accent/60"
								aria-expanded={!isCollapsed}
							>
								<span class="text-muted-foreground">{isCollapsed ? '▸' : '▾'}</span>
								<span class="min-w-0 flex-1 truncate">{group.path}</span>
								<span class="shrink-0 text-micro text-muted-foreground">{group.hits.length}</span>
							</button>
							{#if !isCollapsed}
								{#each group.hits as hit (hit.path + ':' + hit.line + ':' + hit.column)}
									{@const flatIndex = flatContent.indexOf(hit)}
									<button
										type="button"
										onclick={() => void openFile(hit.path, hit.line, hit.column)}
										onmouseenter={() => (contentSelected = flatIndex)}
										class="flex w-full flex-col gap-0.5 rounded-md px-2 py-1 pl-6 text-left {flatIndex === contentSelected ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/60'}"
									>
										<span class="flex w-full items-baseline gap-2">
											<span class="shrink-0 font-mono text-micro text-muted-foreground">
												{hit.line}{hit.column ? `:${hit.column}` : ''}
											</span>
											<span class="min-w-0 flex-1 truncate font-mono text-micro">{hit.preview}</span>
										</span>
									</button>
								{/each}
							{/if}
						</div>
					{/each}
				{/if}
				{#if contentTruncated}
					<p class="px-2 py-1 text-micro text-muted-foreground">{t('page.ide.searchTruncated')}</p>
					<button
						type="button"
						class="mx-2 my-1 rounded-md border border-border px-2 py-1 text-caption hover:bg-accent/60"
						onclick={() => void runContentSearch(true)}
						disabled={searching}
					>
						{searching ? t('common.loading') : t('page.ide.loadMore')}
					</button>
				{/if}
			</div>
		</div>
	{/if}
</div>
