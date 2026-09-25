<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
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
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import {
		listWorkflows,
		getWorkflow,
		importWorkflow,
	} from '$lib/services/workflows';
	import type { WorkflowDetail } from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const STATUS_OPTIONS = [
		{ value: 'active', label: 'Active' },
		{ value: 'draft', label: 'Draft' },
		{ value: 'archived', label: 'Archived' },
	];

	const initial = parseListParams(page.url);

	let query = $state(initial.q ?? '');
	let status = $state(initial.status ?? '');
	let selectedId = $state<string | null>(initial.id ?? null);
	let graphNodeId = $state<string | null>(null);

	const list = createCollection((params) => listWorkflows(params));
	const detail = createResource<WorkflowDetail | null>(async () => {
		if (!selectedId) return null;
		return getWorkflow(selectedId);
	});

	const filtered = $derived(
		list.items.filter((workflow) => {
			const matchesStatus = !status || workflow.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				workflow.name.toLowerCase().includes(needle) ||
				workflow.tags.some((tag) => tag.toLowerCase().includes(needle));
			return matchesStatus && matchesQuery;
		}),
	);

	$effect(() => {
		if (selectedId) void detail.reload();
	});

	$effect(() => {
		if (!selectedId && list.loaded > 0 && !list.loading) {
			selectedId = list.items[0].id;
		}
	});

	$effect(() => {
		gotoWithParams(page.url, {
			q: query,
			status,
			id: selectedId ?? '',
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => void list.loadPages(Number(initial.page) || 1));

	let importOpen = $state(false);
	let importJson = $state('');
	let importing = $state(false);

	const importValid = $derived(importJson.trim().startsWith('{'));

	async function submitImport(): Promise<void> {
		importing = true;
		try {
			const newId = await importWorkflow(importJson);
			importJson = '';
			importOpen = false;
			toasts.success(`Imported workflow ${newId.slice(0, 12)}`);
			await list.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Import failed');
		} finally {
			importing = false;
		}
	}
</script>

<SplitView
	inspectorTitle="Workflow detail"
	inspectorOpen={selectedId !== null}
	oninspectorclose={() => (selectedId = null)}
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
					onclick={() => {
						list.reload();
						if (selectedId) detail.reload();
					}}
				/>
				<Button size="sm" onclick={() => (importOpen = true)}>
					<Icon name="upload" size={13} />
					Import
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

			{#if list.loading && list.loaded === 0}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each Array.from({ length: 6 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="104px" class="rounded-lg" />
					{/each}
				</div>
			{:else if list.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load workflows"
					description={list.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => list.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
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
				<Skeleton shape="block" height="140px" class="rounded-lg" />
				<Skeleton lines={4} />
			</div>
		{:else if detail.error}
			<EmptyState
				icon="alert-triangle"
				title="Failed to load detail"
				description={detail.error}
				class="m-4"
			>
				{#snippet actions()}
					<Button variant="link" size="sm" onclick={() => detail.reload()}
						>Retry</Button
					>
				{/snippet}
			</EmptyState>
		{:else if detail.data}
			{@const selected = detail.data}
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
							<Badge variant="outline" size="sm">{tag}</Badge>
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
	description="Paste a full workflow definition; the server parses, validates and stores it as a new formal workflow."
>
	<Textarea
		bind:value={importJson}
		rows={12}
		class="font-mono text-caption"
		placeholder={'{ "id": "…", "name": "…", "nodes": [], "edges": [] }'}
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}
			>Cancel</Button
		>
		<Button
			size="sm"
			disabled={importing || !importValid}
			onclick={() => void submitImport()}
		>
			Import
		</Button>
	{/snippet}
</Dialog>
