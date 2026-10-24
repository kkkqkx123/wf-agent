<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import CheckpointHits from '#lib/components/domain/checkpoints/CheckpointHits.svelte';
	import { getCheckpointsByEntities } from '#lib/services/checkpoints';
	import type { Checkpoint } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';

	let entityIds = $state('');
	let entityType = $state('checkpoint');
	let results = $state<Checkpoint[]>([]);
	let note = $state<string | null>(null);

	const entityTypeOptions = $derived([
		{
			value: 'checkpoint',
			label: t('domain.checkpoint.typeCheckpoint'),
		},
		{ value: 'agent_loop', label: t('domain.checkpoint.typeAgentLoop') },
	]);

	async function run(): Promise<void> {
		const ids = entityIds
			.split(',')
			.map((id) => id.trim())
			.filter((id) => id.length > 0);
		if (ids.length === 0) {
			toasts.error(t('domain.checkpoint.entityIdsRequired'));
			return;
		}
		try {
			const page = await getCheckpointsByEntities({
				entityIds: ids,
				entityType,
			});
			results = page.items;
			note = t('domain.checkpoint.resultsNote', {
				count: page.items.length,
				more: page.hasMore ? t('domain.checkpoint.moreAvailable') : '',
			});
		} catch (e) {
			console.error('Entity checkpoint query failed:', e);
			toasts.error(
				t('domain.checkpoint.entityFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">
		{t('domain.checkpoint.entityBatchTitle')}
	</h3>
	<Input
		label={t('domain.checkpoint.entityIds')}
		size="sm"
		value={entityIds}
		oninput={(event: Event) =>
			(entityIds = (event.target as HTMLInputElement).value)}
		placeholder="id-1, id-2, id-3"
	/>
	<Select
		label={t('domain.checkpoint.entityType')}
		size="sm"
		value={entityType}
		options={entityTypeOptions}
		onchange={(value) => (entityType = value)}
	/>
	<Button variant="outline" size="sm" onclick={() => void run()}>
		{t('domain.checkpoint.queryEntities')}
	</Button>
	{#if note}
		<p class="text-micro text-muted-foreground">{note}</p>
	{/if}
	<CheckpointHits rows={results} />
</section>
