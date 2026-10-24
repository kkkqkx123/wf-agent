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
	import CreateTaskPanel from '#lib/components/domain/CreateTaskPanel.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import { onDestroy, onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		listExecutions,
		listUnifiedExecutions,
		getExecutionDetail,
		getExecutionStats,
	} from '#lib/services/executions';
	import { rerunWorkflowRun } from '#lib/services/run';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import SessionInspector from '#lib/components/domain/SessionInspector.svelte';
	import type { Execution, ExecutionDetail, Metric } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';
	import { watchListInvalidation } from '#lib/services/list-invalidation';
	import type { StreamState } from '#lib/api/sse';

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
	let versionFilter = $state('');
	let timeRange = $state<'all' | 'today' | 'last7' | 'last30'>('all');
	let autoInsert = $state(false);
	let insertedIds = $state<Set<string>>(new Set());
	let view = $state<'list' | 'table'>('list');
	// Deep links restore the selection; otherwise the first row is picked on
	// load. The effect below writes the selection back to `?id=`.
	let selectedId = $state<string | null>(
		parseListParams(page.url as unknown as URL).id ?? null,
	);
	let compareMode = $state(false);
	let showTaskPanel = $state(false);
	let compareId = $state<string | null>(null);
	let compareDetail = $state<ExecutionDetail | null>(null);
	let rerunningId = $state<string | null>(null);
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
	let hasUpdates = $state(false);
	let listLiveState = $state<StreamState>('closed');
	let stopListLive: (() => void) | null = null;

	const EXECUTIONS_PAGE = 200;

	onMount(() => {
		void reload();
		stopListLive = watchListInvalidation({
			onDirty: () => {
				if (loading || loadingMore) {
					hasUpdates = true;
					return;
				}
				if (typeof document !== 'undefined' && document.hidden) {
					hasUpdates = true;
					return;
				}
				if (autoInsert) {
					void insertLatest();
					return;
				}
				void reload();
			},
			onState: (state) => {
				listLiveState = state;
			},
		});
	});

	onDestroy(() => {
		stopListLive?.();
		stopListLive = null;
	});

	async function reload(): Promise<void> {
		loading = true;
		error = null;
		hasUpdates = false;
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
			if (page.items.length > 0) {
				const ids = new Set(page.items.map((item) => item.id));
				if (!selectedId || !ids.has(selectedId)) {
					selectedId = page.items[0].id;
				}
			} else {
				selectedId = null;
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

	/** Optional incremental insert: prepend only unseen rows with a flash. */
	async function insertLatest(): Promise<void> {
		try {
			const page =
				engine === 'workflow'
					? await listExecutions({ limit: 50, status: status || undefined })
					: await listUnifiedExecutions({
							limit: 50,
							status: status || undefined,
							executionType: engine === 'all' ? undefined : engine,
						});
			const known = new Set(allExecutions.items.map((item) => item.id));
			const fresh = page.items.filter((item) => !known.has(item.id));
			if (fresh.length === 0) return;
			allExecutions = {
				items: [...fresh, ...allExecutions.items],
				hasMore: allExecutions.hasMore,
			};
			insertedIds = new Set(fresh.map((item) => item.id));
			setTimeout(() => {
				insertedIds = new Set();
			}, 3000);
		} catch {
			hasUpdates = true;
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
	// Version and time range narrow client-side: the list endpoint has no
	// dedicated fields for them, so they refine what is already loaded.
	const filtered = $derived(
		allExecutions.items.filter((execution) => {
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				execution.workflowName.toLowerCase().includes(needle) ||
				execution.id.toLowerCase().includes(needle);
			const versionNeedle = versionFilter.trim().toLowerCase();
			const matchesVersion =
				!versionNeedle ||
				(execution.version ?? '').toLowerCase().includes(versionNeedle);
			const matchesTime = (() => {
				if (timeRange === 'all') return true;
				const at = Date.parse(execution.startedAt);
				if (!Number.isFinite(at)) return true;
				const ageMs = Date.now() - at;
				const dayMs = 24 * 60 * 60 * 1000;
				if (timeRange === 'today') return ageMs <= dayMs;
				if (timeRange === 'last7') return ageMs <= 7 * dayMs;
				return ageMs <= 30 * dayMs;
			})();
			return matchesQuery && matchesVersion && matchesTime;
		}),
	);

	const activeFilterCount = $derived(
		(status ? 1 : 0) +
			(versionFilter.trim() ? 1 : 0) +
			(timeRange !== 'all' ? 1 : 0) +
			(engine !== 'all' ? 1 : 0) +
			(query.trim() ? 1 : 0),
	);

	// Keep the selection linkable: list state survives reloads and shares.
	$effect(() => {
		void gotoWithParams(page.url as unknown as URL, { id: selectedId ?? '' });
	});

	function focusCard(id: string): void {
		document
			.querySelector(`[data-execution-id="${id}"]`)
			?.scrollIntoView({ block: 'nearest' });
	}

	function moveSelection(delta: number): void {
		if (filtered.length === 0) return;
		const ids = filtered.map((execution) => execution.id);
		const index = selectedId ? ids.indexOf(selectedId) : -1;
		const next = ids[(index + delta + ids.length) % ids.length] ?? null;
		if (next) {
			selectedId = next;
			focusCard(next);
		}
	}

	function handleListKeys(event: KeyboardEvent): void {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			moveSelection(1);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			moveSelection(-1);
		} else if (event.key === 'Enter' && selectedId) {
			event.preventDefault();
			void goto(resolve('/executions/[id]', { id: selectedId }));
		}
	}

	function openExecution(execution: Execution): void {
		void goto(resolve('/executions/[id]', { id: execution.id }));
	}

	async function rerunExecution(execution: Execution): Promise<void> {
		if (rerunningId) return;
		rerunningId = execution.id;
		try {
			const id = await rerunWorkflowRun(execution.id);
			toasts.success(t('domain.runs.rerunStarted'), id);
			await reload();
		} catch (e) {
			toasts.error(
				t('domain.runs.rerunFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			rerunningId = null;
		}
	}

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
				<Button
					size="sm"
					active={showTaskPanel}
					onclick={() => (showTaskPanel = !showTaskPanel)}
				>
					<Icon name="play" size={13} />
					{t('page.executions.newTask')}
				</Button>
				<Button size="sm" href="/workflows">
					<Icon name="blocks" size={13} />
					{t('page.executions.startFromWorkflow')}
				</Button>
			{/snippet}
		</PageHeader>

		<div class="page-container min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			{#if hasUpdates}
				<div
					class="mb-3 flex items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2"
				>
					<span class="text-caption text-muted-foreground">
						New execution activity arrived{listLiveState === 'reconnecting'
							? ' (reconnecting)'
							: ''}.
					</span>
					<span class="flex items-center gap-2">
						<label
							class="flex cursor-pointer items-center gap-1.5 text-caption text-muted-foreground"
						>
							<input
								type="checkbox"
								bind:checked={autoInsert}
								class="h-3.5 w-3.5 accent-current"
							/>
							Auto-insert
						</label>
						<Button size="sm" variant="outline" onclick={() => void reload()}>
							Refresh
						</Button>
					</span>
				</div>
			{/if}
			{#if showTaskPanel}
				<div class="mb-3">
					<CreateTaskPanel
						onstarted={(executionId) => {
							showTaskPanel = false;
							void goto(resolve('/executions/[id]', { id: executionId }));
						}}
					/>
				</div>
			{/if}
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
					<Input
						bind:value={versionFilter}
						placeholder="Version"
						class="w-28"
					/>
					<Select
						value={timeRange}
						options={[
							{ value: 'all', label: 'All time' },
							{ value: 'today', label: 'Today' },
							{ value: 'last7', label: 'Last 7 days' },
							{ value: 'last30', label: 'Last 30 days' },
						]}
						size="sm"
						class="w-36"
						onchange={(value) => {
							timeRange =
								value === 'today' || value === 'last7' || value === 'last30'
									? value
									: 'all';
						}}
					/>
					<Select
						value={engine}
						options={[
							{ value: 'all', label: t('page.executions.engineAll') },
							{ value: 'workflow', label: t('page.executions.engineWorkflow') },
							{ value: 'agent', label: t('page.executions.engineAgent') },
							{ value: 'chat', label: t('page.executions.engineChat') },
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
					<span class="text-caption tabular-nums text-muted-foreground">
						{activeFilterCount > 0
							? `${activeFilterCount} filter${activeFilterCount > 1 ? 's' : ''} · ${filtered.length} shown`
							: `${filtered.length} shown`}
					</span>
					<label
						class="flex cursor-pointer items-center gap-1.5 text-caption text-muted-foreground"
						title="Prepend new rows with a highlight instead of showing a refresh banner"
					>
						<input
							type="checkbox"
							bind:checked={autoInsert}
							class="h-3.5 w-3.5 accent-current"
						/>
						Auto-insert
					</label>
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
				<div
					class="grid gap-2 md:grid-cols-2 xl:grid-cols-3"
					onkeydown={handleListKeys}
					role="presentation"
				>
					{#each filtered as execution (execution.id)}
						<ExecutionCard
							{execution}
							selected={selectedId === execution.id}
							class={insertedIds.has(execution.id)
								? 'animate-row-in border-success/50'
								: ''}
							onselect={(item) => (selectedId = item.id)}
							onopen={openExecution}
							onrerun={(item) => void rerunExecution(item)}
							rerunning={rerunningId === execution.id}
						/>
					{/each}
					{#if loadingMore}
						<Skeleton shape="block" height="72px" class="rounded-lg" />
						<Skeleton shape="block" height="72px" class="rounded-lg" />
					{/if}
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
