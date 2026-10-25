<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import Textarea from '#lib/ui/components/Textarea.svelte';
	import DataTable from '#lib/ui/components/DataTable.svelte';
	import type { Column } from '#lib/ui/components/table';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import {
		exportQuery,
		getQueryResult,
		listErrorAnalyses,
		listInsightAuditReports,
		listPerformanceNodes,
		runQuery,
	} from '#lib/services/insights';
	import type { AuditReport, ErrorAnalysis, PerfNode, QueryResult } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
		formatRelativeTime,
	} from '#lib/utils/format';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';

	const TABS = [
		{ id: 'query', label: 'Query' },
		{ id: 'audit', label: 'Audit' },
		{ id: 'errors', label: 'Errors' },
		{ id: 'performance', label: 'Performance' },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab) ? requestedTab : 'query',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, {
			tab: tab === 'query' ? '' : tab,
		});
	});
	let statement = $state(
		"SELECT execution_id, workflow, status, duration_ms\nFROM executions\nWHERE status != 'completed'\nORDER BY duration_ms DESC\nLIMIT 50;",
	);
	let queryResult = $state<QueryResult>({
		columns: [],
		rows: [],
		elapsedMs: 0,
		truncated: false,
	});
	let queryBusy = $state(false);
	let queryError = $state<string | null>(null);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let auditReports = $state<AuditReport[]>([]);
	let errorAnalyses = $state<ErrorAnalysis[]>([]);
	let perfNodes = $state<PerfNode[]>([]);

	/** Segment sources already pulled, so a tab loads once. */
	let seenQuery = $state(false);
	let seenAudit = $state(false);
	let seenErrors = $state(false);
	let seenPerf = $state(false);

	onMount(() => {
		void loadTab(tab);
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		const pending =
			current === 'query'
				? !seenQuery
				: current === 'audit'
					? !seenAudit
					: current === 'errors'
						? !seenErrors
						: !seenPerf;
		if (!pending) return;
		loading = true;
		loadError = null;
		try {
			if (current === 'query') {
				seenQuery = true;
				queryResult = await getQueryResult();
			} else if (current === 'audit') {
				seenAudit = true;
				auditReports = await listInsightAuditReports();
			} else if (current === 'errors') {
				seenErrors = true;
				errorAnalyses = await listErrorAnalyses();
			} else {
				seenPerf = true;
				perfNodes = await listPerformanceNodes();
			}
		} catch (e) {
			console.error('Failed to load insights segment:', e);
			loadError = e instanceof Error ? e.message : t('page.insights.loadFailed');
		} finally {
			loading = false;
		}
	}

	async function reload(): Promise<void> {
		seenQuery = false;
		seenAudit = false;
		seenErrors = false;
		seenPerf = false;
		await loadTab(tab);
	}

	async function runAdhocQuery(): Promise<void> {
		queryBusy = true;
		queryError = null;
		try {
			queryResult = await runQuery({ limit: 50 });
			seenQuery = true;
			toasts.success(t('page.insights.defaultScopeExecuted'));
		} catch (e) {
			queryError = e instanceof Error ? e.message : t('page.insights.queryFailed');
			toasts.error(t('page.insights.queryFailedTitle'), e instanceof Error ? e.message : undefined);
		} finally {
			queryBusy = false;
		}
	}

	async function exportAdhocQuery(): Promise<void> {
		try {
			await exportQuery({ expressions: [], format: 'csv', limit: 50 });
			toasts.success(t('page.insights.exportQueued'));
		} catch (e) {
			toasts.error(t('page.insights.exportFailed'), e instanceof Error ? e.message : undefined);
		}
	}

	const rowColumns = $derived<Column<Record<string, string | number | null>>[]>(
		queryResult.columns.map((column) => ({
			key: column,
			header: column,
			text: (row) => (row[column] === null ? '—' : String(row[column])),
		})),
	);

	const auditColumns: Column<AuditReport>[] = [
		{
			key: 'execution',
			header: t('page.insights.columnExecution'),
			text: (row) => row.executionId,
		},
		{
			key: 'status',
			header: t('page.insights.columnStatus'),
			text: (row) => row.status,
		},
		{
			key: 'nodes',
			header: t('page.insights.columnNodes'),
			align: 'right',
			text: (row) => formatNumber(row.nodes),
		},
		{
			key: 'duration',
			header: t('page.insights.columnDuration'),
			align: 'right',
			text: (row) => formatDuration(row.durationMs),
		},
		{
			key: 'tools',
			header: t('page.insights.columnToolCalls'),
			align: 'right',
			text: (row) => formatNumber(row.toolCalls),
		},
		{
			key: 'llm',
			header: t('page.insights.columnLlmCalls'),
			align: 'right',
			text: (row) => formatNumber(row.llmCalls),
		},
		{
			key: 'generated',
			header: t('page.insights.columnGenerated'),
			text: (row) => formatRelativeTime(row.generatedAt),
		},
	];

	const errorColumns: Column<ErrorAnalysis>[] = [
		{
			key: 'category',
			header: t('page.insights.columnCategory'),
			text: (row) => row.category,
		},
		{
			key: 'rootCause',
			header: t('page.insights.columnRootCause'),
			text: (row) => row.rootCause,
		},
		{
			key: 'count',
			header: t('page.insights.columnCount'),
			align: 'right',
			text: (row) => formatNumber(row.occurrences),
		},
		{
			key: 'first',
			header: t('page.insights.columnFirstSeen'),
			text: (row) => formatDateTime(row.firstSeen),
		},
		{
			key: 'last',
			header: t('page.insights.columnLastSeen'),
			text: (row) => formatRelativeTime(row.lastSeen),
		},
		{
			key: 'status',
			header: t('page.insights.columnState'),
			text: (row) => row.status,
		},
	];

	function emptyCopyFor(current: string): {
		title: string;
		description: string;
	} {
		if (current === 'audit') {
			return {
				title: t('page.insights.noAuditReports'),
				description: t('page.insights.noAuditReportsDesc'),
			};
		}
		if (current === 'errors') {
			return {
				title: t('page.insights.noErrorAnalysis'),
				description: t('page.insights.noErrorAnalysisDesc'),
			};
		}
		return {
			title: t('page.insights.noPerformanceData'),
			description: t('page.insights.noPerformanceDataDesc'),
		};
	}

	const segmentEmpty = $derived(
		tab === 'audit'
			? auditReports.length === 0
			: tab === 'errors'
				? errorAnalyses.length === 0
				: perfNodes.length === 0,
	);

	const segmentCopy = $derived(emptyCopyFor(tab));
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.insights.title')} description={t('page.insights.description')}>
		{#snippet actions()}
			<IconButton icon="refresh" label={t('common.refresh')} onclick={() => void reload()} />
			<Button variant="outline" size="sm" onclick={() => void exportAdhocQuery()}>
				<Icon name="download" size={13} />
				Export
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" panelId="insights-panel" />

	<div
		id="insights-panel"
		role="tabpanel"
		aria-label={t('page.insights.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if tab === 'query'}
			<div class="space-y-3">
				<Card title={t('page.insights.adhocQuery')}>
					<Textarea bind:value={statement} class="min-h-28 font-mono text-caption" />
					<p class="mt-1 text-micro text-muted-foreground">
						The statement box is a local draft. The query endpoint takes structured filters, so Run
						executes the default execution scope and reports that scope only until statement
						execution lands.
					</p>
					<div class="mt-2 flex items-center gap-2">
						<Button size="sm" disabled={queryBusy} onclick={() => void runAdhocQuery()}>
							<Icon name="play" size={13} />
							{queryBusy ? t('page.insights.running') : t('page.insights.runQuery')}
						</Button>
						<Button variant="ghost" size="sm" onclick={() => (statement = '')}
							>{t('page.insights.clear')}</Button
						>
						<span class="ml-auto text-micro tabular-nums text-muted-foreground">
							{queryResult.elapsedMs} ms
							{queryResult.truncated ? '· truncated' : ''}
						</span>
					</div>
				</Card>

				<Card
					title={t('page.insights.result')}
					description="{formatNumber(queryResult.rows.length)} rows"
					bodyClass="p-0"
				>
					<PageState
						loading={queryBusy}
						error={queryError}
						empty={queryResult.rows.length === 0}
						emptyTitle={t('page.insights.noRows')}
						emptyDescription={t('page.insights.runDefaultScope')}
					>
						<DataTable
							columns={rowColumns}
							rows={queryResult.rows}
							rowKey={(row) =>
								String(row.execution_id ?? row.id ?? JSON.stringify(row).slice(0, 48))}
							dense
						/>
					</PageState>
				</Card>
			</div>
		{:else}
			<PageState
				{loading}
				error={loadError}
				empty={segmentEmpty}
				emptyTitle={segmentCopy.title}
				emptyDescription={segmentCopy.description}
				onretry={() => void reload()}
			>
				{#if tab === 'audit'}
					<Card title={t('page.insights.auditReports')} bodyClass="p-0">
						<DataTable
							columns={auditColumns}
							rows={auditReports}
							rowKey={(row) => row.id}
							emptyDescription={t('page.insights.auditNoAggregator')}
						/>
					</Card>
					<div class="mt-3 flex flex-wrap gap-2">
						{#each auditReports.slice(0, 3) as report (report.id)}
							<Card class="min-w-56 flex-1" title={report.executionId}>
								<div class="flex items-center justify-between gap-2">
									<StatusBadge status={report.status} size="sm" />
									<span class="text-micro text-muted-foreground">
										{formatRelativeTime(report.generatedAt)}
									</span>
								</div>
								<p class="mt-2 text-caption text-muted-foreground">
									{formatNumber(report.nodes)} nodes · {formatNumber(report.toolCalls)} tool calls
								</p>
								{#snippet footer()}
									<Button variant="ghost" size="sm" href="/insights"
										>{t('page.insights.openReport')}</Button
									>
								{/snippet}
							</Card>
						{/each}
					</div>
				{:else if tab === 'errors'}
					<Card title={t('page.insights.errorAnalysis')} bodyClass="p-0">
						<DataTable
							columns={errorColumns}
							rows={errorAnalyses}
							rowKey={(row) => row.id}
							emptyDescription={t('page.insights.errorNoAggregator')}
						/>
					</Card>
					<div class="mt-3 grid gap-3 lg:grid-cols-2">
						{#each errorAnalyses.slice(0, 2) as error (error.id)}
							<Card title={error.category}>
								{#snippet actions()}
									<StatusBadge status={error.status} size="sm" />
								{/snippet}
								<p class="text-caption">{error.rootCause}</p>
								<div class="mt-2 flex flex-wrap items-center gap-1.5">
									{#if error.similar.length > 0}
										<span class="text-micro text-muted-foreground">similar:</span>
										{#each error.similar as id (id)}
											<Badge variant="outline" class="text-[0.625rem]">{id}</Badge>
										{/each}
									{:else}
										<span class="text-micro text-muted-foreground">no similar errors</span>
									{/if}
								</div>
							</Card>
						{/each}
					</div>
				{:else}
					<Card title={t('page.insights.nodePerformance')}>
						<ul class="space-y-3">
							{#each perfNodes as node (node.node)}
								<li>
									<div class="flex items-center justify-between gap-3 text-caption">
										<span class="truncate font-mono">{node.node}</span>
										<span class="shrink-0 tabular-nums text-muted-foreground">
											{formatDuration(node.avgMs)} avg · {formatDuration(node.p95Ms)} p95
										</span>
									</div>
									<div class="mt-1 flex items-center gap-2">
										<span class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
											<span
												class="block h-full rounded-full bg-chart-1"
												style:width="{node.share * 100}%"
											></span>
										</span>
										<span
											class="w-10 shrink-0 text-right text-micro tabular-nums text-muted-foreground"
										>
											{Math.round(node.share * 100)}%
										</span>
									</div>
								</li>
							{/each}
						</ul>
					</Card>
				{/if}
			</PageState>
		{/if}
	</div>
</div>
