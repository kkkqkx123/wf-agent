<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import MetricGrid from '$lib/components/domain/MetricGrid.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import {
		loadDashboardStats,
		type DashboardStats,
	} from '$lib/services/dashboard';

	let stats = $state<DashboardStats | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		let cancelled = false;
		void loadDashboardStats()
			.then((data) => {
				if (!cancelled) {
					stats = data;
					error = null;
				}
			})
			.catch((err: unknown) => {
				if (!cancelled) {
					error = err instanceof Error ? err.message : String(err);
				}
			});
		return () => {
			cancelled = true;
		};
	});
</script>

<div class="h-full overflow-y-auto">
	<PageHeader
		title="Dashboard"
		description="Aggregated overview of workflows, executions and checkpoints"
	/>

	<div class="p-4 pt-0">
		{#if error}
			<ErrorState title="Failed to load dashboard" description={error} />
		{:else if !stats}
			<div
				class="grid gap-2 grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))]"
			>
				{#each Array(4).map((_, idx) => idx) as idx (idx)}
					<div
						class="h-20 animate-pulse rounded-lg border border-border bg-muted"
					></div>
				{/each}
			</div>
		{:else}
			<MetricGrid metrics={stats.metrics} />

			<section class="mt-6">
				<div class="flex items-center justify-between">
					<h2 class="text-heading font-semibold">Recent executions</h2>
					<button
						type="button"
						class="text-body text-muted-foreground hover:text-foreground"
						onclick={() => goto(resolve('/executions'))}
					>
						View all
					</button>
				</div>
				{#if stats.recentExecutions.length === 0}
					<EmptyState
						title="No executions yet"
						description="Run a workflow or start an agent loop to see activity here"
					/>
				{:else}
					<ul
						class="mt-2 divide-y divide-border rounded-lg border border-border bg-card"
					>
						{#each stats.recentExecutions as execution (execution.id)}
							<li>
								<button
									type="button"
									class="flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-accent/60"
									onclick={() =>
										goto(resolve('/executions/[id]', { id: execution.id }))}
								>
									<Icon
										name="activity"
										size={15}
										class="shrink-0 text-muted-foreground"
									/>
									<span class="min-w-0 flex-1 truncate text-body">
										{execution.workflowName ||
											execution.workflowId ||
											execution.id}
									</span>
									<StatusBadge status={execution.status} />
									<span
										class="hidden shrink-0 text-micro text-muted-foreground sm:block"
									>
										{execution.id.slice(0, 8)}
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</section>
		{/if}
	</div>
</div>
