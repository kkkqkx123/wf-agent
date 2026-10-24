<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import {
		getStoreStats,
		runGc,
		type StoreStats,
	} from '#lib/services/checkpoints';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as store } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '#lib/stores/fileWorkspace.svelte';
	import { formatBytes, formatRelativeTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	let stats = $state<StoreStats | null>(null);
	let gcNote = $state<string | null>(null);
	let error = $state<string | null>(null);

	async function loadStats(): Promise<void> {
		try {
			stats = await getStoreStats();
			error = null;
		} catch (e) {
			console.error('Failed to load store stats:', e);
			error =
				e instanceof Error ? e.message : t('domain.files.statsUnavailable');
		}
	}

	async function runGcSweep(): Promise<void> {
		workspace.beginAction('gc');
		try {
			const result = await runGc();
			await loadStats();
			gcNote = t('domain.files.gcNote', {
				commits: result.removedCheckpoints,
				blobs: result.reclaimedSnapshots,
			});
			toasts.success(t('domain.files.gcCompleted'), gcNote);
			store.bump('files');
		} catch (e) {
			console.error('GC failed:', e);
			toasts.error(
				t('domain.files.gcFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}

	// The storage footprint is refreshed alongside the other file-tab
	// segments whenever the files domain is invalidated.
	$effect(() => {
		void store.filesVersion;
		void loadStats();
	});
</script>

<Card
	title={t('domain.files.storeTitle')}
	description={t('domain.files.storeDescription')}
>
	{#if stats}
		<div
			class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption tabular-nums text-muted-foreground"
		>
			<span>{t('domain.files.commits', { count: stats.commits })}</span>
			<span>{t('domain.files.refs', { count: stats.refs })}</span>
			<span
				>{t('domain.files.looseObjects', { count: stats.looseObjects })}</span
			>
			<span>{formatBytes(stats.storeBytes)}</span>
			<span>
				{stats.lastGcAt
					? t('domain.files.lastGc', {
							time: formatRelativeTime(stats.lastGcAt),
						})
					: t('domain.files.noGc')}
			</span>
		</div>
	{:else if error}
		<p class="text-caption text-destructive">{error}</p>
		<Button variant="ghost" size="sm" onclick={() => void loadStats()}>
			{t('common.retry')}
		</Button>
	{:else}
		<Skeleton lines={4} />
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
				{t('domain.files.runGc')}
			</Button>
			<Button
				variant="ghost"
				size="sm"
				disabled={workspace.busy}
				onclick={() => void loadStats()}
			>
				{t('common.refresh')}
			</Button>
		</div>
	{/snippet}
</Card>
