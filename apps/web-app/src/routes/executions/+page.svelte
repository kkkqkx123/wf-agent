<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import SplitView from '#lib/components/layout/SplitView.svelte';
	import ExecutionCard from '#lib/components/domain/ExecutionCard.svelte';
	import ExecutionInspector from '#lib/components/domain/ExecutionInspector.svelte';
	import FilterBar from '@wf-agent/ui/components/FilterBar.svelte';
	import MetricGrid from '#lib/components/domain/MetricGrid.svelte';
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
	} from '#lib/services/executions';
	import SessionInspector from '#lib/components/domain/SessionInspector.svelte';
	import type { Execution, ExecutionDetail, Metric } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	/** Wire values of the backend execution status enum; the list endpoint
	 * matches them with exact equality, so aliasing them would filter to
	 * nothing. */
	const STATUS_OPTIONS = [
		{ value: 'created', label: t('domain.runs.statusCreated') },
		{ value: 'running', label: t('domain.runs.statusRunning') },
		{ value: 'paused', label: t('domain.runs.statusPaused') },
		{ value: 'completed', label: t('domain.runs.statusCompleted') },
		{ value: 'failed', label: t('domain.runs.statusFailed') },
		{ value: 'stopped', label: t('domain.runs.statusStopped') },
		{ value: 'cancelled', label: t('domain.runs.statusCancelled') },
		{ value: 'timeout', label: t('domain.runs.statusTimeout') },
	];

	let query = $state('');
	let status = $state('');
	let engine = $state<'all' | 'workflow' | 'agent' | 'chat'>('all');
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
			toasts.error(t('page.executions.moreUnavailable'));
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
		// Agent and chat runs render through the session inspector, which
		// loads its own data; only workflow runs use the workflow detail.
		const kind = allExecutions.items.find((row) => row.id === id)?.kind;
		if (kind === 'agent' || kind === 'chat') {
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
					t('page.executions.detailFailed'),
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
		if (id === selectedId) {
			compareDetail = null;
			return;
		}
		// The compare view is the workflow run inspector; non-workflow
		// candidates never enter it.
		const kind = allExecutions.items.find((row) => row.id === id)?.kind;
		if (kind === 'agent' || kind === 'chat') {
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
					t('page.executions.compareDetailFailed'),
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

	// The inspector switches by executor kind: agent and chat runs render
	// the session transcript, workflow runs render the run inspector. Rows
	// from the workflow-only list carry no kind and read as workflow.
	const selectedKind = $derived(
		filtered.find((execution) => execution.id === selectedId)?.kind ?? 'workflow',
	);

	// Compare candidates exclude the primary selection so A/B never render
	// the same run twice. Labels stay short: the full id remains in detail.
	// Only workflow runs are comparable: the compare view is the run
	// inspector, while agent and chat runs render the session transcript.
	const compareOptions = $derived(
		filtered
			.filter(
				(execution) =>
					execution.id !== selectedId &&
					(execution.kind === undefined || execution.kind === 'workflow'),
			)
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
			header: t('page.executions.columnExecution'),
			text: (execution) => execution.id,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'workflow',
			header: t('page.executions.columnWorkflow'),
			text: (execution) => execution.workflowName,
		},
		{ key: 'status', header: t('page.executions.columnStatus'), cell: executionStatusCell },
		{
			key: 'started',
			header: t('page.executions.columnStarted'),
			text: (execution) => formatDateTime(execution.startedAt),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'tasks',
			header: t('page.executions.columnTasks'),
			align: 'right',
			text: (execution) => `${execution.tasksDone}/${execution.tasksTotal}`,
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
	];
</script>

<SplitView
	inspectorTitle={t('page.executionDetail.titleFallback')}
	inspectorOpen={selectedId !== null}
	dual={compareMode}
	secondaryTitle={t('page.executions.compare')}
	secondaryOpen={compareMode && compareId !== null}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title={t('page.executions.title')}
			description={t('page.executions.description')}
		>
			{#snippet actions()}
				<IconButton
					icon="refresh"
					label={t('common.refresh')}
					onclick={() => void reload()}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => (view = view === 'list' ? 'table' : 'list')}
				>
					<Icon name={view === 'list' ? 'blocks' : 'menu'} size={13} />
					{view === 'list' ? t('page.executions.table') : t('page.executions.cards')}
				</Button>
				<Button
					variant="outline"
					size="sm"
					active={compareMode}
					onclick={toggleCompare}
				>
					<Icon name="copy" size={13} />
					{t('page.executions.compare')}
				</Button>
				<Button size="sm" href="/workflows">
					<Icon name="play" size={13} />
					{t('page.executions.startFromWorkflow')}
				</Button>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<MetricGrid metrics={overviewMetrics} class="mb-3" />

			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder={t('page.executions.filterPlaceholder')}
				class="mb-3"
				onstatuschange={() => void reload()}
			>
				{#snippet trailing()}
					<Select
						value={engine}
						options={[
							{ value: 'all', label: 'All engines' },
							{ value: 'workflow', label: 'Workflow' },
							{ value: 'agent', label: 'Agent' },
							{ value: 'chat', label: 'Chat' },
						]}
						size="sm"
						class="w-36"
						onchange={(value) => {
							engine =
								value === 'agent' || value === 'workflow' || value === 'chat'
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
						placeholder={t('page.executions.selectBPlaceholder')}
						class="w-64"
						onchange={(value) => (compareId = value || null)}
					/>
					<IconButton
						icon="arrow-right"
						label={t('page.executions.swap')}
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
					title={t('page.executions.loadFailed')}
					description={error}
					onretry={() => void reload()}
				/>
			{:else if filtered.length === 0}
				<EmptyState
					icon="activity"
					tone="brand"
					title={t('page.executions.noMatch')}
					description={t('page.executions.noMatchDesc')}
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
		{#if selectedKind === 'agent' || selectedKind === 'chat'}
			{#if selectedId}
				<SessionInspector sessionId={selectedId} />
			{/if}
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
								t('page.executions.detailFailed'),
								e instanceof Error ? e.message : undefined,
							);
						});
				}}
			/>
		{/if}
	{/snippet}

	{#snippet secondary()}
		{#if compared}
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
								t('page.executions.compareDetailFailed'),
								e instanceof Error ? e.message : undefined,
							);
						});
				}}
			/>
		{:else}
			<EmptyState
				icon="copy"
				title={t('page.executions.selectB')}
				description={t('page.executions.selectBDesc')}
				class="m-3 rounded-lg border border-border bg-card"
			/>
		{/if}
	{/snippet}
</SplitView>

{#snippet executionStatusCell(execution: Execution)}
	<StatusBadge status={execution.status} size="sm" />
{/snippet}
