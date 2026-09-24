<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import ExecutionCard from '$lib/components/domain/ExecutionCard.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import MetricGrid from '$lib/components/domain/MetricGrid.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import CursorPager from '$lib/components/domain/CursorPager.svelte';
	import type { PageData } from './$types';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	const STATUS_OPTIONS = [
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' },
	];

	let { data }: { data: PageData } = $props();
	let { executions, executionDetail, overviewMetrics } = $derived(data);

	let query = $state('');
	let status = $state('');
	let view = $state<'list' | 'table'>('list');
	let selectedId = $state<string | null>(executions[0]?.id ?? null);

	const filtered = $derived(
		executions.filter((execution) => {
			const matchesStatus = !status || execution.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				execution.workflowName.toLowerCase().includes(needle) ||
				execution.id.toLowerCase().includes(needle);
			return matchesStatus && matchesQuery;
		}),
	);

	const selected = $derived(executionDetail);
</script>

<SplitView
	inspectorTitle="Execution detail"
	inspectorOpen={selectedId !== null}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title="Execution workbench"
			description="Live execution list with status, duration and per-run detail."
		>
			{#snippet actions()}
				<IconButton
					icon="refresh"
					label="Refresh"
					onclick={() => toasts.info('Refresh queued')}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => (view = view === 'list' ? 'table' : 'list')}
				>
					<Icon name={view === 'list' ? 'blocks' : 'menu'} size={13} />
					{view === 'list' ? 'Table' : 'Cards'}
				</Button>
				<Button
					size="sm"
					onclick={() => toasts.success('Execution request prepared')}
				>
					<Icon name="play" size={13} />
					Start
				</Button>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<MetricGrid metrics={overviewMetrics} class="mb-3" />

			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder="Filter by workflow or id…"
				class="mb-3"
			>
				{#snippet trailing()}
					<span class="text-caption text-muted-foreground"
						>{filtered.length} shown</span
					>
				{/snippet}
			</FilterBar>

			{#if filtered.length === 0}
				<EmptyState
					icon="activity"
					title="No executions match"
					description="Adjust the status filter or clear the search to see more runs."
					class="rounded-lg border border-border bg-card"
				/>
			{:else if view === 'list'}
				<div class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
					{#each filtered as execution (execution.id)}
						<ExecutionCard
							{execution}
							selected={selectedId === execution.id}
							onselect={(item) => (selectedId = item.id)}
						/>
					{/each}
				</div>
			{:else}
				<Card bodyClass="p-0">
					<div class="overflow-x-auto">
						<table class="w-full border-collapse text-body">
							<thead>
								<tr class="border-b border-border">
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Execution</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Workflow</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Status</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Started</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Tasks</th
									>
								</tr>
							</thead>
							<tbody>
								{#each filtered as execution (execution.id)}
									<tr
										class={cn(
											'cursor-pointer border-b border-border/60 transition-colors last:border-0',
											selectedId === execution.id
												? 'bg-accent/70'
												: 'hover:bg-accent/40',
										)}
										onclick={() => (selectedId = execution.id)}
									>
										<td class="px-3 py-2 font-mono text-caption"
											>{execution.id}</td
										>
										<td class="px-3 py-2">{execution.workflowName}</td>
										<td class="px-3 py-2"
											><StatusBadge status={execution.status} size="sm" /></td
										>
										<td
											class="px-3 py-2 tabular-nums text-caption text-muted-foreground"
										>
											{formatDateTime(execution.startedAt)}
										</td>
										<td
											class="px-3 py-2 text-right tabular-nums text-caption text-muted-foreground"
										>
											{execution.tasksDone}/{execution.tasksTotal}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</Card>
			{/if}

			<CursorPager
				shown={filtered.length}
				hasMore={false}
				class="mt-3 rounded-lg border border-border bg-card"
			/>
		</div>
	</div>

	{#snippet inspector()}
		{#if selected}
			<ExecutionInspector execution={selected} />
		{/if}
	{/snippet}
</SplitView>
