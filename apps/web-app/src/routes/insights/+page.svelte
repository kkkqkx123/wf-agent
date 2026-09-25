<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import type { Column } from '$lib/components/ui/table';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		runQuery,
		listAuditReports,
		listErrorAnalyses,
		listPerformanceNodes,
		exportQuery,
	} from '$lib/services/insights';
	import type {
		QueryResult,
		AuditReport,
		ErrorAnalysis,
	} from '$lib/types/models';
	import { createResource } from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
		formatRelativeTime,
	} from '$lib/utils/format';

	const TABS = [
		{ id: 'query', label: 'Query' },
		{ id: 'audit', label: 'Audit' },
		{ id: 'errors', label: 'Errors' },
		{ id: 'performance', label: 'Performance' },
	];

	let tab = $state('query');
	let statement = $state('');
	let exporting = $state(false);

	// Query tab state
	let queryLoading = $state(false);
	let queryError = $state<string | null>(null);
	let queryResult = $state<QueryResult>({
		columns: [],
		rows: [],
		elapsedMs: 0,
		truncated: false,
	});

	const audit = createResource(() => listAuditReports());
	const errors = createResource(() => listErrorAnalyses());
	const perf = createResource(() => listPerformanceNodes());

	const auditReports = $derived(audit.data ?? []);
	const errorAnalyses = $derived(errors.data ?? []);
	const perfNodes = $derived(perf.data ?? []);

	const rowColumns = $derived<Column<Record<string, string | number | null>>[]>(
		queryResult.columns.map((column) => ({
			key: column,
			header: column,
			text: (row) => (row[column] === null ? '—' : String(row[column])),
		})),
	);

	const auditColumns: Column<AuditReport>[] = [
		{ key: 'execution', header: 'Execution', text: (row) => row.executionId },
		{ key: 'status', header: 'Status', text: (row) => row.status },
		{
			key: 'nodes',
			header: 'Nodes',
			align: 'right',
			text: (row) => formatNumber(row.nodes),
		},
		{
			key: 'duration',
			header: 'Duration',
			align: 'right',
			text: (row) => formatDuration(row.durationMs),
		},
		{
			key: 'tools',
			header: 'Tool calls',
			align: 'right',
			text: (row) => formatNumber(row.toolCalls),
		},
		{
			key: 'llm',
			header: 'LLM calls',
			align: 'right',
			text: (row) => formatNumber(row.llmCalls),
		},
		{
			key: 'generated',
			header: 'Generated',
			text: (row) => formatRelativeTime(row.generatedAt),
		},
	];

	const errorColumns: Column<ErrorAnalysis>[] = [
		{ key: 'category', header: 'Category', text: (row) => row.category },
		{ key: 'rootCause', header: 'Root cause', text: (row) => row.rootCause },
		{
			key: 'count',
			header: 'Count',
			align: 'right',
			text: (row) => formatNumber(row.occurrences),
		},
		{
			key: 'first',
			header: 'First seen',
			text: (row) => formatDateTime(row.firstSeen),
		},
		{
			key: 'last',
			header: 'Last seen',
			text: (row) => formatRelativeTime(row.lastSeen),
		},
		{ key: 'status', header: 'State', text: (row) => row.status },
	];

	async function executeQuery() {
		if (!statement.trim()) {
			toasts.warning('Please enter a query');
			return;
		}
		queryLoading = true;
		queryError = null;
		try {
			queryResult = await runQuery({
				expressions: [{ sql: statement.trim() }],
				limit: 50,
			});
		} catch (e) {
			queryError = e instanceof Error ? e.message : 'Query failed';
			toasts.error(queryError);
		} finally {
			queryLoading = false;
		}
	}

	async function runExport(): Promise<void> {
		exporting = true;
		try {
			await exportQuery({
				expressions: statement.trim() ? [{ sql: statement.trim() }] : [],
				format: 'csv',
			});
			toasts.success('Export downloaded');
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Export failed');
		} finally {
			exporting = false;
		}
	}

	onMount(() => {
		void audit.reload();
		void errors.reload();
		void perf.reload();
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Query & audit"
		description="Ad-hoc queries, execution audit reports, error analysis and performance breakdown."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => {
					audit.reload();
					errors.reload();
					perf.reload();
				}}
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={exporting}
				onclick={runExport}
			>
				<Icon name="download" size={13} />
				{exporting ? 'Exporting…' : 'Export'}
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'query'}
			<div class="space-y-3">
				<Card title="Ad-hoc query">
					<Textarea
						bind:value={statement}
						class="min-h-28 font-mono text-caption"
					/>
					<div class="mt-2 flex items-center gap-2">
						<Button size="sm" onclick={executeQuery} disabled={queryLoading}>
							<Icon name="play" size={13} />
							{queryLoading ? 'Running…' : 'Run query'}
						</Button>
						<Button variant="ghost" size="sm" onclick={() => (statement = '')}
							>Clear</Button
						>
						<span class="ml-auto text-micro tabular-nums text-muted-foreground">
							{queryResult.elapsedMs} ms
							{queryResult.truncated ? '· truncated' : ''}
						</span>
					</div>
				</Card>

				<Card
					title="Result"
					description="{formatNumber(queryResult.rows.length)} rows"
					bodyClass="p-0"
				>
					<DataTable
						columns={rowColumns}
						rows={queryResult.rows}
						rowKey={(row) => String(row.execution_id)}
						dense
					/>
				</Card>
			</div>
		{:else if tab === 'audit'}
			{#if audit.loading && !audit.data}
				<Skeleton shape="block" height="160px" class="rounded-lg" />
			{:else if audit.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load audit reports"
					description={audit.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => audit.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				<Card title="Audit reports" bodyClass="p-0">
					<DataTable
						columns={auditColumns}
						rows={auditReports}
						rowKey={(row) => row.id}
					/>
				</Card>
			{/if}
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
							{formatNumber(report.nodes)} nodes · {formatNumber(
								report.toolCalls,
							)} tool calls
						</p>
						{#snippet footer()}
							<Button variant="ghost" size="sm" href="/insights"
								>Open read-only report</Button
							>
						{/snippet}
					</Card>
				{/each}
			</div>
		{:else if tab === 'errors'}
			{#if errors.loading && !errors.data}
				<Skeleton shape="block" height="160px" class="rounded-lg" />
			{:else if errors.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load error analysis"
					description={errors.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => errors.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				<Card title="Error analysis" bodyClass="p-0">
					<DataTable
						columns={errorColumns}
						rows={errorAnalyses}
						rowKey={(row) => row.id}
					/>
				</Card>
			{/if}
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
									<Badge variant="outline" size="sm">{id}</Badge>
								{/each}
							{:else}
								<span class="text-micro text-muted-foreground"
									>no similar errors</span
								>
							{/if}
						</div>
					</Card>
				{/each}
			</div>
		{:else}
			{#if perf.loading && !perf.data}
				<Skeleton shape="block" height="160px" class="rounded-lg" />
			{:else if perf.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load performance"
					description={perf.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => perf.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else if perfNodes.length === 0}
				<EmptyState
					icon="chart"
					title="No performance data"
					description="Node stats appear once executions are analysed."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<Card title="Node performance">
					<ul class="space-y-3">
						{#each perfNodes as node (node.node)}
							<li>
								<div
									class="flex items-center justify-between gap-3 text-caption"
								>
									<span class="truncate font-mono">{node.node}</span>
									<span class="shrink-0 tabular-nums text-muted-foreground">
										{formatDuration(node.avgMs)} avg · {formatDuration(
											node.p95Ms,
										)} p95
									</span>
								</div>
								<div class="mt-1 flex items-center gap-2">
									<span
										class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
									>
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
		{/if}
	</div>
</div>
