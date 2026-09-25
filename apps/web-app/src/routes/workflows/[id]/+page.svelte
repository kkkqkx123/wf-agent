<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import Timeline from '$lib/components/domain/Timeline.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import {
		getWorkflow,
		listWorkflowDrafts,
		validateWorkflowDraft,
		promoteWorkflowDraft,
		rollbackWorkflow,
		exportWorkflow,
	} from '$lib/services/workflows';
	import { streamWorkflowExecution } from '$lib/services/streaming';
	import type { StreamNodeUpdate } from '$lib/services/streaming';
	import { listExecutions } from '$lib/services/executions';
	import type {
		Execution,
		TimelineEntry,
		WorkflowGraph as WorkflowGraphModel,
		WorkflowVersion,
	} from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
		formatPercent,
		nodeCount,
	} from '$lib/utils/format';
	import { appPath } from '$lib/utils/route';

	const TABS = [
		{ id: 'graph', label: 'Graph' },
		{ id: 'versions', label: 'Versions' },
		{ id: 'draft', label: 'Draft' },
		{ id: 'runs', label: 'Runs' },
	];

	const id = $derived(page.params.id as string);

	let tab = $state('graph');
	let graphNodeId = $state<string | null>(null);
	let selectedVersion = $state<number | null>(null);
	let busy = $state(false);

	/** Live view of the run started from this page, fed by the execute stream. */
	let nodeStatus = $state<Record<string, string>>({});
	let runTimeline = $state<TimelineEntry[]>([]);
	let runExecutionId = $state('');
	let runError = $state<string | null>(null);
	let runRetryAfterMs = $state<number | null>(null);
	let runStopped = $state(false);
	let running = $state(false);
	let stopRun: (() => void) | null = null;

	function appendNodeEvent(node: StreamNodeUpdate): void {
		nodeStatus = { ...nodeStatus, [node.id]: node.status };
		runTimeline = [
			...runTimeline,
			{
				id: `${node.id}#${runTimeline.length}`,
				at: node.at,
				kind: 'node',
				title: node.name,
				detail:
					node.error ??
					(node.durationMs === null ? '' : formatDuration(node.durationMs)),
				status: node.status,
			},
		];
	}

	function withLiveStatus(graph: WorkflowGraphModel): WorkflowGraphModel {
		if (Object.keys(nodeStatus).length === 0) return graph;
		return {
			...graph,
			nodes: graph.nodes.map((node) => ({
				...node,
				status: nodeStatus[node.id] ?? node.status,
			})),
		};
	}

	const detail = createResource(() => getWorkflow(id));
	// A draft is stored under the id of the workflow it edits, so there is at most one.
	const draft = createResource(async () => {
		const drafts = await listWorkflowDrafts();
		return drafts.find((item) => item.id === id) ?? null;
	});
	const runs = createCollection((params) =>
		listExecutions({ ...params, workflowId: id }),
	);

	/** Validation output for the draft, or null while nothing has been checked. */
	let draftIssues = $state<string[] | null>(null);

	const selectedVersionEntry = $derived(
		(detail.data?.versions ?? []).find(
			(version) => version.version === selectedVersion,
		) ?? null,
	);

	const versionColumns: Column<WorkflowVersion>[] = [
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

	const runColumns: Column<Execution>[] = [
		{
			key: 'id',
			header: 'Execution',
			text: (row) => row.id.slice(0, 12),
		},
		{ key: 'status', header: 'Status', text: (row) => row.status },
		{
			key: 'started',
			header: 'Started',
			text: (row) => formatDateTime(row.startedAt),
		},
		{
			key: 'duration',
			header: 'Duration',
			align: 'right',
			text: (row) =>
				row.durationMs === null ? '—' : formatDuration(row.durationMs),
		},
		{
			key: 'nodes',
			header: 'Nodes',
			align: 'right',
			text: (row) => nodeCount(row.nodesDone, row.nodesTotal) ?? '—',
		},
	];

	function reloadAll(): void {
		void detail.reload();
		void draft.reload();
		void runs.reload();
	}

	onMount(reloadAll);

	async function run(): Promise<void> {
		tab = 'graph';
		nodeStatus = {};
		runTimeline = [];
		runError = null;
		runRetryAfterMs = null;
		runStopped = false;
		runExecutionId = '';
		running = true;
		const controller = new AbortController();
		stopRun = () => controller.abort();
		let terminal: 'completed' | 'failed' | null = null;
		await streamWorkflowExecution(
			id,
			{ input: null },
			{
				onExecution: (executionId) => (runExecutionId = executionId),
				onNode: appendNodeEvent,
				onCompleted: () => (terminal = 'completed'),
				onFailed: (message) => {
					terminal = 'failed';
					runError = message;
				},
				onInterrupted: (reason) => {
					terminal = 'failed';
					runError = reason;
				},
				onError: (failure) => {
					terminal = 'failed';
					runError = failure.message;
					runRetryAfterMs = failure.retryAfterMs;
				},
			},
			controller.signal,
		);
		const outcome = terminal;
		if (outcome) {
			// Bus forwarding drops overflow, so the outcome frame is what
			// settles a node whose completion never arrived.
			nodeStatus = Object.fromEntries(
				Object.entries(nodeStatus).map(([nodeId, status]) =>
					status === 'running' ? [nodeId, outcome] : [nodeId, status],
				),
			);
		}
		stopRun = null;
		runStopped = controller.signal.aborted;
		running = false;
		void detail.reload();
		void runs.reload();
	}

	async function download(): Promise<void> {
		busy = true;
		try {
			await exportWorkflow(id);
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Export failed');
		} finally {
			busy = false;
		}
	}

	async function checkDraft(): Promise<void> {
		busy = true;
		try {
			draftIssues = await validateWorkflowDraft(id);
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Validation failed');
		} finally {
			busy = false;
		}
	}

	/** Both actions replace the formal definition, so they are confirm-gated. */
	let target = $state<
		{ kind: 'promote' } | { kind: 'rollback'; version: WorkflowVersion } | null
	>(null);
	let confirming = $state(false);

	function ask(
		action:
			{ kind: 'promote' } | { kind: 'rollback'; version: WorkflowVersion },
	): void {
		target = action;
		confirming = true;
	}

	const confirmCopy = $derived.by(() => {
		if (!target) return { title: '', detail: '' };
		return target.kind === 'promote'
			? {
					title: 'Promote draft',
					detail: `The draft replaces the formal definition of ${id} and is then removed.`,
				}
			: {
					title: `Roll back to v${target.version.version}`,
					detail: `${id} is repointed at version ${target.version.version}; the current definition stays in history.`,
				};
	});

	async function runConfirmed(): Promise<void> {
		const action = target;
		busy = true;
		try {
			if (action?.kind === 'promote') {
				await promoteWorkflowDraft(id);
				draftIssues = null;
				toasts.success(`Promoted draft of ${id}`);
			} else if (action?.kind === 'rollback') {
				await rollbackWorkflow(id, action.version.version);
				toasts.success(`Rolled back to v${action.version.version}`);
			}
			confirming = false;
			target = null;
			reloadAll();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={detail.data?.name ?? 'Workflow detail'}
		description={detail.data?.description ?? ''}
	>
		{#snippet meta()}
			{#if detail.data}
				<StatusBadge status={detail.data.status} />
				<Badge variant="outline">v{detail.data.version}</Badge>
				<span class="font-mono text-caption text-muted-foreground"
					>{detail.data.id}</span
				>
				<span class="text-caption text-muted-foreground">
					{formatNumber(detail.data.nodeCount)} nodes · {formatNumber(
						detail.data.edgeCount,
					)} edges
				</span>
				{#if detail.data.successRate !== null}
					<span class="text-caption text-muted-foreground">
						{formatPercent(detail.data.successRate)} success over
						{formatNumber(detail.data.runs)} runs
					</span>
				{/if}
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton icon="refresh" label="Refresh" onclick={reloadAll} />
			<Button
				variant="outline"
				size="sm"
				disabled={busy}
				onclick={() => void download()}
			>
				<Icon name="download" size={13} />
				Export
			</Button>
			{#if running}
				<Button variant="outline" size="sm" onclick={() => stopRun?.()}>
					<Icon name="square" size={13} />
					Stop
				</Button>
			{:else}
				<Button size="sm" onclick={() => void run()}>
					<Icon name="play" size={13} />
					Run
				</Button>
			{/if}
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if detail.loading && !detail.data}
			<div class="space-y-3">
				<Skeleton shape="block" height="180px" class="rounded-lg" />
				<Skeleton shape="block" height="120px" class="rounded-lg" />
			</div>
		{:else if detail.error}
			<ErrorState
				title="Failed to load workflow"
				description={detail.error}
				class="rounded-lg border border-border bg-card"
			>
				{#snippet actions()}
					<Button variant="link" size="sm" href="/workflows"
						>Back to workflows</Button
					>
				{/snippet}
			</ErrorState>
		{:else if detail.data}
			{@const safeDetail = detail.data}
			{#if tab === 'graph'}
				{@const liveGraph = withLiveStatus(safeDetail.graph)}
				<WorkflowGraph
					graph={liveGraph}
					selectedId={graphNodeId}
					onselect={(id) => (graphNodeId = id)}
					class="max-h-[26rem]"
				/>
				<div class="mt-3 grid gap-3 lg:grid-cols-2">
					<Card title="Nodes">
						<ul class="space-y-1.5">
							{#each liveGraph.nodes as node (node.id)}
								<li
									class="flex items-center justify-between gap-2 text-caption"
								>
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
							{#each safeDetail.graph.edges as edge (edge.id)}
								<li class="flex items-center gap-2 text-caption">
									<span class="font-mono">{edge.from}</span>
									<Icon
										name="arrow-right"
										size={12}
										class="text-muted-foreground"
									/>
									<span class="font-mono">{edge.to}</span>
									{#if edge.label}
										<Badge variant="outline" size="sm">{edge.label}</Badge>
									{/if}
								</li>
							{/each}
						</ul>
					</Card>
				</div>
				{#if running || runTimeline.length > 0}
					<Card title="Live run" class="mt-3">
						{#snippet actions()}
							{#if runExecutionId}
								<Button
									variant="link"
									size="sm"
									href={appPath(`/executions/${runExecutionId}`)}
								>
									Open execution
								</Button>
							{/if}
						{/snippet}
						{#if runError}
							<p class="text-caption text-destructive">
								{runError}
								{#if runRetryAfterMs !== null}
									<span class="mt-0.5 block text-muted-foreground">
										Rate limited — retry in
										{formatDuration(runRetryAfterMs)}.
									</span>
								{/if}
							</p>
						{:else if runStopped}
							<p class="text-caption text-muted-foreground">
								Stopped with {formatNumber(runTimeline.length)} node events kept.
							</p>
						{:else if runTimeline.length === 0}
							<p class="text-caption text-muted-foreground">
								Waiting for the first node event…
							</p>
						{/if}
						{#if runTimeline.length > 0}
							<Timeline entries={runTimeline} class="mt-3" />
						{/if}
					</Card>
				{/if}
			{:else if tab === 'versions'}
				<Card title="Version history" bodyClass="p-0">
					<DataTable
						columns={versionColumns}
						rows={safeDetail.versions}
						rowKey={(row) => String(row.version)}
						selectedKey={selectedVersion === null
							? null
							: String(selectedVersion)}
						onrowclick={(row) => {
							if (!row.current) {
								selectedVersion =
									selectedVersion === row.version ? null : row.version;
							}
						}}
					/>
				</Card>
				{#if selectedVersionEntry}
					{@render rollbackBar(selectedVersionEntry)}
				{:else}
					<p class="mt-3 text-caption text-muted-foreground">
						Select a superseded version to roll the formal definition back to
						it.
					</p>
				{/if}
			{:else if tab === 'draft'}
				{#if draft.loading && !draft.data}
					<Skeleton shape="block" height="120px" class="rounded-lg" />
				{:else if draft.error}
					<ErrorState
						title="Failed to load draft"
						description={draft.error}
						onretry={() => draft.reload()}
						class="rounded-lg border border-border bg-card"
					/>
				{:else if draft.data}
					{@const currentDraft = draft.data}
					<Card title={currentDraft.name}>
						{#snippet actions()}
							<Badge variant="outline">draft</Badge>
						{/snippet}
						<p class="text-caption text-muted-foreground">
							Updated {formatDateTime(currentDraft.updatedAt)} · same id as the formal
							definition
						</p>
						{#if draftIssues !== null}
							{#if draftIssues.length === 0}
								<p
									class="mt-2 flex items-center gap-1.5 text-caption text-success"
								>
									<Icon name="check" size={12} />
									No publish-blocking issues
								</p>
							{:else}
								<ul class="mt-2 space-y-1">
									{#each draftIssues as issue, index (index)}
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
						{/if}
						{#snippet footer()}
							<div class="flex items-center gap-2">
								<Button
									size="sm"
									disabled={busy}
									onclick={() => ask({ kind: 'promote' })}
								>
									Promote
								</Button>
								<Button
									variant="outline"
									size="sm"
									disabled={busy}
									onclick={() => void checkDraft()}
								>
									Validate
								</Button>
							</div>
						{/snippet}
					</Card>
				{:else}
					<EmptyState
						icon="pencil"
						title="No draft"
						description="This workflow has no stored draft. Editors save work-in-progress here before it is promoted to the formal definition."
						class="rounded-lg border border-border bg-card"
					/>
				{/if}
			{:else}
				{#if runs.loading && runs.loaded === 0}
					<div class="space-y-2">
						{#each Array.from({ length: 4 }, (_, position) => position) as index (index)}
							<Skeleton shape="block" height="44px" class="rounded-lg" />
						{/each}
					</div>
				{:else if runs.error}
					<ErrorState
						title="Failed to load runs"
						description={runs.error}
						onretry={() => runs.reload()}
						class="rounded-lg border border-border bg-card"
					/>
				{:else}
					<Card title="Executions" bodyClass="p-0">
						<DataTable
							columns={runColumns}
							rows={runs.items}
							rowKey={(row) => row.id}
							onrowclick={(row) =>
								goto(resolve(appPath(`/executions/${row.id}`)))}
							emptyTitle="Never run"
							emptyDescription="No execution has been recorded for this workflow."
						/>
					</Card>
					<LoadMorePager
						shown={runs.loaded}
						hasMore={runs.hasMore}
						loading={runs.loading}
						pageSize={runs.pageSize}
						onloadmore={() => runs.loadMore()}
						class="mt-3 rounded-lg border border-border bg-card"
					/>
				{/if}
			{/if}
		{/if}
	</div>
</div>

{#snippet rollbackBar(version: WorkflowVersion)}
	<div class="mt-3 flex items-center gap-3">
		<p class="text-caption text-muted-foreground">
			Selected v{version.version} · {version.note || 'no note'} ·
			{formatDateTime(version.createdAt)}
		</p>
		<Button
			variant="outline"
			size="sm"
			disabled={busy}
			onclick={() => ask({ kind: 'rollback', version })}
		>
			<Icon name="history" size={13} />
			Roll back to v{version.version}
		</Button>
	</div>
{/snippet}

<Dialog
	bind:open={confirming}
	title={confirmCopy.title}
	description={confirmCopy.detail}
	onclose={() => (target = null)}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (confirming = false)}
			>Keep as is</Button
		>
		<Button
			variant="destructive"
			size="sm"
			disabled={!target || busy}
			onclick={() => void runConfirmed()}
		>
			{target?.kind === 'promote' ? 'Promote' : 'Roll back'}
		</Button>
	{/snippet}
</Dialog>
