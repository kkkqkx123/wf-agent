<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import CursorPager from '@wf-agent/ui/components/CursorPager.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import type { StatusTone } from '@wf-agent/ui/status';
	import { getFileChangesPage } from '$lib/services/checkpoints';
	import type { FileChange } from '$lib/types/models';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { formatRelativeTime } from '$lib/utils/format';

	let {
		selectedId,
		onselect,
	}: {
		selectedId: string | null;
		onselect: (change: FileChange) => void;
	} = $props();

	const PAGE_SIZE = 100;

	let changes = $state<FileChange[]>([]);
	let changesHasMore = $state(false);
	let changesLoading = $state(false);
	let changesOffset = $state(0);
	let loading = $state(false);
	let loadError = $state<string | null>(null);

	const CHANGE_TONE: Record<string, StatusTone> = {
		added: 'success',
		modified: 'info',
		renamed: 'warning',
		deleted: 'danger',
	};

	// This card owns the changes list: every files-domain bump (SSE file
	// events plus local workspace mutations) reloads the first page.
	$effect(() => {
		void store.filesVersion;
		void reload();
	});

	async function reload(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			const page = await getFileChangesPage({ limit: PAGE_SIZE });
			changes = page.items;
			changesOffset = page.items.length;
			changesHasMore = page.hasMore;
		} catch (e) {
			console.error('Failed to load file changes:', e);
			loadError = e instanceof Error ? e.message : 'Changes unavailable';
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (changesLoading || !changesHasMore) return;
		changesLoading = true;
		try {
			const page = await getFileChangesPage({
				limit: PAGE_SIZE,
				offset: changesOffset,
			});
			changes = [...changes, ...page.items];
			changesOffset += page.items.length;
			changesHasMore = page.hasMore;
		} catch {
			// Pagination failures stay local; the loaded prefix remains.
		} finally {
			changesLoading = false;
		}
	}
</script>

<Card title="Changes" description="Recent file deltas grouped by session">
	{#if loading && changes.length === 0}
		<Skeleton lines={5} />
	{:else if loadError && changes.length === 0}
		<p class="text-caption text-destructive">{loadError}</p>
		<Button variant="ghost" size="sm" onclick={() => void reload()}>
			Retry
		</Button>
	{:else}
		<ul class="divide-y divide-border">
			{#each changes as change (change.id)}
				<li class="py-2 first:pt-0">
					<button
						type="button"
						onclick={() => onselect(change)}
						aria-pressed={selectedId === change.id}
						class={cn(
							'flex w-full items-start justify-between gap-3 rounded-md px-2 py-1 text-left transition-colors hover:bg-accent/50',
							selectedId === change.id && 'bg-accent/70',
						)}
					>
						<div class="min-w-0">
							<p class="truncate font-mono text-caption">
								{change.path}
							</p>
							<p class="mt-0.5 text-micro text-muted-foreground">
								{change.actor} · {change.session} · {formatRelativeTime(
									change.at,
								)}
							</p>
						</div>
						<div class="flex shrink-0 items-center gap-2">
							{#if change.changeType}
								<span
									class={cn(
										'text-micro',
										toneText(CHANGE_TONE[change.changeType]),
									)}
								>
									{change.changeType}
								</span>
							{/if}
							{#if change.additions !== null && change.deletions !== null}
								<span class="text-micro tabular-nums text-success">
									+{change.additions}
								</span>
								<span class="text-micro tabular-nums text-destructive">
									-{change.deletions}
								</span>
							{/if}
						</div>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet footer()}
		<CursorPager
			shown={changes.length}
			hasMore={changesHasMore}
			loading={changesLoading}
			pageSize={PAGE_SIZE}
			onloadmore={() => void loadMore()}
		/>
	{/snippet}
</Card>
