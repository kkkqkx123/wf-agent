<script lang="ts">
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { filterFsTree, getFsTreeIndex } from '#lib/utils/fs-tree-index';
	import { t } from '#lib/i18n';

	interface Props {
		open: boolean;
		onclose: () => void;
	}

	let { open, onclose }: Props = $props();

	let query = $state('');
	let results = $state<Array<{ path: string; is_dir: boolean }>>([]);
	let truncated = $state(false);
	let selected = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);

	function parseLocation(raw: string): { path: string; line: number | null; col: number | null } {
		const match = raw.match(/^(.*?)(?::(\d+)(?::(\d+))?)?$/);
		const path = (match?.[1] ?? raw).trim();
		const line = match?.[2] ? Number.parseInt(match[2], 10) : null;
		const col = match?.[3] ? Number.parseInt(match[3], 10) : null;
		return {
			path,
			line: line !== null && Number.isFinite(line) && line >= 1 ? Math.floor(line) : null,
			col: col !== null && Number.isFinite(col) && col >= 1 ? Math.floor(col) : null,
		};
	}

	$effect(() => {
		if (!open) {
			query = '';
			results = [];
			truncated = false;
			selected = 0;
			return;
		}
		inputEl?.focus();
		void getFsTreeIndex(ide.root || 'workspace')
			.then((index) => {
				if (!open) return;
				truncated = index.truncated;
				results = filterFsTree(index.entries, '', 20).filter((e) => !e.is_dir);
			})
			.catch(() => {
				results = [];
			});
	});

	$effect(() => {
		if (!open) return;
		const needle = query.trim();
		if (needle.length < 2) {
			results = [];
			truncated = false;
			selected = 0;
			return;
		}
		let cancelled = false;
		const timer = setTimeout(() => {
			void getFsTreeIndex(ide.root || 'workspace')
				.then((index) => {
					if (cancelled || !open) return;
					const { path } = parseLocation(needle);
					truncated = index.truncated;
					results = filterFsTree(index.entries, path, 20).filter((e) => !e.is_dir);
					selected = 0;
				})
				.catch(() => {
					if (!cancelled) results = [];
				});
		}, 120);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	});

	async function choose(path: string): Promise<void> {
		const { path: clean, line, col } = parseLocation(`${path}${suffixFor(query)}`);
		await ide.openFile(clean, false, line, col);
		onclose();
	}

	function suffixFor(raw: string): string {
		const match = raw.trim().match(/^(.*?:\d+(?::\d+)?)$/);
		if (!match) return '';
		const full = match[1] ?? '';
		const colon = full.indexOf(':');
		return colon >= 0 ? full.slice(colon) : '';
	}

	function onKey(event: KeyboardEvent): void {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			selected = results.length ? (selected + 1) % results.length : 0;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			selected = results.length ? (selected - 1 + results.length) % results.length : 0;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const hit = results[selected];
			if (hit) void choose(hit.path);
		}
	}
</script>

<Dialog {open} title={t('page.ide.quickOpenTitle')} description={t('page.ide.quickOpenDesc')} width="36rem" {onclose}>
	<div onkeydown={onKey} role="presentation">
		<input
			bind:this={inputEl}
			bind:value={query}
			placeholder={t('page.ide.quickOpenPlaceholder')}
			aria-label={t('page.ide.quickOpenPlaceholder')}
			class="h-9 w-full rounded-md border border-input bg-card px-3 text-body focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
		/>
		<div class="mt-2 max-h-72 overflow-y-auto">
			{#if results.length === 0}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					{t('page.ide.quickOpenEmpty')}
				</p>
			{:else}
				{#each results as hit, index (hit.path)}
					{@const name = hit.path.split('/').pop() ?? hit.path}
					<button
						type="button"
						onclick={() => void choose(hit.path)}
						onmouseenter={() => (selected = index)}
						class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left {index === selected
							? 'bg-accent text-accent-foreground'
							: 'hover:bg-accent/60'}"
					>
						<span class="min-w-0 flex-1 truncate text-body">{name}</span>
						<span class="shrink-0 truncate text-micro text-muted-foreground">{hit.path}</span>
					</button>
				{/each}
			{/if}
			{#if truncated}
				<p class="px-2 py-1 text-micro text-muted-foreground">{t('page.ide.quickOpenTruncated')}</p>
			{/if}
		</div>
	</div>
</Dialog>
