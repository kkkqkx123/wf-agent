<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import CheckpointHits from '$lib/components/domain/checkpoints/CheckpointHits.svelte';
	import { getCheckpointsByTimeRange } from '$lib/services/checkpoints';
	import type { Checkpoint } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';

	let workflowId = $state('');
	let rangeStart = $state('');
	let rangeEnd = $state('');
	let results = $state<Checkpoint[]>([]);
	let note = $state<string | null>(null);

	async function run(): Promise<void> {
		const workflow = workflowId.trim();
		const start = Number(rangeStart);
		const end = Number(rangeEnd);
		if (!workflow || !Number.isFinite(start) || !Number.isFinite(end)) {
			toasts.error('Enter a workflow id and numeric start/end milliseconds');
			return;
		}
		try {
			const page = await getCheckpointsByTimeRange({ workflowId: workflow, start, end });
			results = page.items;
			note = `${page.items.length} checkpoint(s)${page.hasMore ? ' (more available)' : ''}`;
		} catch (e) {
			console.error('Time-range query failed:', e);
			toasts.error(
				'Time-range query failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}
</script>

<section class="space-y-2">
	<h3 class="text-caption font-medium text-foreground">Time range</h3>
	<Input
		label="Workflow id"
		size="sm"
		value={workflowId}
		oninput={(event: Event) =>
			(workflowId = (event.target as HTMLInputElement).value)}
		placeholder="workflow id"
	/>
	<div class="grid grid-cols-2 gap-2">
		<Input
			label="Start (epoch ms)"
			size="sm"
			value={rangeStart}
			oninput={(event: Event) =>
				(rangeStart = (event.target as HTMLInputElement).value)}
			placeholder="0"
		/>
		<Input
			label="End (epoch ms)"
			size="sm"
			value={rangeEnd}
			oninput={(event: Event) =>
				(rangeEnd = (event.target as HTMLInputElement).value)}
			placeholder="0"
		/>
	</div>
	<Button variant="outline" size="sm" onclick={() => void run()}>
		Query range
	</Button>
	{#if note}
		<p class="text-micro text-muted-foreground">{note}</p>
	{/if}
	<CheckpointHits rows={results} />
</section>
