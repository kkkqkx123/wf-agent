<script lang="ts">
	import type {
		ExecutionDetail,
		ExecutionHierarchy,
		ExecutionHistory,
		ExecutionSubtree,
		NodeTrace,
		TimelineEntry,
		ToolCallEntry,
	} from '$lib/types/models';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Progress from '@wf-agent/ui/components/Progress.svelte';
	import Timeline from './Timeline.svelte';
	import TimelineOutline from './TimelineOutline.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import NodeTracePanel from './NodeTracePanel.svelte';
	import ExecutionHierarchyBreadcrumb from './ExecutionHierarchyBreadcrumb.svelte';
	import ExecutionHierarchyTree from './ExecutionHierarchyTree.svelte';
	import ExecutionHistoryPanel from './ExecutionHistoryPanel.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import { extractMarkdownText } from '$lib/utils/markdown';
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
		getExecutionHierarchy,
		getExecutionSubtree,
		getExecutionHistory,
	} from '$lib/services/executions';
	import { getExecutionNodeTraces } from '$lib/services/node-trace';
	import {
		getExecutionCriticalPath,
		getExecutionDecisionPoints,
		getExecutionEfficiency,
		getExecutionFailedNodes,
		getExecutionGraphNeighbors,
		getExecutionGraphOverview,
		getExecutionSlowNodes,
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
	import { statusTone } from '@wf-agent/ui/status';
	import { cn } from '@wf-agent/ui/cn';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import {
		applyEdgeOverlay,
		applyExecutionOverlay,
		projectEdgeOverlay,
		projectExecutionOverlay,
	} from '$lib/graph/execution-projection';
	import {
		EXECUTION_TABS,
		type ExecutionTab,
	} from '$lib/config/execution-tabs';
	import { openEventStream, type StreamState } from '$lib/api/sse';
	import type { EventRecord } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { SvelteMap } from 'svelte/reactivity';

	interface Props {
		execution: ExecutionDetail;
		tab?: ExecutionTab;
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

	let nodeTraces = $state<NodeTrace[]>([]);
	let nodeTracesSkipped = $state(0);
	let nodeTracesError = $state<string | null>(null);
	let nodeTracesLoading = $state(false);

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
	let seenTrace = $state('');
	let seenGraph = $state('');
	let seenAnalysis = $state('');
	let seenState = $state('');
	let seenHierarchy = $state('');
	let seenHistory = $state('');

	let hierarchy = $state<ExecutionHierarchy | null>(null);
	let hierarchyError = $state<string | null>(null);
	let subtree = $state<ExecutionSubtree | null>(null);
	let recordedHistory = $state<ExecutionHistory | null>(null);
	let historyError = $state<string | null>(null);

	// Live execution overlay: SSE frames buffer here and flush on a fixed
	// tick, so high-frequency node updates never re-render per frame.
	let liveStatuses = $state<Record<string, string>>({});
	let streamState = $state<StreamState>('closed');
	let pendingLive = new SvelteMap<string, string>();

	// Replay cursor over the call stack; when set, the same projection
	// bridge renders history instead of live state.
	let replayIndex = $state<number | null>(null);
	let replayPlaying = $state(false);
	let replaySpeed = $state(1);

	// Tool list filter driven by graph selection (bidirectional focus).
	let toolFilter = $state('');
	let explorer = $state<{ focus: (id: string) => void } | null>(null);

	const failedAll = $derived([
		...new Set([...failedNodes, ...execution.analysis.failureNodes]),
	]);
	const criticalAll = $derived([
		...new Set([...criticalPath, ...execution.analysis.criticalPath]),
	]);
	const slowEntries = $derived([...execution.analysis.slowNodes, ...slowNodes]);

	const slowAll = $derived(slowEntries);

	/** Deduplicated slow rows for the analysis tab; overlapping sources keep
	 * the longest duration so graph and analysis never disagree on membership. */
	const slowDisplay = $derived.by(() => {
		const longest = new SvelteMap<string, number>();
		for (const entry of slowEntries) {
			const prev = longest.get(entry.node) ?? 0;
			if (entry.durationMs > prev) longest.set(entry.node, entry.durationMs);
		}
		return [...longest.entries()].map(([node, durationMs]) => ({
			node,
			durationMs,
		}));
	});

	const decisionAll = $derived([
		...new Set([...decisionPoints, ...execution.analysis.decisionPoints]),
	]);

	const completedFrames = $derived(
		callStack.filter((frame) =>
			['completed', 'complete', 'success', 'succeeded', 'done', 'ok'].includes(
				frame.status.trim().toLowerCase(),
			),
		),
	);

	const replayView = $derived.by(() => {
		if (replayIndex === null || callStack.length === 0) return null;
		const upto = callStack.slice(0, replayIndex + 1);
		return {
			executedNodes: upto.map((frame) => frame.node),
			currentNode:
				upto.length > 0 ? (upto[upto.length - 1].node ?? null) : null,
			failedNodes: upto
				.filter((frame) =>
					[
						'failed',
						'failure',
						'error',
						'errored',
						'timeout',
						'aborted',
					].includes(frame.status.trim().toLowerCase()),
				)
				.map((frame) => frame.node),
		};
	});

	const executionOverlay = $derived(
		projectExecutionOverlay(graphNodes, {
			currentNode: replayView?.currentNode ?? execution.currentNode,
			failedNodes: [...failedAll, ...(replayView?.failedNodes ?? [])],
			criticalPath: criticalAll,
			slowNodes: slowAll,
			decisionPoints: decisionAll,
			executedNodes:
				replayView?.executedNodes ?? completedFrames.map((frame) => frame.node),
			liveStatuses: replayView ? {} : liveStatuses,
		}),
	);

	const overlayNodes = $derived(
		applyExecutionOverlay(graphNodes, executionOverlay),
	);

	const overlayEdges = $derived.by(() => {
		const tones = projectEdgeOverlay(graphEdges, executionOverlay);
		return applyEdgeOverlay(graphEdges, tones);
	});

	const pulseIds = $derived(
		[...executionOverlay.marks.values()]
			.filter((mark) => mark.pulse)
			.map((mark) => mark.id),
	);

	const criticalIds = $derived(
		[...executionOverlay.marks.values()]
			.filter((mark) => mark.critical)
			.map((mark) => mark.id),
	);

	const heatTierById = $derived(
		Object.fromEntries(
			[...executionOverlay.marks.values()]
				.filter((mark) => mark.heatTier > 0)
				.map((mark) => [mark.id, mark.heatTier]),
		),
	);

	const decisionIds = $derived(
		[...executionOverlay.marks.values()]
			.filter((mark) => mark.decision)
			.map((mark) => mark.id),
	);

	const heatLabels = $derived.by(() => {
		const durations = new Map(
			slowEntries.map((entry) => [entry.node, entry.durationMs]),
		);
		return Object.fromEntries(
			[...executionOverlay.marks.values()]
				.filter((mark) => mark.slow)
				.map((mark) => {
					const duration = durations.get(mark.id);
					return [
						mark.id,
						typeof duration === 'number'
							? formatDuration(duration)
							: 'slow node',
					];
				}),
		);
	});

	const decisionLabels = $derived.by(() => {
		const labels: Record<string, string> = {};
		for (const nodeId of decisionIds) {
			const branches = graphEdges
				.filter((edge) => edge.source === nodeId && (edge.label ?? '').trim())
				.map((edge) => edge.label as string);
			labels[nodeId] =
				branches.length > 0 ? branches.slice(0, 3).join(' / ') : 'branch node';
		}
		return labels;
	});

	/** Active replay scope: executed graph node ids plus the cursor frame's
	 * timestamp. Tools join by exact node id, timeline entries by exact
	 * timestamp; unattributed rows stay visible. */
	const replayScope = $derived.by(() => {
		if (replayIndex === null || callStack.length === 0) return null;
		const graphIds = new Set(graphNodes.map((node) => node.id));
		const executed = new Set(
			(replayView?.executedNodes ?? []).filter((id) => graphIds.has(id ?? '')),
		);
		const frame = callStack[Math.min(replayIndex, callStack.length - 1)];
		return {
			graphIds,
			executed,
			cutoff: frame?.enteredAt || null,
		};
	});

	const filteredTools = $derived.by(() => {
		const needle = toolFilter.trim().toLowerCase();
		const scoped =
			replayScope === null
				? toolCalls
				: toolCalls.filter((tool) =>
						replayScope.graphIds.has(tool.nodeId ?? '')
							? replayScope.executed.has(tool.nodeId as string)
							: true,
					);
		if (!needle) return scoped;
		return scoped.filter(
			(tool) =>
				tool.name.toLowerCase().includes(needle) ||
				tool.id.toLowerCase().includes(needle),
		);
	});

	/** Timeline entries up to the replay cursor (exact timestamp cutoff).
	 * Entries without a timestamp stay visible; nothing is hidden on
	 * uncertain grounds. */
	const scopedTimeline = $derived.by(() => {
		const cutoff = replayScope?.cutoff;
		if (!cutoff) return timeline;
		return timeline.filter((entry) => !entry.at || entry.at <= cutoff);
	});

	/** Best-effort match between a tool call and a graph node by name. */
	function matchToolNode(toolName: string, toolId: string): string | null {
		const needle = toolName.trim().toLowerCase();
		for (const node of graphNodes) {
			const id = node.id.toLowerCase();
			const label = node.label.toLowerCase();
			if (id === needle || label === needle || id === toolId.toLowerCase()) {
				return node.id;
			}
		}
		for (const node of graphNodes) {
			const id = node.id.toLowerCase();
			const label = node.label.toLowerCase();
			if (
				(needle && (label.includes(needle) || needle.includes(id))) ||
				toolId.toLowerCase().includes(id)
			) {
				return node.id;
			}
		}
		return null;
	}

	function focusGraphNode(nodeId: string): void {
		tab = 'graph';
		queueMicrotask(() => explorer?.focus(nodeId));
	}

	function focusToolOnGraph(toolId: string, toolName: string): void {
		const direct = toolCalls.find((tool) => tool.id === toolId)?.nodeId;
		if (direct && graphNodes.some((node) => node.id === direct)) {
			focusGraphNode(direct);
			return;
		}
		const nodeId = matchToolNode(toolName, toolId);
		if (!nodeId) {
			toasts.info('No graph node matches this tool call');
			return;
		}
		focusGraphNode(nodeId);
	}

	function filterToolsByGraph(nodeId: string): void {
		const node = graphNodes.find((entry) => entry.id === nodeId);
		toolFilter = node ? node.label : nodeId;
		tab = 'tools';
	}

	/** Open the node trace list with the selected graph node expanded. */
	function openNodeTrace(nodeId: string): void {
		graphNodeId = nodeId;
		tab = 'trace';
	}

	/** Jump from the graph selection back to the analysis row that
	 * produced its mark. */
	function revealInAnalysis(kind: 'slow' | 'critical' | 'decision'): void {
		tab = 'analysis';
		queueMicrotask(() =>
			document
				.getElementById(`analysis-${kind}`)
				?.scrollIntoView({ block: 'nearest' }),
		);
	}

	const selectedAnalysisKind = $derived.by(
		(): 'slow' | 'critical' | 'decision' | null => {
			if (!graphNodeId) return null;
			if (slowEntries.some((entry) => entry.node === graphNodeId))
				return 'slow';
			if (criticalAll.includes(graphNodeId)) return 'critical';
			if (decisionAll.includes(graphNodeId)) return 'decision';
			return null;
		},
	);

	function noteLiveEvent(event: EventRecord): void {
		let meta: Record<string, unknown>;
		try {
			meta = JSON.parse(event.payload || '{}') as Record<string, unknown>;
		} catch {
			return;
		}
		const node = ['node_id', 'nodeId', 'node']
			.map((key) => meta[key])
			.find(
				(value): value is string => typeof value === 'string' && value !== '',
			);
		const status = ['status', 'state']
			.map((key) => meta[key])
			.find(
				(value): value is string => typeof value === 'string' && value !== '',
			);
		if (node && status) pendingLive.set(node, status);
	}

	const isLive = $derived(
		[
			'running',
			'in_progress',
			'executing',
			'streaming',
			'started',
			'pending',
		].includes(execution.status.trim().toLowerCase()),
	);

	// Live subscription lives only while the graph tab is visible and the
	// execution is still active; leaving the tab closes the stream.
	$effect(() => {
		if (tab !== 'graph' || !isLive) return;
		const stop = openEventStream({
			executionId: execution.id,
			onEvent: noteLiveEvent,
			onState: (state) => (streamState = state),
		});
		const timer = setInterval(() => {
			if (pendingLive.size === 0) return;
			const flushed = Object.fromEntries(pendingLive);
			pendingLive.clear();
			liveStatuses = { ...liveStatuses, ...flushed };
		}, 120);
		return () => {
			clearInterval(timer);
			pendingLive.clear();
			stop();
		};
	});

	// Replay playback advances the cursor on a fixed tick; colors flow
	// through the same projection bridge as live state.
	$effect(() => {
		if (!replayPlaying || callStack.length === 0) return;
		const timer = setInterval(
			() => {
				const last = callStack.length - 1;
				const next = (replayIndex ?? -1) + 1;
				if (next > last) {
					replayPlaying = false;
					return;
				}
				replayIndex = next;
			},
			Math.max(200, Math.round(800 / replaySpeed)),
		);
		return () => clearInterval(timer);
	});

	const overlays = $derived.by<GraphOverlay[]>(() => {
		const list: GraphOverlay[] = [];
		if (failedAll.length > 0) {
			list.push({ id: 'failed', label: 'Failed nodes', ids: failedAll });
		}
		if (criticalAll.length > 0) {
			list.push({ id: 'critical', label: 'Critical path', ids: criticalAll });
		}
		if (decisionAll.length > 0) {
			list.push({
				id: 'decisions',
				label: 'Decision points',
				ids: decisionAll,
			});
		}
		if (slowEntries.length > 0) {
			list.push({
				id: 'slow',
				label: 'Slow nodes',
				ids: [...new Set(slowEntries.map((entry) => entry.node))],
			});
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
			const overview = await getExecutionGraphOverview(id);
			graphNodes = overview.graph.nodes.map((node) => ({
				id: node.id,
				label: node.label,
				kind: node.kind,
				status: node.status,
			}));
			graphEdges = overview.graph.edges.map((edge) => ({
				id: edge.id,
				source: edge.from,
				target: edge.to,
				label: edge.label,
				kind: edge.kind,
			}));
			failedNodes = overview.failedNodes;
			criticalPath = overview.criticalPath;
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

	async function loadNodeTraces(id: string): Promise<void> {
		seenTrace = id;
		nodeTracesError = null;
		nodeTracesLoading = true;
		try {
			const page = await getExecutionNodeTraces(id);
			nodeTraces = page.items;
			nodeTracesSkipped = page.skipped;
		} catch (e) {
			seenTrace = '';
			nodeTraces = [];
			nodeTracesError = e instanceof Error ? e.message : 'Node traces failed.';
		} finally {
			nodeTracesLoading = false;
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		try {
			const neighbors = await getExecutionGraphNeighbors(execution.id, id);
			neighborhoodIds = [
				id,
				...neighbors.predecessors,
				...neighbors.successors,
			];
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
		if (tab === 'trace' && seenTrace !== id) {
			void loadNodeTraces(id);
		}
		// The overview live card reads the same trace snapshot; live runs
		// pull it once so the current node output shows without visiting trace.
		if (tab === 'overview' && seenTrace !== id && isLive) {
			void loadNodeTraces(id);
		}
		if (tab === 'graph' && seenGraph !== id) {
			seenGraph = id;
			void loadGraph(id);
		}
		// The replay cursor walks the call stack, so the graph tab pulls
		// state once; the state tab then reuses the same snapshot.
		if (tab === 'graph' && seenState !== id) {
			seenState = id;
			void loadState(id);
		}
		if (tab === 'analysis' && seenAnalysis !== id) {
			seenAnalysis = id;
			void loadAnalysis(id);
		}
		if (tab === 'state' && seenState !== id) {
			seenState = id;
			void loadState(id);
		}
		if (tab === 'hierarchy' && seenHierarchy !== id) {
			seenHierarchy = id;
			void loadHierarchy(id);
		}
		if (tab === 'history' && seenHistory !== id) {
			seenHistory = id;
			void loadHistory(id);
		}
	});

	async function loadHierarchy(id: string): Promise<void> {
		hierarchyError = null;
		try {
			const [view, tree] = await Promise.all([
				getExecutionHierarchy(id),
				getExecutionSubtree(id),
			]);
			hierarchy = view;
			subtree = tree;
		} catch (e: unknown) {
			seenHierarchy = '';
			hierarchy = null;
			subtree = null;
			hierarchyError = e instanceof Error ? e.message : 'Hierarchy failed.';
		}
	}

	async function loadHistory(id: string): Promise<void> {
		historyError = null;
		try {
			recordedHistory = await getExecutionHistory(id);
		} catch (e: unknown) {
			seenHistory = '';
			recordedHistory = null;
			historyError = e instanceof Error ? e.message : 'History failed.';
		}
	}

	const tone = $derived(statusTone(execution.status));
	const contextItems = $derived(
		context.length > 0 ? context : execution.context,
	);
	const progressTone = $derived(
		tone === 'danger'
			? 'danger'
			: tone === 'success'
				? 'success'
				: tone === 'running'
					? 'running'
					: 'default',
	);

	/** Current node output for the overview card. Token deltas carry no node
	 * attribution on the backend, so this renders the latest recorded output
	 * of the current node, not a true per-node stream. */
	const liveTrace = $derived(
		execution.currentNode
			? (nodeTraces.find((trace) => trace.nodeId === execution.currentNode) ??
					null)
			: null,
	);
	const liveMarkdown = $derived(
		liveTrace ? extractMarkdownText(liveTrace.output) : null,
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

	<Segmented
		items={EXECUTION_TABS}
		bind:value={tab}
		size="sm"
		class="px-2"
		panelId="execution-panel"
	/>

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
				<Card title="Live output">
					{#if liveTrace}
						<div class="flex items-center justify-between gap-2">
							<p class="truncate font-mono text-caption text-foreground">
								{liveTrace.nodeName || liveTrace.nodeId}
							</p>
							{#if isLive}
								<span
									class="flex shrink-0 items-center gap-1.5 text-micro text-muted-foreground"
								>
									<span
										class="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-running"
									></span>
									Live
								</span>
							{/if}
						</div>
						<div class="mt-1.5">
							{#if liveMarkdown !== null}
								<StreamMarkdown content={liveMarkdown} done={!isLive} />
							{:else}
								<p
									class="text-caption break-words whitespace-pre-wrap text-foreground"
								>
									{typeof liveTrace.output === 'string'
										? liveTrace.output
										: JSON.stringify(liveTrace.output ?? null)}
								</p>
							{/if}
						</div>
						{#if execution.currentNode}
							<button
								type="button"
								class="mt-1.5 text-micro text-foreground underline-offset-2 hover:underline"
								onclick={() =>
									execution.currentNode && openNodeTrace(execution.currentNode)}
							>
								Open trace
							</button>
						{/if}
					{:else if nodeTracesLoading}
						<p class="text-caption text-muted-foreground">
							Loading node output…
						</p>
					{:else}
						<p class="text-caption text-muted-foreground">
							{isLive
								? 'Node output appears once a node has produced it.'
								: 'Open the Trace tab to load node outputs.'}
						</p>
					{/if}
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
			<div class="mb-2 flex flex-wrap items-center gap-2">
				{#if isLive}
					<span class="text-micro text-muted-foreground">
						Live: {streamState === 'open' ? 'connected' : streamState}
						{Object.keys(liveStatuses).length > 0
							? `· ${Object.keys(liveStatuses).length} live node(s)`
							: ''}
					</span>
				{:else}
					<span class="text-micro text-muted-foreground">Historical run</span>
				{/if}
				{#if callStack.length > 0}
					<span class="mx-1 h-4 w-px bg-border"></span>
					{#if replayIndex === null}
						<button
							type="button"
							class="text-micro text-foreground underline-offset-2 hover:underline"
							onclick={() => {
								replayIndex = 0;
								replayPlaying = false;
							}}
						>
							Start replay
						</button>
					{:else}
						<button
							type="button"
							class="text-micro text-foreground underline-offset-2 hover:underline"
							onclick={() => (replayPlaying = !replayPlaying)}
						>
							{replayPlaying ? 'Pause' : 'Play'}
						</button>
						<label class="text-micro text-muted-foreground">
							Speed
							<select
								bind:value={replaySpeed}
								class="ml-1 rounded border border-border bg-card text-micro text-foreground"
								aria-label="Replay speed"
							>
								<option value={0.5}>0.5×</option>
								<option value={1}>1×</option>
								<option value={2}>2×</option>
								<option value={4}>4×</option>
							</select>
						</label>
						<button
							type="button"
							class="text-micro text-muted-foreground underline-offset-2 hover:underline"
							disabled={replayIndex <= 0}
							onclick={() => {
								replayPlaying = false;
								replayIndex = Math.max(0, (replayIndex ?? 1) - 1);
							}}
						>
							Step back
						</button>
						<button
							type="button"
							class="text-micro text-muted-foreground underline-offset-2 hover:underline"
							disabled={replayIndex >= callStack.length - 1}
							onclick={() => {
								replayPlaying = false;
								replayIndex = Math.min(
									callStack.length - 1,
									(replayIndex ?? -1) + 1,
								);
							}}
						>
							Step forward
						</button>
						<input
							type="range"
							min={0}
							max={callStack.length - 1}
							value={replayIndex}
							oninput={(event) => {
								replayPlaying = false;
								replayIndex = Number(event.currentTarget.value);
							}}
							class="w-32 accent-current"
							aria-label="Replay position"
						/>
						<span class="text-micro tabular-nums text-muted-foreground">
							{(replayIndex ?? 0) + 1}/{callStack.length}
						</span>
						<button
							type="button"
							class="text-micro text-muted-foreground underline-offset-2 hover:underline"
							onclick={() => {
								replayPlaying = false;
								replayIndex = null;
							}}
						>
							Exit replay
						</button>
					{/if}
				{/if}
			</div>
			<GraphExplorer
				bind:this={explorer}
				nodes={overlayNodes}
				edges={overlayEdges}
				preset="execution"
				loading={graphLoading}
				error={graphError}
				onretry={() => void loadGraph(execution.id)}
				selectedId={graphNodeId}
				onselect={(id) => (graphNodeId = id)}
				onexpand={(id) => void expandNeighborhood(id)}
				expandLabel="Reveal neighborhood"
				{overlays}
				{activeOverlay}
				onoverlay={(id) => (activeOverlay = id)}
				{pulseIds}
				{criticalIds}
				failedIds={failedAll}
				{heatTierById}
				{decisionIds}
				{heatLabels}
				{decisionLabels}
			>
				{#snippet inspector()}
					{#if graphNodeId}
						<button
							type="button"
							class="mt-1 text-micro text-foreground underline-offset-2 hover:underline"
							onclick={() => graphNodeId && filterToolsByGraph(graphNodeId)}
						>
							Show tool calls for this node
						</button>
						<button
							type="button"
							class="mt-1 text-micro text-foreground underline-offset-2 hover:underline"
							onclick={() => graphNodeId && openNodeTrace(graphNodeId)}
						>
							Show node trace
						</button>
						{#if selectedAnalysisKind}
							<button
								type="button"
								class="mt-1 text-micro text-foreground underline-offset-2 hover:underline"
								onclick={() =>
									selectedAnalysisKind &&
									revealInAnalysis(selectedAnalysisKind)}
							>
								Show in analysis
							</button>
						{/if}
					{/if}
				{/snippet}
			</GraphExplorer>
		{:else if tab === 'trace'}
			<NodeTracePanel
				executionId={execution.id}
				traces={nodeTraces}
				skipped={nodeTracesSkipped}
				loading={nodeTracesLoading}
				error={nodeTracesError}
				selectedNodeId={graphNodeId}
				onretry={() => {
					seenTrace = '';
					nodeTracesError = null;
				}}
				onlocate={(nodeId) => focusGraphNode(nodeId)}
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
				{#if replayScope !== null}
					<p class="mb-2 text-micro text-muted-foreground">
						Following replay cursor ({scopedTimeline.length}/{timeline.length})
					</p>
				{/if}
				<div class="flex items-start gap-3">
					<Timeline entries={scopedTimeline} class="min-w-0 flex-1" />
					<TimelineOutline
						entries={scopedTimeline}
						class="hidden w-44 xl:block"
					/>
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
					{#if replayScope !== null}
						<p class="text-micro text-muted-foreground">
							Following replay cursor ({filteredTools.length}/{toolCalls.length})
						</p>
					{/if}
					{#if toolFilter}
						<div class="flex items-center gap-2">
							<p class="text-micro text-muted-foreground">
								Filtered by “{toolFilter}” ({filteredTools.length}/{toolCalls.length})
							</p>
							<button
								type="button"
								class="text-micro text-foreground underline-offset-2 hover:underline"
								onclick={() => (toolFilter = '')}
							>
								Clear
							</button>
						</div>
					{/if}
					{#each filteredTools as entry (entry.id)}
						<div
							role="button"
							tabindex={0}
							aria-label={`Locate tool ${entry.name} on graph`}
							title="Click to locate on graph"
							onclick={() => focusToolOnGraph(entry.id, entry.name)}
							onkeydown={(event) => {
								if (event.key === 'Enter' || event.key === ' ') {
									event.preventDefault();
									focusToolOnGraph(entry.id, entry.name);
								}
							}}
							class="cursor-pointer rounded-lg"
						>
							<ToolCallCard {entry} />
						</div>
					{:else}
						<p class="text-caption text-muted-foreground">
							{toolCalls.length === 0
								? 'No tool calls recorded.'
								: 'No tool calls match the filter.'}
						</p>
					{/each}
				</div>
			{/if}
		{:else if tab === 'analysis'}
			{#if analysisLoading}
				<Skeleton
					lines={5}
					class="rounded-lg border border-border bg-card p-4"
				/>
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
						<ul class="space-y-1.5" id="analysis-slow">
							{#each slowDisplay as node (node.node)}
								<li
									class="flex items-center justify-between gap-2 text-caption"
								>
									<button
										type="button"
										class="truncate font-mono underline-offset-2 hover:underline"
										title="Locate on graph"
										onclick={() => focusGraphNode(node.node)}
									>
										{node.node}
									</button>
									<span class="shrink-0 tabular-nums text-muted-foreground">
										{formatDuration(node.durationMs)}
									</span>
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">
									No slow nodes recorded.
								</li>
							{/each}
						</ul>
					</Card>
					<Card title="Critical path">
						<ol
							class="flex flex-wrap items-center gap-1.5"
							id="analysis-critical"
						>
							{#each criticalAll as node (node)}
								<li class="flex items-center gap-1.5">
									<button
										type="button"
										class="rounded border border-border px-1.5 py-0.5 font-mono text-micro underline-offset-2 hover:underline"
										title="Locate on graph"
										onclick={() => focusGraphNode(node)}
									>
										{node}
									</button>
									{#if node !== criticalAll[criticalAll.length - 1]}
										<span class="text-micro text-muted-foreground">→</span>
									{/if}
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">
									No path data.
								</li>
							{/each}
						</ol>
					</Card>
					<Card title="Decision points">
						<div class="flex flex-wrap gap-1.5" id="analysis-decision">
							{#each decisionAll as node (node)}
								<button
									type="button"
									class="rounded-full border border-border px-2 py-0.5 font-mono text-micro underline-offset-2 hover:underline"
									title="Locate on graph"
									onclick={() => focusGraphNode(node)}
								>
									{node}
								</button>
							{/each}
						</div>
						<p class="mt-2 text-caption text-muted-foreground">
							{formatNumber(failedAll.length)} failed nodes
						</p>
					</Card>
				</div>
			{/if}
		{:else if tab === 'hierarchy'}
			{#if hierarchyError}
				<ErrorState
					title="Hierarchy failed to load"
					description={hierarchyError}
					onretry={() => loadHierarchy(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if hierarchy && subtree}
				<div class="space-y-4">
					<ExecutionHierarchyBreadcrumb {hierarchy} />
					<ExecutionHierarchyTree {subtree} currentId={execution.id} />
				</div>
			{:else}
				<Skeleton
					lines={6}
					class="rounded-lg border border-border bg-card p-4"
				/>
			{/if}
		{:else if tab === 'history'}
			{#if historyError}
				<ErrorState
					title="History failed to load"
					description={historyError}
					onretry={() => loadHistory(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else if recordedHistory}
				<ExecutionHistoryPanel history={recordedHistory} />
			{:else}
				<Skeleton
					lines={6}
					class="rounded-lg border border-border bg-card p-4"
				/>
			{/if}
		{:else}
			{#if stateLoading}
				<Skeleton
					lines={5}
					class="rounded-lg border border-border bg-card p-4"
				/>
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
								<li class="text-caption text-muted-foreground">
									Call stack empty.
								</li>
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
									value={memory.currentBytes / Math.max(1, memory.peakBytes)}
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
