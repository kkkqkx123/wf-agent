<script lang="ts">
	import type {
		ExecutionDetail,
		TimelineEntry,
		ToolCallEntry,
	} from '$lib/types/models';
	import Card from '$lib/components/ui/Card.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import Timeline from './Timeline.svelte';
	import TimelineOutline from './TimelineOutline.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import GraphExplorer, {
		type GraphOverlay,
	} from '$lib/components/domain/GraphExplorer.svelte';
	import {
		getExecutionTimeline,
		getExecutionToolCalls,
		getExecutionContext,
		getExecutionVariables,
		getExecutionCallStack,
		getExecutionMemory,
	} from '$lib/services/executions';
	import {
		getExecutionCriticalPath,
		getExecutionDecisionPoints,
		getExecutionEfficiency,
		getExecutionFailedNodes,
		getExecutionGraphEdges,
		getExecutionGraphNeighbors,
		getExecutionGraphNodes,
		getExecutionSlowNodes,
		toWorkflowGraph,
		type EfficiencyEntry,
		type SlowNodeEntry,
	} from '$lib/services/graph';
	import {
		formatBytes,
		formatDateTime,
		formatDuration,
		formatNumber,
		shortId,
	} from '$lib/utils/format';
	import { statusTone } from '$lib/utils/status';
	import { cn } from '$lib/utils/cn';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import { toasts } from '$lib/stores/toast.svelte';

	interface Props {
		execution: ExecutionDetail;
		tab?: string;
		class?: string;
	}

	let {
		execution,
		tab = $bindable('overview'),
		class: className = '',
	}: Props = $props();

	let toolCalls = $state<ToolCallEntry[]>([]);
	let toolsError = $state<string | null>(null);
	let timeline = $state<TimelineEntry[]>([]);
	let timelineError = $state<string | null>(null);

	let graphNodes = $state<DisplayNode[]>([]);
	let graphEdges = $state<DisplayEdge[]>([]);
	let graphError = $state<string | null>(null);
	let graphLoading = $state(false);
	let graphNodeId = $state<string | null>(null);
	let activeOverlay = $state<string | null>(null);
	let neighborhoodIds = $state<string[]>([]);

	let slowNodes = $state<SlowNodeEntry[]>([]);
	let decisionPoints = $state<string[]>([]);
	let failedNodes = $state<string[]>([]);
	let criticalPath = $state<string[]>([]);
	let efficiency = $state<EfficiencyEntry | null>(null);
	let analysisError = $state<string | null>(null);
	let analysisLoading = $state(false);

	let context = $state<Array<{ key: string; value: string }>>([]);
	let variables = $state<Array<{ key: string; value: string }>>([]);
	let callStack = $state<
		Array<{ node: string; depth: number; enteredAt: string; status: string }>
	>([]);
	let memory = $state({ currentBytes: 0, peakBytes: 0 });
	let stateError = $state<string | null>(null);
	let stateLoading = $state(false);

	let seenTools = $state('');
	let seenTimeline = $state('');
	let seenGraph = $state('');
	let seenAnalysis = $state('');
	let seenState = $state('');

	const overlays = $derived.by<GraphOverlay[]>(() => {
		const list: GraphOverlay[] = [];
		if (failedNodes.length > 0) {
			list.push({ id: 'failed', label: 'Failed nodes', ids: failedNodes });
		}
		if (criticalPath.length > 0) {
			list.push({ id: 'critical', label: 'Critical path', ids: criticalPath });
		}
		if (activeOverlay === '__neighborhood') {
			list.push({
				id: '__neighborhood',
				label: 'Neighborhood',
				ids: neighborhoodIds,
			});
		}
		return list;
	});

	async function loadGraph(id: string): Promise<void> {
		graphLoading = true;
		graphError = null;
		try {
			const [nodes, edges] = await Promise.all([
				getExecutionGraphNodes(id),
				getExecutionGraphEdges(id),
			]);
			const graph = toWorkflowGraph(nodes, edges);
			graphNodes = graph.nodes.map((node) => ({
				id: node.id,
				label: node.label,
				kind: node.kind,
				status: node.status,
			}));
			graphEdges = graph.edges.map((edge) => ({
				id: edge.id,
				source: edge.from,
				target: edge.to,
				label: edge.label,
			}));
			const [failed, critical] = await Promise.all([
				getExecutionFailedNodes(id).catch(() => [] as string[]),
				getExecutionCriticalPath(id).catch(() => [] as string[]),
			]);
			failedNodes = failed;
			criticalPath = critical;
		} catch (e) {
			graphError = e instanceof Error ? e.message : 'Graph failed to load.';
		} finally {
			graphLoading = false;
		}
	}

	async function loadAnalysis(id: string): Promise<void> {
		analysisLoading = true;
		analysisError = null;
		try {
			const [slow, points, failed, critical, ratio] = await Promise.all([
				getExecutionSlowNodes(id),
				getExecutionDecisionPoints(id),
				getExecutionFailedNodes(id),
				getExecutionCriticalPath(id),
				getExecutionEfficiency(id),
			]);
			slowNodes = slow;
			decisionPoints = points;
			failedNodes = failed;
			criticalPath = critical;
			efficiency = ratio;
		} catch (e) {
			analysisError = e instanceof Error ? e.message : 'Analysis failed.';
		} finally {
			analysisLoading = false;
		}
	}

	async function loadState(id: string): Promise<void> {
		stateLoading = true;
		stateError = null;
		try {
			const [ctx, vars, stack, mem] = await Promise.all([
				getExecutionContext(id),
				getExecutionVariables(id),
				getExecutionCallStack(id),
				getExecutionMemory(id),
			]);
			context = ctx;
			variables = vars;
			callStack = stack;
			memory = mem.peakBytes > 0 ? mem : { currentBytes: 0, peakBytes: 0 };
		} catch (e) {
			stateError = e instanceof Error ? e.message : 'State failed to load.';
		} finally {
			stateLoading = false;
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		try {
			const neighbors = await getExecutionGraphNeighbors(execution.id, id);
			neighborhoodIds = [id, ...neighbors.predecessors, ...neighbors.successors];
			activeOverlay = '__neighborhood';
		} catch (e) {
			toasts.error(
				'Neighborhood failed to load',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	$effect(() => {
		const id = execution.id;
		if (!id) return;
		if (tab === 'tools' && seenTools !== id) {
			seenTools = id;
			toolsError = null;
			void getExecutionToolCalls(id)
				.then((rows) => {
					toolCalls = rows;
				})
				.catch((e: unknown) => {
					seenTools = '';
					toolCalls = [];
					toolsError = e instanceof Error ? e.message : 'Tools failed.';
				});
		}
		if (tab === 'timeline' && seenTimeline !== id) {
			seenTimeline = id;
			timelineError = null;
			void getExecutionTimeline(id)
				.then((rows) => {
					timeline = rows;
				})
				.catch((e: unknown) => {
					seenTimeline = '';
					timeline = [];
					timelineError = e instanceof Error ? e.message : 'Timeline failed.';
				});
		}
		if (tab === 'graph' && seenGraph !== id) {
			seenGraph = id;
			void loadGraph(id);
		}
		if (tab === 'analysis' && seenAnalysis !== id) {
			seenAnalysis = id;
			void loadAnalysis(id);
		}
		if (tab === 'state' && seenState !== id) {
			seenState = id;
			void loadState(id);
		}
	});

	const TABS = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'graph', label: 'Graph' },
		{ id: 'timeline', label: 'Timeline' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'analysis', label: 'Analysis' },
		{ id: 'state', label: 'State' },
	];

	const tone = $derived(statusTone(execution.status));
	const contextItems = $derived(
		context.length > 0 ? context : execution.context,
	);	const progressTone = $derived(
		tone === 'danger'
			? 'danger'
			: tone === 'success'
				? 'success'
				: tone === 'running'
					? 'running'
					: 'default',
	);
</script>

<div class={cn('flex h-full min-h-0 flex-col', className)}>
	<div class="border-b border-border px-3 py-3">
		<div class="flex items-start justify-between gap-2">
			<div class="min-w-0">
				<h2 class="truncate text-title font-semibold">
					{execution.workflowName}
				</h2>
				<p class="mt-0.5 font-mono text-micro text-muted-foreground">
					{execution.id}
				</p>
			</div>
			<StatusBadge status={execution.status} />
		</div>

		<div class="mt-3 space-y-1.5">
			<div
				class="flex items-center justify-between text-micro text-muted-foreground"
			>
				<span>Progress</span>
				<span class="tabular-nums"
					>{execution.tasksDone}/{execution.tasksTotal} tasks</span
				>
			</div>
			<Progress value={execution.progress} tone={progressTone} />
		</div>

		<dl class="mt-3 grid grid-cols-2 gap-x-3 gap-y-2">
			<div>
				<dt class="text-micro text-muted-foreground">Started</dt>
				<dd class="text-caption tabular-nums">
					{formatDateTime(execution.startedAt)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Duration</dt>
				<dd class="text-caption tabular-nums">
					{formatDuration(execution.durationMs)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Memory peak</dt>
				<dd class="text-caption tabular-nums">
					{formatBytes(execution.memoryPeakBytes)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Trigger</dt>
				<dd class="truncate text-caption">{execution.trigger ?? '—'}</dd>
			</div>
		</dl>
	</div>

	<Segmented items={TABS} bind:value={tab} size="sm" class="px-2" panelId="execution-panel" />

	<div
		id="execution-panel"
		role="tabpanel"
		aria-label="Execution sections"
		class="min-h-0 flex-1 overflow-y-auto px-3 py-3"
	>
		{#if tab === 'overview'}
			<div class="space-y-3">
				<Card title="Context">
					<KeyValueList items={contextItems} dense />
				</Card>
				<Card title="Current position">
					<p class="text-body">{execution.currentNode ?? 'No active node'}</p>
					<p class="mt-1 text-caption text-muted-foreground">
						{execution.failedNodes} failed nodes across {formatNumber(
							execution.tasksTotal,
						)} tasks
					</p>
				</Card>
				<Card title="Status migration">
					<ol class="space-y-1.5">
						{#each execution.migration as entry, index (index)}
							<li class="flex items-start gap-2 text-caption">
								<time class="shrink-0 tabular-nums text-muted-foreground">
									{formatDateTime(entry.at)}
								</time>
								<span class="min-w-0">
									<span class="text-foreground">{entry.from}</span>
									<span class="mx-1 text-muted-foreground">→</span>
									<span class="text-foreground">{entry.to}</span>
									<span class="block text-micro text-muted-foreground"
										>{entry.reason}</span
									>
								</span>
							</li>
						{/each}
					</ol>
				</Card>
			</div>
		{:else if tab === 'graph'}
			<GraphExplorer
				nodes={graphNodes}
				edges={graphEdges}
				preset="execution"
				loading={graphLoading}
				error={graphError}
				onretry={() => void loadGraph(execution.id)}
				selectedId={graphNodeId}
				onselect={(id) => (graphNodeId = id)}
				onexpand={(id) => void expandNeighborhood(id)}
				expandLabel="Reveal neighborhood"
				overlays={overlays}
				{activeOverlay}
				onoverlay={(id) => (activeOverlay = id)}
			/>
		{:else if tab === 'timeline'}
			{#if timelineError}
				<ErrorState
					title="Timeline failed to load"
					description={timelineError}
					onretry={() => {
						seenTimeline = '';
						timelineError = null;
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="flex items-start gap-3">
					<Timeline entries={timeline} class="min-w-0 flex-1" />
					<TimelineOutline entries={timeline} class="hidden w-44 xl:block" />
				</div>
			{/if}
		{:else if tab === 'tools'}
			{#if toolsError}
				<ErrorState
					title="Tool calls failed to load"
					description={toolsError}
					onretry={() => {
						seenTools = '';
						toolsError = null;
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-2">
					{#each toolCalls as entry (entry.id)}
						<ToolCallCard {entry} />
					{/each}
				</div>
			{/if}
		{:else if tab === 'analysis'}
			{#if analysisLoading}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else if analysisError}
				<ErrorState
					title="Analysis failed to load"
					description={analysisError}
					onretry={() => void loadAnalysis(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-3">
					{#if efficiency}
						<Card title="Efficiency">
							<p class="text-caption text-muted-foreground">
								{efficiency.executedSteps} executed steps vs {efficiency.optimalSteps}
								optimal ({efficiency.ratio.toFixed(2)}×) · {efficiency.wastefulNodes}
								wasteful nodes · {efficiency.retryCount} retries
							</p>
						</Card>
					{/if}
					<Card title="Slow nodes">
						<ul class="space-y-1.5">
							{#each slowNodes as node (node.node)}
								<li class="flex items-center justify-between gap-2 text-caption">
									<span class="truncate font-mono">{node.node}</span>
									<span class="shrink-0 tabular-nums text-muted-foreground">
										{formatDuration(node.durationMs)}
									</span>
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">No slow nodes recorded.</li>
							{/each}
						</ul>
					</Card>
					<Card title="Critical path">
						<ol class="flex flex-wrap items-center gap-1.5">
							{#each criticalPath as node (node)}
								<li class="flex items-center gap-1.5">
									<span
										class="rounded border border-border px-1.5 py-0.5 font-mono text-micro"
									>
										{node}
									</span>
									{#if node !== criticalPath[criticalPath.length - 1]}
										<span class="text-micro text-muted-foreground">→</span>
									{/if}
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">No path data.</li>
							{/each}
						</ol>
					</Card>
					<Card title="Decision points">
						<div class="flex flex-wrap gap-1.5">
							{#each decisionPoints as node (node)}
								<span
									class="rounded-full border border-border px-2 py-0.5 font-mono text-micro"
								>
									{node}
								</span>
							{/each}
						</div>
						<p class="mt-2 text-caption text-muted-foreground">
							{formatNumber(failedNodes.length)} failed nodes
						</p>
					</Card>
				</div>
			{/if}
		{:else}
			{#if stateLoading}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else if stateError}
				<ErrorState
					title="State failed to load"
					description={stateError}
					onretry={() => void loadState(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-3">
					<Card title="Variables">
						<KeyValueList items={variables} dense />
					</Card>
					<Card title="Call stack">
						<ol class="space-y-2">
							{#each callStack as frame (frame.node)}
								<li
									class="flex items-start justify-between gap-2 border-b border-border/60 pb-2 last:border-0 last:pb-0"
								>
									<span class="min-w-0">
										<span class="block truncate font-mono text-caption"
											>{frame.node}</span
										>
										<span class="text-micro text-muted-foreground">
											depth {frame.depth} · {formatDateTime(frame.enteredAt)}
										</span>
									</span>
									<StatusBadge status={frame.status} size="sm" dot={false} />
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">Call stack empty.</li>
							{/each}
						</ol>
					</Card>
					<Card title="Memory">
						<div class="space-y-2">
							<div>
								<div class="flex items-center justify-between text-caption">
									<span class="text-muted-foreground">Current</span>
									<span class="tabular-nums"
										>{formatBytes(memory.currentBytes)}</span
									>
								</div>
								<Progress
									value={memory.currentBytes /
										Math.max(1, memory.peakBytes)}
									tone="default"
									class="mt-1"
								/>
							</div>
							<p class="text-caption text-muted-foreground">
								Peak {formatBytes(memory.peakBytes)} · snapshot {shortId(
									execution.id,
									10,
								)}
							</p>
						</div>
					</Card>
				</div>
			{/if}
		{/if}
	</div>
</div>
