<script lang="ts">
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import { workflowDetail, workflows } from '$lib/fixtures/workflows';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime, formatNumber } from '$lib/utils/format';

	const TABS = [
		{ id: 'graph', label: 'Graph' },
		{ id: 'versions', label: 'Versions' },
		{ id: 'drafts', label: 'Drafts' },
		{ id: 'runs', label: 'Runs' },
	];

	let tab = $state('graph');
	let graphNodeId = $state<string | null>(null);

	const workflow = $derived(
		workflows.find((item) => item.id === page.params.id) ?? workflowDetail,
	);
	const detail = $derived(workflowDetail);

	const versionColumns: Column<(typeof detail.versions)[number]>[] = [
		{ key: 'version', header: 'Version', text: (row) => `v${row.version}` },
		{ key: 'note', header: 'Note', text: (row) => row.note },
		{ key: 'author', header: 'Author', text: (row) => row.author },
		{
			key: 'created',
			header: 'Created',
			text: (row) => formatDateTime(row.createdAt),
		},
		{
			key: 'current',
			header: 'State',
			text: (row) => (row.current ? 'current' : 'superseded'),
		},
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={workflow.name} description={workflow.description}>
		{#snippet meta()}
			<StatusBadge status={workflow.status} />
			<Badge variant="outline">v{workflow.version}</Badge>
			<span class="font-mono text-caption text-muted-foreground"
				>{workflow.id}</span
			>
			<span class="text-caption text-muted-foreground">
				{formatNumber(workflow.nodeCount)} nodes · {formatNumber(
					workflow.edgeCount,
				)} edges
			</span>
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pencil"
				label="Edit definition"
				onclick={() => toasts.info('Editor pending')}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.success('Export prepared')}
			>
				<Icon name="download" size={13} />
				Export
			</Button>
			<Button size="sm" onclick={() => toasts.success('Run request prepared')}>
				<Icon name="play" size={13} />
				Run
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'graph'}
			<WorkflowGraph
				graph={detail.graph}
				selectedId={graphNodeId}
				onselect={(id) => (graphNodeId = id)}
				class="max-h-[26rem]"
			/>
			<div class="mt-3 grid gap-3 lg:grid-cols-2">
				<Card title="Nodes">
					<ul class="space-y-1.5">
						{#each detail.graph.nodes as node (node.id)}
							<li class="flex items-center justify-between gap-2 text-caption">
								<span class="truncate font-mono">{node.id}</span>
								<span class="flex shrink-0 items-center gap-2">
									<span class="text-muted-foreground">{node.kind}</span>
									<StatusBadge status={node.status} size="sm" dot={false} />
								</span>
							</li>
						{/each}
					</ul>
				</Card>
				<Card title="Edges">
					<ul class="space-y-1.5">
						{#each detail.graph.edges as edge (edge.id)}
							<li class="flex items-center gap-2 text-caption">
								<span class="font-mono">{edge.from}</span>
								<Icon
									name="arrow-right"
									size={12}
									class="text-muted-foreground"
								/>
								<span class="font-mono">{edge.to}</span>
								{#if edge.label}
									<Badge variant="outline" class="text-[0.625rem]"
										>{edge.label}</Badge
									>
								{/if}
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		{:else if tab === 'versions'}
			<Card title="Version history" bodyClass="p-0">
				<DataTable
					columns={versionColumns}
					rows={detail.versions}
					rowKey={(row) => String(row.version)}
				/>
			</Card>
			<div class="mt-3 flex gap-2">
				<Button
					variant="outline"
					size="sm"
					onclick={() => toasts.info('Compare view pending')}
				>
					<Icon name="copy" size={13} />
					Compare
				</Button>
				<Button
					variant="outline"
					size="sm"
					onclick={() => toasts.warning('Rollback requires confirmation')}
				>
					<Icon name="history" size={13} />
					Rollback
				</Button>
			</div>
		{:else if tab === 'drafts'}
			<div class="space-y-2">
				{#each detail.drafts as draft (draft.id)}
					<Card title={draft.name}>
						{#snippet actions()}
							<StatusBadge
								status={draft.valid ? 'completed' : 'failed'}
								size="sm"
							/>
						{/snippet}
						<p class="text-caption text-muted-foreground">
							Updated {formatDateTime(draft.updatedAt)}
						</p>
						{#if draft.issues.length > 0}
							<ul class="mt-2 space-y-1">
								{#each draft.issues as issue, index (index)}
									<li
										class="flex items-start gap-1.5 text-caption text-destructive"
									>
										<Icon
											name="alert-circle"
											size={12}
											class="mt-0.5 shrink-0"
										/>
										<span>{issue}</span>
									</li>
								{/each}
							</ul>
						{/if}
						{#snippet footer()}
							<div class="flex items-center gap-2">
								<Button
									size="sm"
									disabled={!draft.valid}
									onclick={() => toasts.success('Draft promoted')}
								>
									Promote
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toasts.info('Validation queued')}
								>
									Validate
								</Button>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{:else}
			<EmptyState
				icon="activity"
				title="No runs loaded"
				description="Execution history for this definition appears once the API stage is connected."
				class="rounded-lg border border-border bg-card"
			/>
		{/if}
	</div>
</div>
