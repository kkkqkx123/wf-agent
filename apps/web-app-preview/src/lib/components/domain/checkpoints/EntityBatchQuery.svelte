<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import CheckpointHits from '$lib/components/domain/checkpoints/CheckpointHits.svelte';
	import { getCheckpointsByEntities } from '$lib/services/checkpoints';
	import type { Checkpoint } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';

	let entityIds = $state('');
	let entityType = $state('checkpoint');
	let results = $state<Checkpoint[]>([]);
	let note = $state<string | null>(null);

	const entityTypeOptions = [
		{ value: 'checkpoint', label: 'Workflow execution (checkpoint)' },
		{ value: 'agent_loop', label: 'Agent loop (agent_loop)' },
	];

	async function run(): Promise<void> {
		const ids = entityIds
			.split(',')
			.map((id) => id.trim())
			.filter((id) => id.length > 0);
		if (ids.length === 0) {
			toasts.error('Enter at least one entity id');
			return;
		}
		try {
			const page = await getCheckpointsByEntities({
				entityIds: ids,
				entityType,
			});
			results = page.items;
			note = `${page.items.length} checkpoint(s)${page.hasMore ? ' (more available)' : ''}`;
		} catch (e) {
			console.error('Entity checkpoint query failed:', e);
			toasts.error(
				'Entity query failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">Entity batch</h3>
	<Input
		label="Entity ids"
		size="sm"
		value={entityIds}
		oninput={(event: Event) =>
			(entityIds = (event.target as HTMLInputElement).value)}
		placeholder="id-1, id-2, id-3"
	/>
	<Select
		label="Entity type"
		size="sm"
		value={entityType}
		options={entityTypeOptions}
		onchange={(value) => (entityType = value)}
	/>
	<Button variant="outline" size="sm" onclick={() => void run()}>
		Query entities
	</Button>
	{#if note}
		<p class="text-micro text-muted-foreground">{note}</p>
	{/if}
	<CheckpointHits rows={results} />
</section>
