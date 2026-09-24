<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import WorkflowCard from '$lib/components/domain/WorkflowCard.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import type { PageData } from './$types';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '$lib/utils/format';

	const STATUS_OPTIONS = [
		{ value: 'active', label: 'Active' },
		{ value: 'draft', label: 'Draft' },
		{ value: 'archived', label: 'Archived' },
	];

	let { data }: { data: PageData } = $props();
	let { workflows, workflowDetail } = $derived(data);

	let query = $state('');
	let status = $state('');
	let selectedId = $state<string | null>(workflows[0]?.id ?? null);
	let graphNodeId = $state<string | null>(null);

	const filtered = $derived(
		workflows.filter((workflow) => {
			const matchesStatus = !status || workflow.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				workflow.name.toLowerCase().includes(needle) ||
				workflow.tags.some((tag) => tag.toLowerCase().includes(needle));
			return matchesStatus && matchesQuery;
		}),
	);

	const selected = $derived(workflowDetail);
</script>

<SplitView
	inspectorTitle="Workflow detail"
	inspectorOpen={selectedId !== null}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title="Workflows"
			description="Definitions, versions and drafts across the orchestration layer."
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
					onclick={() => toasts.info('Import dialog pending')}
				>
					<Icon name="upload" size={13} />
					Import
				</Button>
				<Button size="sm" onclick={() => toasts.success('Draft created')}>
					<Icon name="plus" size={13} />
					New
				</Button>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder="Filter by name or tag…"
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
					icon="workflow"
					title="No workflows match"
					description="Clear the filters to browse every definition."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each filtered as workflow (workflow.id)}
						<WorkflowCard
							{workflow}
							selected={selectedId === workflow.id}
							onselect={(item) => (selectedId = item.id)}
						/>
					{/each}
				</div>
			{/if}
		</div>
	</div>

	{#snippet inspector()}
		{#if selected}
			<div class="flex h-full min-h-0 flex-col">
				<div class="border-b border-border px-3 py-3">
					<div class="flex items-start justify-between gap-2">
						<div class="min-w-0">
							<h2 class="truncate text-title font-semibold">{selected.name}</h2>
							<p class="mt-0.5 font-mono text-micro text-muted-foreground">
								{selected.id}
							</p>
						</div>
						<StatusBadge status={selected.status} />
					</div>
					<p class="mt-2 text-caption text-muted-foreground">
						{selected.description}
					</p>
					<div class="mt-2 flex flex-wrap items-center gap-1.5">
						{#each selected.tags as tag (tag)}
							<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
						{/each}
					</div>
				</div>

				<div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
					<Card title="Summary">
						<KeyValueList
							items={[
								{ key: 'version', value: `v${selected.version}` },
								{ key: 'category', value: selected.category },
								{ key: 'author', value: selected.author },
								{ key: 'nodes', value: formatNumber(selected.nodeCount) },
								{ key: 'edges', value: formatNumber(selected.edgeCount) },
								{ key: 'runs', value: formatNumber(selected.runs) },
								{
									key: 'success',
									value:
										selected.successRate === null
											? '—'
											: formatPercent(selected.successRate),
								},
								{
									key: 'updated',
									value: formatRelativeTime(selected.updatedAt),
								},
							]}
							dense
						/>
					</Card>

					<div>
						<h3 class="mb-1.5 text-caption font-medium">Graph</h3>
						<WorkflowGraph
							graph={selected.graph}
							selectedId={graphNodeId}
							onselect={(id) => (graphNodeId = id)}
							class="max-h-56"
						/>
						{#if graphNodeId}
							<p class="mt-1.5 text-caption text-muted-foreground">
								Selected node <span class="font-mono text-foreground"
									>{graphNodeId}</span
								>
							</p>
						{/if}
					</div>

					<Card title="Neighbors">
						<ul class="space-y-1.5">
							{#each selected.neighbors as neighbor (neighbor.id)}
								<li
									class="flex items-center justify-between gap-2 text-caption"
								>
									<span class="truncate">{neighbor.label}</span>
									<span
										class={neighbor.reachable
											? 'text-success'
											: 'text-muted-foreground'}
									>
										{neighbor.reachable ? 'reachable' : 'unreachable'}
									</span>
								</li>
							{/each}
						</ul>
					</Card>

					<Button
						variant="outline"
						size="sm"
						href="/workflows/{selected.id}"
						class="w-full"
					>
						<Icon name="arrow-right" size={13} />
						Open full detail
					</Button>
				</div>
			</div>
		{/if}
	{/snippet}
</SplitView>
