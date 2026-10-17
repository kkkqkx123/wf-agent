<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import {
		getStoreStats,
		runGc,
		type StoreStats,
	} from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';
	import { formatBytes, formatRelativeTime } from '$lib/utils/format';

	let stats = $state<StoreStats | null>(null);
	let gcNote = $state<string | null>(null);

	async function loadStats(): Promise<void> {
		try {
			stats = await getStoreStats();
		} catch (e) {
			console.error('Failed to load store stats:', e);
		}
	}

	async function runGcSweep(): Promise<void> {
		workspace.beginAction('gc');
		try {
			const result = await runGc();
			await loadStats();
			gcNote = `Removed ${result.removedCheckpoints} commits, reclaimed ${result.reclaimedSnapshots} blobs`;
			toasts.success('GC completed', gcNote);
			store.bump('files');
		} catch (e) {
			console.error('GC failed:', e);
			workspace.endAction('GC failed');
			toasts.error('GC failed', e instanceof Error ? e.message : undefined);
			return;
		}
		workspace.endAction();
	}

	// The storage footprint is refreshed alongside the other file-tab
	// segments whenever the files domain is invalidated.
	$effect(() => {
		void store.filesVersion;
		void loadStats();
	});
</script>

<Card
	title="Object store"
	description="File-checkpoint storage footprint and garbage collection"
>
	{#if stats}
		<div
			class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption tabular-nums text-muted-foreground"
		>
			<span>{stats.commits} commits</span>
			<span>{stats.refs} refs</span>
			<span>{stats.looseObjects} loose objects</span>
			<span>{formatBytes(stats.storeBytes)}</span>
			<span>
				{stats.lastGcAt
					? `last GC ${formatRelativeTime(stats.lastGcAt)}`
					: 'no GC yet'}
			</span>
		</div>
	{:else}
		<p class="text-caption text-muted-foreground">Loading store statistics…</p>
	{/if}
	{#if gcNote}
		<p class="mt-1.5 text-micro text-muted-foreground">{gcNote}</p>
	{/if}
	{#snippet footer()}
		<div class="flex flex-wrap items-center gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={workspace.busy}
				onclick={() => void runGcSweep()}
			>
				<Icon name="database" size={13} />
				Run GC
			</Button>
			<Button
				variant="ghost"
				size="sm"
				disabled={workspace.busy}
				onclick={() => void loadStats()}
			>
				Refresh
			</Button>
		</div>
	{/snippet}
</Card>
