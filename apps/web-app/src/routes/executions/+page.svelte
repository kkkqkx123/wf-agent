<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import ExecutionCard from '$lib/components/domain/ExecutionCard.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import FilterBar from '@wf-agent/ui/components/FilterBar.svelte';
	import MetricGrid from '$lib/components/domain/MetricGrid.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import type { Column } from '@wf-agent/ui/components/table';
	import CursorPager from '@wf-agent/ui/components/CursorPager.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import { onMount } from 'svelte';
	import {
		listExecutions,
		listUnifiedExecutions,
		getExecutionDetail,
		getExecutionStats,
	} from '$lib/services/executions';
	import type { Execution, ExecutionDetail, Metric } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';

	/** Wire values of the backend execution status enum; the list endpoint
	 * matches them with exact equality, so aliasing them would filter to
	 * nothing. */
	const STATUS_OPTIONS = [
		{ value: 'created', label: 'Created' },
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'stopped', label: 'Stopped' },
		{ value: 'cancelled', label: 'Cancelled' },
		{ value: 'timeout', label: 'Timeout' },
	];

	let query = $state('');
	let status = $state('');
	let engine = $state<'all' | 'workflow' | 'agent_loop'>('workflow');
	let view = $state<'list' | 'table'>('list');
	let selectedId = $state<string | null>(null);
	let compareMode = $state(false);
	let compareId = $state<string | null>(null);
	let compareDetail = $state<ExecutionDetail | null>(null);
	let allExecutions = $state<{ items: Execution[]; hasMore: boolean }>({
		items: [],
		hasMore: false,
	});
	let nextCursor = $state<string | null>(null);
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
				engine === 'workflow'
					? listExecutions({
							limit: EXECUTIONS_PAGE,
							status: status || undefined,
						})
					: listUnifiedExecutions({
							limit: EXECUTIONS_PAGE,
							status: status || undefined,
							executionType: engine === 'all' ? undefined : engine,
						}),
				getExecutionStats(),
			]);
			allExecutions = { items: page.items, hasMore: page.hasMore };
			nextCursor = 'nextCursor' in page ? page.nextCursor : null;
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
			const page =
				engine === 'workflow'
					? await listExecutions({
							limit: EXECUTIONS_PAGE,
							offset: allExecutions.items.length,
							status: status || undefined,
						})
					: await listUnifiedExecutions({
							limit: EXECUTIONS_PAGE,
							cursor: nextCursor ?? undefined,
							status: status || undefined,
							executionType: engine === 'all' ? undefined : engine,
						});
			nextCursor = 'nextCursor' in page ? page.nextCursor : null;
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
		if (selectedItem?.kind === 'agent_loop') {
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

	$effect(() => {
		const id = compareId;
		if (!compareMode || !id) {
			compareDetail = null;
			return;
		}
		if (id === selectedId || comparedItem?.kind === 'agent_loop') {
			compareDetail = null;
			return;
		}
		void getExecutionDetail(id)
			.then((row) => {
				compareDetail = row;
			})
			.catch((e) => {
				compareDetail = null;
				toasts.error(
					'Compare detail failed',
					e instanceof Error ? e.message : undefined,
				);
			});
	});
	// The status travels to the server; the free-text query only narrows the
	// page already fetched, which is why it filters names/ids instead of payloads.
	const filtered = $derived(
		allExecutions.items.filter((execution) => {
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				execution.workflowName.toLowerCase().includes(needle) ||
				execution.id.toLowerCase().includes(needle);
			return matchesQuery;
		}),
	);

	const selected = $derived(detail);
	const compared = $derived(compareDetail);

	// Agent runs drill down on the agent-loops surface, whose detail
	// endpoints differ from the workflow ones used above.
	const selectedItem = $derived(
		filtered.find((execution) => execution.id === selectedId) ?? null,
	);
	const comparedItem = $derived(
		filtered.find((execution) => execution.id === compareId) ?? null,
	);

	// Compare candidates exclude the primary selection so A/B never render
	// the same run twice. Labels stay short: the full id remains in detail.
	const compareOptions = $derived(
		filtered
			.filter((execution) => execution.id !== selectedId)
			.map((execution) => ({
				value: execution.id,
				label: `${execution.workflowName} · ${execution.id.slice(0, 8)}`,
			})),
	);

	function toggleCompare(): void {
		compareMode = !compareMode;
		if (compareMode && !compareId) {
			compareId = compareOptions[0]?.value ?? null;
		}
		if (!compareMode) {
			compareId = null;
			compareDetail = null;
		}
	}

	function swapCompare(): void {
		const primary = selectedId;
		selectedId = compareId;
		compareId = primary;
		const primaryDetail = detail;
		detail = compareDetail;
		compareDetail = primaryDetail;
	}

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
	dual={compareMode}
	secondaryTitle="Compare"
	secondaryOpen={compareMode && compareId !== null}
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
				<Button
					variant="outline"
					size="sm"
					active={compareMode}
					onclick={toggleCompare}
				>
					<Icon name="copy" size={13} />
					Compare
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
				onstatuschange={() => void reload()}
			>
				{#snippet trailing()}
					<Select
						value={engine}
						options={[
							{ value: 'all', label: 'All engines' },
							{ value: 'workflow', label: 'Workflow' },
							{ value: 'agent_loop', label: 'Agent' },
						]}
						size="sm"
						class="w-36"
						onchange={(value) => {
							engine =
								value === 'agent_loop' || value === 'workflow'
									? value
									: 'all';
							selectedId = null;
							void reload();
						}}
					/>
					<span class="text-caption text-muted-foreground"
						>{filtered.length} shown</span
					>
				{/snippet}
			</FilterBar>

			{#if compareMode}
				<div
					class="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-border bg-card px-3 py-2"
				>
					<span class="text-caption text-muted-foreground">
						Compare A
						<span class="font-mono">{selectedId?.slice(0, 8) ?? '—'}</span>
						with B
					</span>
					<Select
						value={compareId ?? ''}
						options={compareOptions}
						size="sm"
						placeholder="Select execution B…"
						class="w-64"
						onchange={(value) => (compareId = value || null)}
					/>
					<IconButton
						icon="arrow-right"
						label="Swap A and B"
						disabled={!selectedId || !compareId}
						onclick={swapCompare}
					/>
					<span class="text-micro text-muted-foreground">
						Two full details side by side; topology merge stays single.
					</span>
				</div>
			{/if}

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
					tone="brand"
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
		{#if selectedItem?.kind === 'agent_loop' && selectedId}
			<div class="m-3 rounded-lg border border-border bg-card p-4">
				<p class="text-sm font-medium">Agent run {selectedId.slice(0, 8)}</p>
				<p class="text-caption text-muted-foreground">
					Agent runs drill down on the agent-loops surface.
				</p>
				<Button size="sm" href="/agent-loops" class="mt-2">
					Open agent loops
				</Button>
			</div>
		{:else if selected}
			<ExecutionInspector
				execution={selected}
				onrefresh={() => {
					const id = selectedId;
					if (!id) return;
					void getExecutionDetail(id)
						.then((row) => {
							detail = row;
						})
						.catch((e) => {
							toasts.error(
								'Execution detail failed',
								e instanceof Error ? e.message : undefined,
							);
						});
				}}
			/>
		{/if}
	{/snippet}

	{#snippet secondary()}
		{#if comparedItem?.kind === 'agent_loop' && compareId}
			<div class="m-3 rounded-lg border border-border bg-card p-4">
				<p class="text-sm font-medium">Agent run {compareId.slice(0, 8)}</p>
				<p class="text-caption text-muted-foreground">
					Agent runs drill down on the agent-loops surface.
				</p>
				<Button size="sm" href="/agent-loops" class="mt-2">
					Open agent loops
				</Button>
			</div>
		{:else if compared}
			<ExecutionInspector
				execution={compared}
				onrefresh={() => {
					const id = compareId;
					if (!id) return;
					void getExecutionDetail(id)
						.then((row) => {
							compareDetail = row;
						})
						.catch((e) => {
							toasts.error(
								'Compare detail failed',
								e instanceof Error ? e.message : undefined,
							);
						});
				}}
			/>
		{:else}
			<EmptyState
				icon="copy"
				title="Select execution B"
				description="Pick a second run above to compare full details side by side."
				class="m-3 rounded-lg border border-border bg-card"
			/>
		{/if}
	{/snippet}
</SplitView>

{#snippet executionStatusCell(execution: Execution)}
	<StatusBadge status={execution.status} size="sm" />
{/snippet}
