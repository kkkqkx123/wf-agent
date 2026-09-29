<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import ExecutionCard from '$lib/components/domain/ExecutionCard.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import FilterBar from '$lib/components/ui/FilterBar.svelte';
	import MetricGrid from '$lib/components/domain/MetricGrid.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import CursorPager from '$lib/components/ui/CursorPager.svelte';
	import { onMount } from 'svelte';
	import {
		listExecutions,
		getExecutionDetail,
		getExecutionStats,
	} from '$lib/services/executions';
	import type { Execution, ExecutionDetail, Metric } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';

	const STATUS_OPTIONS = [
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' },
	];

	let query = $state('');
	let status = $state('');
	let view = $state<'list' | 'table'>('list');
	let selectedId = $state<string | null>(null);
	let allExecutions = $state<{ items: Execution[]; hasMore: boolean }>({
		items: [],
		hasMore: false,
	});
	let detail = $state<ExecutionDetail | null>(null);
	let overviewMetrics = $state<Metric[]>([]);
	let loading = $state(true);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);

	const EXECUTIONS_PAGE = 200;

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		loading = true;
		error = null;
		try {
			const [page, metrics] = await Promise.all([
				listExecutions({ limit: EXECUTIONS_PAGE }),
				getExecutionStats(),
			]);
			allExecutions = { items: page.items, hasMore: page.hasMore };
			overviewMetrics = metrics;
			if (page.items.length > 0 && !selectedId) {
				selectedId = page.items[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (loadingMore || !allExecutions.hasMore) return;
		loadingMore = true;
		try {
			const page = await listExecutions({
				limit: EXECUTIONS_PAGE,
				offset: allExecutions.items.length,
			});
			allExecutions = {
				items: [...allExecutions.items, ...page.items],
				hasMore: page.hasMore,
			};
		} catch (e) {
			console.error('Failed to load more executions:', e);
			toasts.error('More executions unavailable');
		} finally {
			loadingMore = false;
		}
	}

	$effect(() => {
		const id = selectedId;
		if (!id) {
			detail = null;
			return;
		}
		void getExecutionDetail(id)
			.then((row) => {
				detail = row;
			})
			.catch((e) => {
				detail = null;
				toasts.error(
					'Execution detail failed',
					e instanceof Error ? e.message : undefined,
				);
			});
	});

	const filtered = $derived(
		allExecutions.items.filter((execution) => {
			const matchesStatus = !status || execution.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				execution.workflowName.toLowerCase().includes(needle) ||
				execution.id.toLowerCase().includes(needle);
			return matchesStatus && matchesQuery;
		}),
	);

	const selected = $derived(detail);

	const executionColumns: Column<Execution>[] = [
		{
			key: 'id',
			header: 'Execution',
			text: (execution) => execution.id,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'workflow',
			header: 'Workflow',
			text: (execution) => execution.workflowName,
		},
		{ key: 'status', header: 'Status', cell: executionStatusCell },
		{
			key: 'started',
			header: 'Started',
			text: (execution) => formatDateTime(execution.startedAt),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'tasks',
			header: 'Tasks',
			align: 'right',
			text: (execution) => `${execution.tasksDone}/${execution.tasksTotal}`,
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
	];
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
					onclick={() => void reload()}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => (view = view === 'list' ? 'table' : 'list')}
				>
					<Icon name={view === 'list' ? 'blocks' : 'menu'} size={13} />
					{view === 'list' ? 'Table' : 'Cards'}
				</Button>
				<Button size="sm" href="/workflows">
					<Icon name="play" size={13} />
					Start from workflow
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

			{#if loading}
				<div class="space-y-2">
					<Skeleton shape="block" height="72px" class="rounded-lg" />
					<Skeleton shape="block" height="72px" class="rounded-lg" />
				</div>
			{:else if error}
				<ErrorState
					title="Failed to load executions"
					description={error}
					onretry={() => void reload()}
				/>
			{:else if filtered.length === 0}
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
					<DataTable
						columns={executionColumns}
						rows={filtered}
						rowKey={(execution) => execution.id}
						selectedKey={selectedId}
						onrowclick={(execution) => (selectedId = execution.id)}
						virtualize={false}
					/>
				</Card>
			{/if}

			<CursorPager
				shown={filtered.length}
				hasMore={allExecutions.hasMore}
				loading={loadingMore}
				pageSize={EXECUTIONS_PAGE}
				onloadmore={() => void loadMore()}
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

{#snippet executionStatusCell(execution: Execution)}
	<StatusBadge status={execution.status} size="sm" />
{/snippet}
