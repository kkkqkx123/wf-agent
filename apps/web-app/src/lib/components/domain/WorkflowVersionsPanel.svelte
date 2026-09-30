<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import type { Column } from '@wf-agent/ui/components/table';
	import {
		diffWorkflowVersions,
		type VersionDiff,
	} from '$lib/services/workflows';
	import { rollbackWorkflow } from '$lib/services/graph';
	import { buildVersionDiffView } from '$lib/graph/execution-projection';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { WorkflowVersion } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';

	interface Props {
		workflowId: string;
		versions: WorkflowVersion[];
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		onchanged: () => void;
	}

	let { workflowId, versions, nodes, edges, onchanged }: Props = $props();

	let selectedId = $state<string | null>(null);
	let compareFrom = $state('');
	let compareTo = $state('');
	let diff = $state<VersionDiff | null>(null);
	let diffError = $state<string | null>(null);
	let diffLoading = $state(false);
	let rollbackTarget = $state('');
	let rollbackArmed = $state(false);
	let rollbackBusy = $state(false);
	let diffExplorer = $state<{ focus: (id: string) => void } | null>(null);

	const versionColumns: Column<WorkflowVersion>[] = [
		{ key: 'version', header: 'Version', text: (row) => row.version },
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

	const versionOptions = $derived(
		versions.map((row) => ({ value: row.version, label: row.version })),
	);

	// Edge-level diff view: single derivation feeds both the text rows and
	// the graph, so counts and colors always agree.
	const diffView = $derived(buildVersionDiffView(nodes, edges, diff));
	const diffGraphNodes = $derived<DisplayNode[]>(diffView.nodes);
	const diffGraphEdges = $derived<DisplayEdge[]>(diffView.edges);
	const diffIsEmpty = $derived(diff !== null && diffView.empty);

	$effect(() => {
		if (versions.length > 0) {
			if (!compareFrom) compareFrom = versions[versions.length - 1].version;
			if (!compareTo) compareTo = versions[0].version;
			if (!rollbackTarget) rollbackTarget = versions[0].version;
		}
	});

	async function runCompare(): Promise<void> {
		if (!compareFrom || !compareTo) return;
		diffLoading = true;
		diffError = null;
		try {
			diff = await diffWorkflowVersions(workflowId, compareFrom, compareTo);
		} catch (e) {
			diffError = e instanceof Error ? e.message : 'Compare failed.';
			diff = null;
		} finally {
			diffLoading = false;
		}
	}

	async function runRollback(): Promise<void> {
		if (!rollbackTarget) return;
		rollbackBusy = true;
		try {
			await rollbackWorkflow(workflowId, rollbackTarget);
			rollbackArmed = false;
			toasts.success(`Rolled back to ${rollbackTarget}`);
			onchanged();
		} catch (e) {
			toasts.error(
				'Rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			rollbackBusy = false;
		}
	}
</script>

<Card title="Version history" bodyClass="p-0">
	<DataTable
		columns={versionColumns}
		rows={versions}
		rowKey={(row) => row.version}
	/>
</Card>
<div class="mt-3 grid gap-3 lg:grid-cols-2">
	<Card title="Compare versions">
		<div class="flex flex-wrap items-end gap-2">
			<Select
				bind:value={compareFrom}
				options={versionOptions}
				size="sm"
				placeholder="From"
				class="w-32"
			/>
			<Select
				bind:value={compareTo}
				options={versionOptions}
				size="sm"
				placeholder="To"
				class="w-32"
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={!compareFrom || !compareTo || diffLoading}
				onclick={() => void runCompare()}
			>
				<Icon name="copy" size={13} />
				Compare
			</Button>
		</div>
		{#if diffError}
			<p class="mt-2 text-caption text-destructive">{diffError}</p>
		{:else if diff}
			{#if diffIsEmpty}
				<p class="mt-2 text-caption text-muted-foreground">
					No differences between {compareFrom} and {compareTo}.
				</p>
			{:else}
				<ul class="mt-2 space-y-1 text-caption">
					<li>
						Added nodes ({diffView.addedNodes}):
						{#each diff.addedNodes as nodeId (nodeId)}
							<button
								type="button"
								aria-current={selectedId === nodeId}
								class="mr-1 font-mono text-success underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-success"
								onclick={() => diffExplorer?.focus(nodeId)}
							>
								{nodeId}
							</button>
						{:else}—{/each}
					</li>
					<li>
						Removed nodes ({diffView.removedNodes}):
						{#each diff.removedNodes as nodeId (nodeId)}
							<button
								type="button"
								aria-current={selectedId === nodeId}
								class="mr-1 font-mono text-destructive underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-destructive"
								onclick={() => diffExplorer?.focus(nodeId)}
							>
								{nodeId}
							</button>
						{:else}—{/each}
					</li>
					<li>
						Added edges ({diffView.addedEdges}):
						{#each diff.addedEdges as edge (`${edge.source}->${edge.target}`)}
							<button
								type="button"
								aria-current={selectedId === edge.source ||
									selectedId === edge.target}
								class="mr-1 font-mono text-success underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-success"
								onclick={() => diffExplorer?.focus(edge.source)}
							>
								{edge.source} → {edge.target}
							</button>
						{:else}—{/each}
					</li>
					<li>
						Removed edges ({diffView.removedEdges}):
						{#each diff.removedEdges as edge (`${edge.source}->${edge.target}`)}
							<button
								type="button"
								aria-current={selectedId === edge.source ||
									selectedId === edge.target}
								class="mr-1 font-mono text-destructive underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-destructive"
								onclick={() => diffExplorer?.focus(edge.source)}
							>
								{edge.source} → {edge.target}
							</button>
						{:else}—{/each}
					</li>
				</ul>
			{/if}
		{/if}
	</Card>
	<Card title="Rollback">
		<div class="flex flex-wrap items-end gap-2">
			<Select
				bind:value={rollbackTarget}
				options={versionOptions}
				size="sm"
				placeholder="Version"
				class="w-32"
			/>
			{#if rollbackArmed}
				<Button
					size="sm"
					disabled={rollbackBusy || !rollbackTarget}
					onclick={() => void runRollback()}
				>
					Confirm rollback to {rollbackTarget}
				</Button>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => (rollbackArmed = false)}
				>
					Cancel
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					disabled={!rollbackTarget}
					onclick={() => (rollbackArmed = true)}
				>
					<Icon name="history" size={13} />
					Rollback
				</Button>
			{/if}
		</div>
	</Card>
</div>
{#if diff && !diffLoading}
	<div class="mt-3">
		{#if diffIsEmpty}
			<p class="mb-2 text-caption text-muted-foreground">
				Graph matches the text: no added or removed nodes or edges.
			</p>
		{/if}
		<GraphExplorer
			bind:this={diffExplorer}
			nodes={diffGraphNodes}
			edges={diffGraphEdges}
			preset="workflow"
			{selectedId}
			onselect={(id) => (selectedId = id)}
			overlays={[
				{ id: 'added', label: 'Added', ids: diff.addedNodes },
				{ id: 'removed', label: 'Removed', ids: diff.removedNodes },
			]}
		/>
	</div>
{/if}
