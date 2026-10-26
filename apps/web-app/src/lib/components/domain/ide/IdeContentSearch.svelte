<script lang="ts">
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { searchFsContent, type FsContentMatch } from '#lib/services/fs';
	import { t } from '#lib/i18n';

	interface Props {
		open: boolean;
		onclose: () => void;
	}

	let { open, onclose }: Props = $props();

	let query = $state('');
	let scope = $state('');
	let caseSensitive = $state(false);
	let wholeWord = $state(false);
	let useRegex = $state(false);
	let matches = $state<FsContentMatch[]>([]);
	let truncated = $state(false);
	let searching = $state(false);
	let error = $state<string | null>(null);
	let selected = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);

	$effect(() => {
		if (open) {
			error = null;
			inputEl?.focus();
		} else {
			query = '';
			scope = '';
			matches = [];
			truncated = false;
			selected = 0;
		}
	});

	$effect(() => {
		if (!open) return;
		const needle = query.trim();
		if (needle.length < 2) {
			matches = [];
			truncated = false;
			error = null;
			return;
		}
		let cancelled = false;
		const timer = setTimeout(() => {
			searching = true;
			void searchFsContent(needle, {
				path: scope.trim(),
				caseSensitive,
				wholeWord,
				regex: useRegex,
				limit: 100,
			})
				.then((result) => {
					if (cancelled || !open) return;
					matches = result.matches;
					truncated = result.truncated;
					selected = 0;
					error = null;
				})
				.catch((e) => {
					if (!cancelled) {
						error = e instanceof Error ? e.message : String(e);
						matches = [];
					}
				})
				.finally(() => {
					if (!cancelled) searching = false;
				});
		}, 300);
		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	});

	async function choose(match: FsContentMatch): Promise<void> {
		await ide.openFile(match.path, false, match.line, match.column ?? 1);
		onclose();
	}

	function onKey(event: KeyboardEvent): void {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			selected = matches.length ? (selected + 1) % matches.length : 0;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			selected = matches.length ? (selected - 1 + matches.length) % matches.length : 0;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const hit = matches[selected];
			if (hit) void choose(hit);
		}
	}
</script>

<Dialog {open} title={t('page.ide.searchTitle')} description={t('page.ide.searchDesc')} width="40rem" {onclose}>
	<div onkeydown={onKey} role="presentation" class="space-y-2">
		<input
			bind:this={inputEl}
			bind:value={query}
			placeholder={t('page.ide.searchPlaceholder')}
			aria-label={t('page.ide.searchPlaceholder')}
			class="h-9 w-full rounded-md border border-input bg-card px-3 text-body focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
		/>
		<input
			bind:value={scope}
			placeholder={t('page.ide.searchPathPlaceholder')}
			aria-label={t('page.ide.searchPathPlaceholder')}
			class="h-8 w-full rounded-md border border-input bg-card px-3 text-caption focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
		/>
		<div class="flex flex-wrap gap-4 text-caption text-muted-foreground">
			<label class="flex cursor-pointer items-center gap-1.5">
				<input type="checkbox" bind:checked={caseSensitive} class="accent-primary" />
				{t('page.ide.searchCaseSensitive')}
			</label>
			<label class="flex cursor-pointer items-center gap-1.5">
				<input type="checkbox" bind:checked={wholeWord} class="accent-primary" />
				{t('page.ide.searchWholeWord')}
			</label>
			<label class="flex cursor-pointer items-center gap-1.5">
				<input type="checkbox" bind:checked={useRegex} class="accent-primary" />
				{t('page.ide.searchRegex')}
			</label>
		</div>
		<div class="max-h-72 overflow-y-auto">
			{#if error}
				<p class="px-2 py-3 text-caption text-destructive">{error}</p>
			{:else if searching && matches.length === 0}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">{t('common.loading')}</p>
			{:else if matches.length === 0}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					{t('page.ide.searchEmpty')}
				</p>
			{:else}
				{#each matches as hit, index (hit.path + ':' + hit.line + ':' + hit.column)}
					<button
						type="button"
						onclick={() => void choose(hit)}
						onmouseenter={() => (selected = index)}
						class="flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left {index === selected
							? 'bg-accent text-accent-foreground'
							: 'hover:bg-accent/60'}"
					>
						<span class="flex w-full items-baseline gap-2">
							<span class="min-w-0 flex-1 truncate text-body">{hit.path}</span>
							<span class="shrink-0 font-mono text-micro text-muted-foreground">
								{hit.line}{hit.column ? `:${hit.column}` : ''}
							</span>
						</span>
						<span class="w-full truncate font-mono text-micro text-muted-foreground">{hit.preview}</span>
					</button>
				{/each}
			{/if}
			{#if truncated}
				<p class="px-2 py-1 text-micro text-muted-foreground">{t('page.ide.searchTruncated')}</p>
			{/if}
		</div>
		<div class="flex justify-end">
			<Button size="sm" variant="ghost" onclick={onclose}>{t('common.close')}</Button>
		</div>
	</div>
</Dialog>
