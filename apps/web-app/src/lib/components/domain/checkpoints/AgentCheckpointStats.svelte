<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import {
		getAgentCheckpointStats,
		type AgentCheckpointStats,
	} from '#lib/services/checkpoints';
	import { t } from '#lib/i18n';

	let stats = $state<AgentCheckpointStats | null>(null);
	let note = $state<string | null>(null);

	$effect(() => {
		void load();
	});

	async function load(): Promise<void> {
		try {
			stats = await getAgentCheckpointStats();
			note = null;
		} catch (e) {
			console.error('Failed to load agent checkpoint stats:', e);
			note =
				e instanceof Error
					? e.message
					: t('domain.checkpoint.statsUnavailable');
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">
		{t('domain.checkpoint.statsTitle')}
	</h3>
	{#if note}
		<p class="text-micro text-muted-foreground">{note}</p>
	{:else if stats}
		<dl class="grid grid-cols-2 gap-x-3 gap-y-1 text-caption">
			<dt class="text-muted-foreground">{t('domain.checkpoint.statTotal')}</dt>
			<dd class="tabular-nums">{stats.total}</dd>
			<dt class="text-muted-foreground">{t('domain.checkpoint.statActive')}</dt>
			<dd class="tabular-nums">{stats.active}</dd>
			<dt class="text-muted-foreground">
				{t('domain.checkpoint.statAvgBlob')}
			</dt>
			<dd class="tabular-nums">
				{stats.avgBlobSize === null ? '—' : `${stats.avgBlobSize} B`}
			</dd>
		</dl>
		<div class="flex flex-wrap gap-1">
			{#each Object.entries(stats.byType) as [type, count] (type)}
				<Badge variant="neutral">{type}: {count}</Badge>
			{/each}
		</div>
	{:else}
		<p class="text-micro text-muted-foreground">{t('common.loading')}</p>
	{/if}
	<Button variant="ghost" size="sm" onclick={() => void load()}>
		{t('domain.checkpoint.refreshStats')}
	</Button>
</section>
