<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import GraphExplorer, {
		type GraphOverlay,
	} from '$lib/components/domain/GraphExplorer.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import {
		diffWorkflowVersions,
		executeWorkflow,
		exportWorkflow,
		getWorkflowDetail,
		type VersionDiff,
	} from '$lib/services/workflows';
	import {
		getGraphAnalysis,
		getGraphNeighbors,
		promoteWorkflowDraft,
		rollbackWorkflow,
		validateWorkflowDraft,
	} from '$lib/services/graph';
	import { listExecutions } from '$lib/services/executions';
	import type {
		Execution,
		GraphAnalysisResult,
		WorkflowDetail,
		WorkflowVersion,
	} from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime, formatNumber } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';

	const TABS = [
		{ id: 'graph', label: 'Graph' },
		{ id: 'versions', label: 'Versions' },
		{ id: 'drafts', label: 'Drafts' },
		{ id: 'runs', label: 'Runs' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'graph',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'graph' ? '' : tab });
	});

	let detail = $state<WorkflowDetail | null>(null);
	let detailError = $state<string | null>(null);
	let detailLoading = $state(true);

	let graphNodeId = $state<string | null>(null);
	let analysis = $state<GraphAnalysisResult | null>(null);
	let analysisError = $state<string | null>(null);
	let activeOverlay = $state<string | null>(null);

	let compareFrom = $state('');
	let compareTo = $state('');
	let diff = $state<VersionDiff | null>(null);
	let diffError = $state<string | null>(null);
	let diffLoading = $state(false);
	let rollbackTarget = $state('');
	let rollbackArmed = $state(false);
	let rollbackBusy = $state(false);

	let runs = $state<Execution[]>([]);
	let runsError = $state<string | null>(null);
	let runsLoading = $state(false);

	const workflow = $derived(detail);

	const nodes = $derived<DisplayNode[]>(
		(detail?.graph.nodes ?? []).map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
		})),
	);

	const edges = $derived<DisplayEdge[]>(
		(detail?.graph.edges ?? []).map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
		})),
	);

	const overlays = $derived.by<GraphOverlay[]>(() => {
		if (!analysis) return [];
		const list: GraphOverlay[] = [];
		if (analysis.cycleDetection.hasCycle) {
			list.push({
				id: 'cycles',
				label: 'Cycle nodes',
				ids: analysis.cycleDetection.cycleNodes,
			});
		}
		if (analysis.reachability.unreachableNodes.length > 0) {
			list.push({
				id: 'unreachable',
				label: 'Unreachable',
				ids: analysis.reachability.unreachableNodes,
			});
		}
		if (analysis.reachability.deadEndNodes.length > 0) {
			list.push({
				id: 'dead-ends',
				label: 'Dead ends',
				ids: analysis.reachability.deadEndNodes,
			});
		}
		return list;
	});

	async function load(id: string): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getWorkflowDetail(id);
			graphNodeId = null;
			activeOverlay = null;
			if (detail.versions.length > 0) {
				compareFrom = detail.versions[detail.versions.length - 1].version;
				compareTo = detail.versions[0].version;
				rollbackTarget = detail.versions[0].version;
			}
			void loadAnalysis(id);
		} catch (e) {
			detailError =
				e instanceof Error ? e.message : 'Failed to load workflow.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	async function loadAnalysis(id: string): Promise<void> {
		analysis = null;
		analysisError = null;
		try {
			analysis = await getGraphAnalysis(id);
		} catch (e) {
			analysisError =
				e instanceof Error ? e.message : 'Analysis failed to load.';
		}
	}

	async function loadRuns(id: string): Promise<void> {
		runsLoading = true;
		runsError = null;
		try {
			const result = await listExecutions({ workflowId: id, limit: 50 });
			runs = result.items;
		} catch (e) {
			runsError = e instanceof Error ? e.message : 'Runs failed to load.';
		} finally {
			runsLoading = false;
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		const workflowId = page.params.id;
		if (!workflowId) return;
		try {
			const neighbors = await getGraphNeighbors(workflowId, id);
			neighborhoodIds = [id, ...neighbors.predecessors, ...neighbors.successors];
			activeOverlay = '__neighborhood';
			toasts.success(
				`Neighborhood: ${neighbors.predecessors.length} in · ${neighbors.successors.length} out`,
			);
		} catch (e) {
			toasts.error(
				'Neighborhood failed to load',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	let neighborhoodIds = $state<string[]>([]);

	const effectiveOverlays = $derived.by<GraphOverlay[]>(() => {
		if (activeOverlay === '__neighborhood') {
			return [
				...overlays,
				{ id: '__neighborhood', label: 'Neighborhood', ids: neighborhoodIds },
			];
		}
		return overlays;
	});

	async function runCompare(id: string): Promise<void> {
		if (!compareFrom || !compareTo) return;
		diffLoading = true;
		diffError = null;
		try {
			diff = await diffWorkflowVersions(id, compareFrom, compareTo);
		} catch (e) {
			diffError = e instanceof Error ? e.message : 'Compare failed.';
			diff = null;
		} finally {
			diffLoading = false;
		}
	}

	async function runRollback(id: string): Promise<void> {
		if (!rollbackTarget) return;
		rollbackBusy = true;
		try {
			await rollbackWorkflow(id, rollbackTarget);
			rollbackArmed = false;
			toasts.success(`Rolled back to ${rollbackTarget}`);
			await load(id);
		} catch (e) {
			toasts.error(
				'Rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			rollbackBusy = false;
		}
	}

	async function runPromote(draftId: string): Promise<void> {
		try {
			const report = await promoteWorkflowDraft(draftId);
			toasts.success(
				`Draft promoted (${report.passCount} passed, ${report.warningCount} warnings)`,
			);
			const id = page.params.id;
			if (id) await load(id);
		} catch (e) {
			toasts.error(
				'Promotion failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runValidate(draftId: string): Promise<void> {
		try {
			const issues = await validateWorkflowDraft(draftId);
			if (detail) {
				detail = {
					...detail,
					drafts: detail.drafts.map((draft) =>
						draft.id === draftId
							? {
									...draft,
									valid: issues.length === 0,
									issues: issues.map(
										(issue) => `${issue.field}: ${issue.message}`,
									),
								}
							: draft,
					),
				};
			}
			toasts.success(
				issues.length === 0
					? 'Draft is valid'
					: `Draft has ${issues.length} issue(s)`,
			);
		} catch (e) {
			toasts.error(
				'Validation failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runExport(id: string): Promise<void> {
		try {
			await exportWorkflow(id);
			toasts.success('Workflow definition exported');
		} catch (e) {
			toasts.error(
				'Export failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runExecute(id: string): Promise<void> {
		try {
			const executionId = await executeWorkflow(id);
			toasts.success('Execution started');
			await goto(resolve('/executions/[id]', { id: executionId }));
		} catch (e) {
			toasts.error(
				'Execution failed to start',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	onMount(() => {
		const id = page.params.id;
		if (!id) return;
		void load(id);
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (detail && detail.id === id) {
			if (tab === 'runs' && runs.length === 0 && !runsLoading && !runsError) {
				void loadRuns(id);
			}
			return;
		}
		void load(id);
	});

	$effect(() => {
		if (tab === 'runs') {
			const id = page.params.id;
			if (id && detail && runs.length === 0 && !runsLoading && !runsError) {
				void loadRuns(id);
			}
		}
	});

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
		(detail?.versions ?? []).map((row) => ({
			value: row.version,
			label: row.version,
		})),
	);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={workflow?.name ?? 'Workflow detail'}
		description={workflow?.description ?? ''}
	>
		{#snippet meta()}
			{#if workflow}
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
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pencil"
				label="Definition editing is not available in this release"
				disabled
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => {
					const id = page.params.id;
					if (id) void runExport(id);
				}}
			>
				<Icon name="download" size={13} />
				Export
			</Button>
			<Button
				size="sm"
				onclick={() => {
					const id = page.params.id;
					if (id) void runExecute(id);
				}}
			>
				<Icon name="play" size={13} />
				Run
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if detailLoading}
			<Skeleton lines={6} class="rounded-lg border border-border bg-card p-4" />
		{:else if detailError || !detail}
			<ErrorState
				title="Workflow failed to load"
				description={detailError ?? 'Unknown error.'}
				onretry={() => {
					const id = page.params.id;
					if (id) void load(id);
				}}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'graph'}
			{#if analysisError}
				<p class="mb-2 text-caption text-warning">
					Graph analysis unavailable ({analysisError});
					<button
						type="button"
						class="underline"
						onclick={() => {
							const id = page.params.id;
							if (id) void loadAnalysis(id);
						}}
					>
						retry
					</button>.
				</p>
			{/if}
			<GraphExplorer
				{nodes}
				{edges}
				preset="workflow"
				selectedId={graphNodeId}
				onselect={(id) => (graphNodeId = id)}
				onexpand={(id) => void expandNeighborhood(id)}
				expandLabel="Reveal neighborhood"
				overlays={effectiveOverlays}
				{activeOverlay}
				onoverlay={(id) => (activeOverlay = id)}
			/>
			<div class="mt-3 grid gap-3 lg:grid-cols-2">
				<Card title="Nodes">
					<ul class="space-y-1.5">
						{#each detail.graph.nodes as node (node.id)}
							<li class="flex items-center justify-between gap-2 text-caption">
								<button
									type="button"
									class="truncate font-mono underline-offset-2 hover:underline"
									onclick={() => (graphNodeId = node.id)}
								>
									{node.id}
								</button>
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
							onclick={() => {
								const id = page.params.id;
								if (id) void runCompare(id);
							}}
						>
							<Icon name="copy" size={13} />
							Compare
						</Button>
					</div>
					{#if diffError}
						<p class="mt-2 text-caption text-destructive">{diffError}</p>
					{:else if diff}
						<ul class="mt-2 space-y-1 text-caption">
							<li>Added nodes: {diff.addedNodes.join(', ') || '—'}</li>
							<li>Removed nodes: {diff.removedNodes.join(', ') || '—'}</li>
							<li>Added edges: {diff.addedEdges.join(', ') || '—'}</li>
							<li>Removed edges: {diff.removedEdges.join(', ') || '—'}</li>
						</ul>
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
								onclick={() => {
									const id = page.params.id;
									if (id) void runRollback(id);
								}}
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
								<Button size="sm" onclick={() => void runPromote(draft.id)}>
									Promote
								</Button>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => void runValidate(draft.id)}
								>
									Validate
								</Button>
							</div>
						{/snippet}
					</Card>
				{:else}
					<EmptyState
						icon="file"
						title="No drafts"
						description="Editable drafts for this workspace appear here."
						class="rounded-lg border border-border bg-card"
					/>
				{/each}
			</div>
		{:else}
			{#if runsLoading}
				<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
			{:else if runsError}
				<ErrorState
					title="Runs failed to load"
					description={runsError}
					onretry={() => {
						const id = page.params.id;
						if (id) void loadRuns(id);
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if runs.length === 0}
				<EmptyState
					icon="activity"
					title="No runs yet"
					description="Executions of this workflow appear here once it runs."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<Card title="Runs" bodyClass="p-0">
					<ul class="divide-y divide-border">
						{#each runs as run (run.id)}
							<li class="flex items-center justify-between gap-2 px-3 py-2 text-caption">
								<a
									href={resolve('/executions/[id]', { id: run.id })}
									class="truncate font-mono underline-offset-2 hover:underline"
								>
									{run.id}
								</a>
								<span class="flex shrink-0 items-center gap-2">
									<StatusBadge status={run.status} size="sm" dot={false} />
									<span class="text-muted-foreground">
										{formatDateTime(run.startedAt)}
									</span>
								</span>
							</li>
						{/each}
					</ul>
				</Card>
			{/if}
		{/if}
	</div>
</div>
