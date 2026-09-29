<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import WorkflowCard from '$lib/components/domain/WorkflowCard.svelte';
	import GraphCanvas from '$lib/components/domain/GraphCanvas.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import {
		createMinimalWorkflow,
		importWorkflow,
		listWorkflows,
		getWorkflowDetail,
	} from '$lib/services/workflows';
	import { getGraphNeighbors } from '$lib/services/graph';
	import type { Workflow, WorkflowDetail } from '$lib/types/models';
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

	let query = $state('');
	let status = $state('');
	let selectedId = $state<string | null>(null);
	let graphNodeId = $state<string | null>(null);
	let neighborhood = $state<{
		predecessors: string[];
		successors: string[];
	} | null>(null);
	let neighborhoodLoading = $state(false);
	let neighborhoodError = $state<string | null>(null);
	let neighborhoodRequest = 0;
	let allWorkflows = $state<Workflow[]>([]);
	let selected = $state<WorkflowDetail | null>(null);
	let listError = $state<string | null>(null);
	let detailError = $state<string | null>(null);

	let importOpen = $state(false);
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);
	let newOpen = $state(false);
	let newName = $state('');
	let newBusy = $state(false);

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		listError = null;
		try {
			const page = await listWorkflows({ limit: 200 });
			allWorkflows = page.items;
			if (page.items.length > 0 && !selectedId) {
				selectedId = page.items[0].id;
			}
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Workflows failed.';
			allWorkflows = [];
		}
	}

	async function runImport(): Promise<void> {
		importError = null;
		try {
			JSON.parse(importText);
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		importBusy = true;
		try {
			const id = await importWorkflow(importText);
			toasts.success('Workflow imported');
			importOpen = false;
			importText = '';
			await reload();
			await goto(resolve('/workflows/[id]', { id }));
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Import failed.';
		} finally {
			importBusy = false;
		}
	}

	async function runCreate(): Promise<void> {
		newBusy = true;
		try {
			const workflow = await createMinimalWorkflow(newName);
			toasts.success('Workflow created');
			newOpen = false;
			newName = '';
			await goto(resolve('/workflows/[id]', { id: workflow.id }));
		} catch (e) {
			toasts.error('Create failed', e instanceof Error ? e.message : undefined);
		} finally {
			newBusy = false;
		}
	}

	$effect(() => {
		const id = selectedId;
		graphNodeId = null;
		neighborhood = null;
		neighborhoodError = null;
		neighborhoodLoading = false;
		if (!id) {
			selected = null;
			return;
		}
		detailError = null;
		void getWorkflowDetail(id)
			.then((row) => {
				selected = row;
			})
			.catch((e) => {
				selected = null;
				detailError = e instanceof Error ? e.message : 'Detail failed.';
			});
	});

	async function loadNeighborhood(
		workflowId: string,
		nodeId: string,
	): Promise<void> {
		const request = ++neighborhoodRequest;
		neighborhoodLoading = true;
		neighborhoodError = null;
		try {
			const rows = await getGraphNeighbors(workflowId, nodeId);
			if (request !== neighborhoodRequest) return;
			neighborhood = rows;
		} catch (e) {
			if (request !== neighborhoodRequest) return;
			neighborhood = null;
			neighborhoodError =
				e instanceof Error ? e.message : 'Neighborhood failed.';
		} finally {
			if (request === neighborhoodRequest) neighborhoodLoading = false;
		}
	}

	$effect(() => {
		const workflowId = selected?.id;
		const nodeId = graphNodeId;
		if (!workflowId || !nodeId) return;
		void loadNeighborhood(workflowId, nodeId);
	});

	const filtered = $derived(
		allWorkflows.filter((workflow) => {
			const matchesStatus = !status || workflow.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				workflow.name.toLowerCase().includes(needle) ||
				workflow.tags.some((tag) => tag.toLowerCase().includes(needle));
			return matchesStatus && matchesQuery;
		}),
	);
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
					onclick={() => void reload()}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => {
						importText = '';
						importError = null;
						importOpen = true;
					}}
				>
					<Icon name="upload" size={13} />
					Import
				</Button>
				<Button
					size="sm"
					onclick={() => {
						newName = '';
						newOpen = true;
					}}
				>
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

			{#if listError}
				<ErrorState
					title="Workflows failed to load"
					description={listError}
					onretry={() => void reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if filtered.length === 0}
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
		{#if detailError && !selected}
			<ErrorState
				title="Detail failed to load"
				description={detailError}
				onretry={() => {
					if (selectedId) {
						detailError = null;
						void getWorkflowDetail(selectedId)
							.then((row) => {
								selected = row;
							})
							.catch((e) => {
								detailError = e instanceof Error ? e.message : 'Detail failed.';
							});
					}
				}}
				class="m-3 rounded-lg border border-border bg-card"
			/>
		{:else if selected}
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
						<GraphCanvas
							nodes={(selected.graph.nodes ?? []).map((node) => ({
								id: node.id,
								label: node.label,
								kind: node.kind,
								status: node.status,
							}))}
							edges={(selected.graph.edges ?? []).map((edge) => ({
								id: edge.id,
								source: edge.from,
								target: edge.to,
								label: edge.label,
								kind: edge.kind,
							}))}
							preset="workflow"
							layout="layered"
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
						{#if !graphNodeId}
							<p class="text-caption text-muted-foreground">
								Select a node in the graph to see its neighborhood.
							</p>
						{:else if neighborhoodLoading}
							<p class="text-caption text-muted-foreground">
								Loading neighborhood…
							</p>
						{:else if neighborhoodError}
							<p class="text-caption text-destructive">{neighborhoodError}</p>
							<Button
								variant="ghost"
								size="sm"
								class="mt-2"
								onclick={() => {
									const workflowId = selected?.id;
									if (workflowId && graphNodeId) {
										void loadNeighborhood(workflowId, graphNodeId);
									}
								}}
							>
								Retry
							</Button>
						{:else if neighborhood}
							<div class="space-y-2">
								<div>
									<p
										class="text-micro uppercase tracking-wide text-muted-foreground"
									>
										Predecessors · {neighborhood.predecessors.length}
									</p>
									<ul class="mt-1 space-y-1">
										{#each neighborhood.predecessors as id (id)}
											<li class="truncate font-mono text-caption">{id}</li>
										{:else}
											<li class="text-caption text-muted-foreground">—</li>
										{/each}
									</ul>
								</div>
								<div>
									<p
										class="text-micro uppercase tracking-wide text-muted-foreground"
									>
										Successors · {neighborhood.successors.length}
									</p>
									<ul class="mt-1 space-y-1">
										{#each neighborhood.successors as id (id)}
											<li class="truncate font-mono text-caption">{id}</li>
										{:else}
											<li class="text-caption text-muted-foreground">—</li>
										{/each}
									</ul>
								</div>
							</div>
						{/if}
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

<Dialog
	bind:open={importOpen}
	title="Import workflow"
	description="Paste a workflow definition as JSON."
>
	<Textarea
		bind:value={importText}
		placeholder={'{\n  "id": "my-workflow",\n  …\n}'}
		class="min-h-48 font-mono text-small"
	/>
	{#if importError}
		<p
			class="mt-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive"
		>
			{importError}
		</p>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}>
				Cancel
			</Button>
			<Button size="sm" disabled={importBusy} onclick={() => void runImport()}>
				{importBusy ? 'Importing…' : 'Import'}
			</Button>
		</div>
	{/snippet}
</Dialog>

<Dialog
	bind:open={newOpen}
	title="New workflow"
	description="A minimal start → end definition to extend in the detail view."
>
	<Input bind:value={newName} placeholder="Workflow name" />
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (newOpen = false)}>
				Cancel
			</Button>
			<Button size="sm" disabled={newBusy} onclick={() => void runCreate()}>
				{newBusy ? 'Creating…' : 'Create'}
			</Button>
		</div>
	{/snippet}
</Dialog>
