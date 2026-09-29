<script lang="ts">
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import { listExecutions } from '$lib/services/executions';
	import type { Execution } from '$lib/types/models';
	import { formatDateTime } from '$lib/utils/format';
	import { resolve } from '$app/paths';

	interface Props {
		workflowId: string;
	}

	let { workflowId }: Props = $props();

	let runs = $state<Execution[]>([]);
	let runsError = $state<string | null>(null);
	let runsLoading = $state(false);

	async function loadRuns(): Promise<void> {
		runsLoading = true;
		runsError = null;
		try {
			const result = await listExecutions({ workflowId, limit: 50 });
			runs = result.items;
		} catch (e) {
			runsError = e instanceof Error ? e.message : 'Runs failed to load.';
		} finally {
			runsLoading = false;
		}
	}

	onMount(() => {
		void loadRuns();
	});
</script>

{#if runsLoading}
	<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
{:else if runsError}
	<ErrorState
		title="Runs failed to load"
		description={runsError}
		onretry={() => void loadRuns()}
		class="rounded-lg border border-border bg-card"
	/>
{:else if runs.length === 0}
	<EmptyState
		icon="activity"
		title="No runs yet"
		description="Executions of this workflow appear here once it runs."
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<Card title="Runs" bodyClass="p-0">
		<ul class="divide-y divide-border">
			{#each runs as run (run.id)}
				<li
					class="flex items-center justify-between gap-2 px-3 py-2 text-caption"
				>
					<a
						href={resolve('/executions/[id]', { id: run.id })}
						class="truncate font-mono underline-offset-2 hover:underline"
					>
						{run.id}
					</a>
					<span class="flex shrink-0 items-center gap-2">
						<StatusBadge status={run.status} size="sm" dot={false} />
						<span class="text-muted-foreground">
							{formatDateTime(run.startedAt)}
						</span>
					</span>
				</li>
			{/each}
		</ul>
	</Card>
{/if}
