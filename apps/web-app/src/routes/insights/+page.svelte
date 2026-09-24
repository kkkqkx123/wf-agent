<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		auditReports,
		errorAnalyses,
		perfNodes,
		queryResult,
	} from '$lib/fixtures/insights';
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
	let statement = $state(
		"SELECT execution_id, workflow, status, duration_ms\nFROM executions\nWHERE status != 'completed'\nORDER BY duration_ms DESC\nLIMIT 50;",
	);

	const rowColumns: Column<Record<string, string | number | null>>[] =
		queryResult.columns.map((column) => ({
			key: column,
			header: column,
			text: (row) => (row[column] === null ? '—' : String(row[column])),
		}));

	const auditColumns: Column<(typeof auditReports)[number]>[] = [
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

	const errorColumns: Column<(typeof errorAnalyses)[number]>[] = [
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
				onclick={() => toasts.info('Refresh queued')}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.success('Export queued')}
			>
				<Icon name="download" size={13} />
				Export
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
						<Button size="sm" onclick={() => toasts.success('Query executed')}>
							<Icon name="play" size={13} />
							Run query
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
			<Card title="Audit reports" bodyClass="p-0">
				<DataTable
					columns={auditColumns}
					rows={auditReports}
					rowKey={(row) => row.id}
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
			<Card title="Error analysis" bodyClass="p-0">
				<DataTable
					columns={errorColumns}
					rows={errorAnalyses}
					rowKey={(row) => row.id}
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
								<span class="text-micro text-muted-foreground"
									>no similar errors</span
								>
							{/if}
						</div>
					</Card>
				{/each}
			</div>
		{:else}
			<Card title="Node performance">
				<ul class="space-y-3">
					{#each perfNodes as node (node.node)}
						<li>
							<div class="flex items-center justify-between gap-3 text-caption">
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
	</div>
</div>
