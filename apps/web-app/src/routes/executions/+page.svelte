<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import ExecutionCard from '$lib/components/domain/ExecutionCard.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import {
		listExecutions,
		getExecution,
		listToolCalls,
		listTimeline,
	} from '$lib/services/executions';
	import type {
		Execution,
		ExecutionDetail,
		ToolCallEntry,
		TimelineEntry,
	} from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { live } from '$lib/stores/live.svelte';
	import { formatDateTime, nodeCount } from '$lib/utils/format';
	import {
		loadWorkflowTitles,
		workflowTitle,
	} from '$lib/stores/workflow-titles.svelte';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const STATUS_OPTIONS = [
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' },
	];

	interface ExecutionBundle {
		execution: ExecutionDetail;
		toolCalls: ToolCallEntry[];
		timeline: TimelineEntry[];
	}

	const initial = parseListParams(page.url);

	let query = $state(initial.q ?? '');
	let status = $state(initial.status ?? '');
	let view = $state<'list' | 'table'>(
		initial.tab === 'table' ? 'table' : 'list',
	);
	let selectedId = $state<string | null>(initial.id ?? null);

	const list = createCollection((params) => listExecutions(params));
	const detail = createResource<ExecutionBundle | null>(async () => {
		if (!selectedId) return null;
		const id = selectedId;
		const [execution, toolCalls, timeline] = await Promise.all([
			getExecution(id),
			listToolCalls(id),
			listTimeline(id),
		]);
		return { execution, toolCalls, timeline };
	});

	const filtered = $derived(
		list.items.filter((execution) => {
			const matchesStatus = !status || execution.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				workflowTitle(execution.workflowId).toLowerCase().includes(needle) ||
				execution.id.toLowerCase().includes(needle);
			return matchesStatus && matchesQuery;
		}),
	);

	$effect(() => {
		if (selectedId) void detail.reload();
	});

	// Auto-select the first execution once the list has loaded.
	$effect(() => {
		if (!selectedId && list.loaded > 0 && !list.loading) {
			selectedId = list.items[0].id;
		}
	});

	// The address bar mirrors whatever is on screen, so a reload or a shared
	// link restores the same filter, selection and paging depth.
	$effect(() => {
		gotoWithParams(page.url, {
			q: query,
			status,
			tab: view === 'table' ? 'table' : '',
			id: selectedId ?? '',
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void loadWorkflowTitles();
		void list.loadPages(Number(initial.page) || 1);
		// Live execution-state events trigger a throttled reload; no inline
		// row splicing since the list is offset-paginated.
		let timer: ReturnType<typeof setTimeout> | null = null;
		const scheduleReload = () => {
			if (timer) return;
			timer = setTimeout(() => {
				timer = null;
				void list.reload();
				if (selectedId) void detail.reload();
			}, 1000);
		};
		const unsubscribe = live.subscribe((event) => {
			if (!behavior.autoRefresh) return;
			if (!event.type.startsWith('WORKFLOW_EXECUTION_')) return;
			scheduleReload();
		});
		return () => {
			unsubscribe();
			if (timer) clearTimeout(timer);
		};
	});
</script>

<SplitView
	inspectorTitle="Execution detail"
	inspectorOpen={selectedId !== null}
	oninspectorclose={() => (selectedId = null)}
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
					onclick={() => {
						list.reload();
						if (selectedId) detail.reload();
					}}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => (view = view === 'list' ? 'table' : 'list')}
				>
					<Icon name={view === 'list' ? 'blocks' : 'menu'} size={13} />
					{view === 'list' ? 'Table' : 'Cards'}
				</Button>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
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

			{#if list.loading && list.loaded === 0}
				<div class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
					{#each Array.from({ length: 6 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="104px" class="rounded-lg" />
					{/each}
				</div>
			{:else if list.error}
				<ErrorState
					title="Failed to load executions"
					description={list.error}
					onretry={() => list.reload()}
					class="rounded-lg border border-border bg-card"
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
				{#snippet executionId(execution: Execution)}
					<span class="font-mono text-caption">{execution.id}</span>
				{/snippet}
				{#snippet executionStatus(execution: Execution)}
					<StatusBadge status={execution.status} size="sm" />
				{/snippet}
				{#snippet executionStarted(execution: Execution)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{formatDateTime(execution.startedAt)}
					</span>
				{/snippet}
				{#snippet executionTasks(execution: Execution)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{nodeCount(execution.nodesDone, execution.nodesTotal) ?? '—'}
					</span>
				{/snippet}
				<Card bodyClass="p-0">
					<DataTable
						rows={filtered}
						rowKey={(row) => row.id}
						selectedKey={selectedId}
						onrowclick={(row) => (selectedId = row.id)}
						columns={[
							{ key: 'id', header: 'Execution', cell: executionId },
							{
								key: 'workflow',
								header: 'Workflow',
								text: (row) => workflowTitle(row.workflowId),
							},
							{ key: 'status', header: 'Status', cell: executionStatus },
							{ key: 'started', header: 'Started', cell: executionStarted },
							{
								key: 'tasks',
								header: 'Tasks',
								align: 'right',
								cell: executionTasks,
							},
						]}
					/>
				</Card>
			{/if}

			<LoadMorePager
				shown={list.loaded}
				hasMore={list.hasMore}
				loading={list.loading}
				pageSize={list.pageSize}
				onloadmore={() => list.loadMore()}
				class="mt-3 rounded-lg border border-border bg-card"
			/>
		</div>
	</div>

	{#snippet inspector()}
		{#if detail.loading && !detail.data}
			<div class="space-y-3 p-4">
				<Skeleton lines={2} />
				<Skeleton shape="block" height="120px" class="rounded-lg" />
				<Skeleton lines={4} />
			</div>
		{:else if detail.error}
			<ErrorState
				title="Failed to load detail"
				description={detail.error}
				onretry={() => detail.reload()}
				class="m-4"
			/>
		{:else if detail.data}
			<ExecutionInspector
				execution={detail.data.execution}
				toolCalls={detail.data.toolCalls}
				timeline={detail.data.timeline}
			/>
		{/if}
	{/snippet}
</SplitView>
