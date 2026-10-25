<script lang="ts">
	import type {
		Execution,
		ExecutionHierarchy,
		ExecutionSubtree,
		NodeExecution,
		NodeTrace,
		TimelineEntry,
		ToolCallEntry,
	} from '#lib/types/models';
	import Card from '#lib/ui/components/Card.svelte';
	import ExecutionAnnotation from './ExecutionAnnotation.svelte';
	import NodeTracePanel from './NodeTracePanel.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Progress from '#lib/ui/components/Progress.svelte';
	import Timeline from './Timeline.svelte';
	import TimelineOutline from './TimelineOutline.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import ToolCallGroup from './ToolCallGroup.svelte';
	import { groupToolCalls } from '#lib/utils/tool-output';
	import { preferences } from '#lib/stores/preferences.svelte';
	import ExecutionHierarchyBreadcrumb from './ExecutionHierarchyBreadcrumb.svelte';
	import ExecutionHierarchyTree from './ExecutionHierarchyTree.svelte';
	import StreamMarkdown from '#lib/components/chat/LazyStreamMarkdown.svelte';
	import { extractMarkdownText } from '#lib/utils/markdown';
	import type { GraphOverlay } from '#lib/graph/display-model';
	import { getExecutionHistory, getExecutionToolCalls } from '#lib/services/executions';
	import { getExecutionGraphNeighbors } from '#lib/services/graph';
	import type { EfficiencyEntry, SlowNodeEntry } from '#lib/services/graph';
	import {
		loadAnalysisData,
		loadGraphData,
		loadHierarchyData,
		loadStateData,
		loadTraceData,
	} from '#lib/services/inspector-loaders';
	import { isLiveStatus, watchExecutionActivity } from '#lib/services/inspector-activity';
	import { watchErrorAnalysis } from '#lib/services/error-analysis-live';
	import type { ErrorRecord } from '#lib/utils/stream-parser';
	import { formatBytes, formatDateTime, formatDuration, formatNumber } from '#lib/utils/format';
	import { statusTone } from '#lib/ui/status';
	import { cn } from '#lib/ui/cn';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import {
		applyEdgeOverlay,
		applyExecutionOverlay,
		matchDecisionNodeId,
		projectEdgeOverlay,
		projectExecutionOverlay,
	} from '#lib/graph/execution-projection';
	import {
		EXECUTION_TABS,
		TAB_GROUPS,
		defaultTab,
		type ExecutionTab,
	} from '#lib/config/execution-tabs';
	import type { StreamState } from '#lib/api/sse';
	import type { EventRecord } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { t } from '#lib/i18n';

	interface Props {
		execution: Execution;
		tab?: ExecutionTab;
		class?: string;
		/** Reload the execution, typically after a terminal event. */
		onrefresh?: () => void;
	}

	let {
		execution,
		tab = $bindable('overview'),
		class: className = '',
		onrefresh,
	}: Props = $props();

	// Which engine owns this execution decides the tab strip and the extra
	// loop-specific panels below.
	const kind = $derived(execution.kind);
	const kindTabs = $derived(EXECUTION_TABS.filter((entry) => TAB_GROUPS[kind].includes(entry.id)));

	// Keep `?tab=` honest across kinds: a workflow tab on a loop detail (or
	// vice versa, e.g. after the redirect) falls back to the group default.
	$effect(() => {
		if (!TAB_GROUPS[kind].includes(tab)) {
			tab = defaultTab();
		}
	});

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
	let liveErrors = $state<ErrorRecord[]>([]);
	let liveErrorNotice = $state<string | null>(null);

	let variables = $state<Array<{ key: string; value: string }>>([]);
	let historyNodes = $state<NodeExecution[]>([]);
	let stateError = $state<string | null>(null);
	let stateLoading = $state(false);

	let seenTools = $state('');
	let seenTimeline = $state('');
	let seenTrace = $state('');
	let seenGraph = $state('');
	let seenAnalysis = $state('');
	let seenState = $state('');
	/** Null marks "not loaded", which an empty execution id could never mean. */
	let seenHierarchy = $state<string | null>(null);

	let hierarchy = $state<ExecutionHierarchy | null>(null);
	let hierarchyError = $state<string | null>(null);
	let subtree = $state<ExecutionSubtree | null>(null);

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

	const failedAll = $derived([...failedNodes]);
	const criticalAll = $derived([...criticalPath]);
	const slowEntries = $derived([...slowNodes]);

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

	const decisionAll = $derived([...decisionPoints]);

	const completedFrames = $derived(historyNodes.filter((node) => !node.error));

	const replayView = $derived.by(() => {
		if (replayIndex === null || historyNodes.length === 0) return null;
		const upto = historyNodes.slice(0, replayIndex + 1);
		return {
			executedNodes: upto.map((frame) => frame.nodeId),
			currentNode: upto.length > 0 ? (upto[upto.length - 1].nodeId ?? null) : null,
			failedNodes: upto.filter((frame) => frame.error).map((frame) => frame.nodeId),
		};
	});

	const executionOverlay = $derived(
		projectExecutionOverlay(graphNodes, {
			currentNode: replayView?.currentNode ?? execution.currentNode,
			failedNodes: [...failedAll, ...(replayView?.failedNodes ?? [])],
			criticalPath: criticalAll,
			slowNodes: slowAll,
			decisionPoints: decisionAll,
			executedNodes: replayView?.executedNodes ?? completedFrames.map((frame) => frame.nodeId),
			liveStatuses: replayView ? {} : liveStatuses,
		}),
	);

	const overlayNodes = $derived(applyExecutionOverlay(graphNodes, executionOverlay));

	const overlayEdges = $derived.by(() => {
		const tones = projectEdgeOverlay(graphEdges, executionOverlay);
		return applyEdgeOverlay(graphEdges, tones);
	});

	const pulseIds = $derived(
		[...executionOverlay.marks.values()].filter((mark) => mark.pulse).map((mark) => mark.id),
	);

	const criticalIds = $derived(
		[...executionOverlay.marks.values()].filter((mark) => mark.critical).map((mark) => mark.id),
	);

	const heatTierById = $derived(
		Object.fromEntries(
			[...executionOverlay.marks.values()]
				.filter((mark) => mark.heatTier > 0)
				.map((mark) => [mark.id, mark.heatTier]),
		),
	);

	const decisionIds = $derived(
		[...executionOverlay.marks.values()].filter((mark) => mark.decision).map((mark) => mark.id),
	);

	const heatLabels = $derived.by(() => {
		const durations = new Map(slowEntries.map((entry) => [entry.node, entry.durationMs]));
		return Object.fromEntries(
			[...executionOverlay.marks.values()]
				.filter((mark) => mark.slow)
				.map((mark) => {
					const duration = durations.get(mark.id);
					return [
						mark.id,
						typeof duration === 'number'
							? formatDuration(duration)
							: t('domain.inspector.slowNode'),
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
				branches.length > 0 ? branches.slice(0, 3).join(' / ') : t('domain.inspector.branchNode');
		}
		return labels;
	});

	/** Active replay scope: executed graph node ids plus the cursor timestamp.
	 * Tools join by exact node id; unattributed rows stay visible. */
	const replayScope = $derived.by(() => {
		if (replayIndex === null || historyNodes.length === 0) return null;
		const graphIds = new Set(graphNodes.map((node) => node.id));
		const executed = new Set(
			(replayView?.executedNodes ?? []).filter((id) => graphIds.has(id ?? '')),
		);
		const frame = historyNodes[Math.min(replayIndex, historyNodes.length - 1)];
		return {
			graphIds,
			executed,
			cutoff: frame?.endedAt || frame?.startedAt || null,
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
			(tool) => tool.name.toLowerCase().includes(needle) || tool.id.toLowerCase().includes(needle),
		);
	});

	/** Audit tool rows group like live tool rows. Expansion is local memory
	 * cleared on execution switch; with previews off the flat list renders
	 * instead. */
	const groupedTools = $derived(groupToolCalls(filteredTools));
	const groupTools = $derived(preferences.toolOutputPreviews);
	let openGroups = new SvelteSet<string>();

	function toggleGroup(key: string): void {
		if (openGroups.has(key)) openGroups.delete(key);
		else openGroups.add(key);
	}

	$effect(() => {
		void execution.id;
		openGroups.clear();
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
		const byName = matchDecisionNodeId(graphNodes, toolName);
		if (byName) return byName;
		const normalizedToolId = toolId.trim().toLowerCase();
		if (!normalizedToolId) return null;
		const exact = graphNodes.find((node) => node.id.toLowerCase() === normalizedToolId);
		if (exact) return exact.id;
		return graphNodes.find((node) => normalizedToolId.includes(node.id.toLowerCase()))?.id ?? null;
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
			toasts.info(t('domain.inspector.noGraphNodeMatch'));
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
			document.getElementById(`analysis-${kind}`)?.scrollIntoView({ block: 'nearest' }),
		);
	}

	const selectedAnalysisKind = $derived.by((): 'slow' | 'critical' | 'decision' | null => {
		if (!graphNodeId) return null;
		if (slowEntries.some((entry) => entry.node === graphNodeId)) return 'slow';
		if (criticalAll.includes(graphNodeId)) return 'critical';
		if (decisionAll.includes(graphNodeId)) return 'decision';
		return null;
	});

	function noteLiveEvent(event: EventRecord): void {
		let meta: Record<string, unknown>;
		try {
			meta = JSON.parse(event.payload || '{}') as Record<string, unknown>;
		} catch {
			return;
		}
		const node = ['node_id', 'nodeId', 'node']
			.map((key) => meta[key])
			.find((value): value is string => typeof value === 'string' && value !== '');
		const status = ['status', 'state']
			.map((key) => meta[key])
			.find((value): value is string => typeof value === 'string' && value !== '');
		if (node && status) pendingLive.set(node, status);
	}

	const isLive = $derived(isLiveStatus(execution.status));

	// Activity subscription follows the execution itself, not the visible
	// tab: timeline, tools and traces invalidate here and refetch through
	// the tab loader below, so live runs never show a stale first fetch.
	// Invalidations are debounced so high-frequency node updates refetch at
	// most once per window instead of once per event.
	let lastInvalidate = 0;
	$effect(() => {
		const id = execution.id;
		if (!id || !isLive) return;
		return watchExecutionActivity(id, {
			onEvent: (event) => {
				if (tab === 'graph') noteLiveEvent(event);
				const now = Date.now();
				if (now - lastInvalidate < 2000) return;
				lastInvalidate = now;
				if (seenTools === id) seenTools = '';
				if (seenTimeline === id) seenTimeline = '';
				if (seenTrace === id) seenTrace = '';
			},
			onTerminal: () => {
				seenTools = '';
				seenTimeline = '';
				seenTrace = '';
				seenGraph = '';
				seenAnalysis = '';
				seenState = '';
				seenHierarchy = null;
				onrefresh?.();
			},
			onState: (state) => (streamState = state),
		});
	});

	// Live frames buffer here and flush on a fixed tick, so high-frequency
	// node updates never re-render per frame. Frames arrive through the
	// activity subscription below, which stays open on every tab; the
	// overlay only consumes them while the graph tab is visible.
	$effect(() => {
		if (tab !== 'graph' || !isLive) return;
		const timer = setInterval(() => {
			if (pendingLive.size === 0) return;
			const flushed = Object.fromEntries(pendingLive);
			pendingLive.clear();
			liveStatuses = { ...liveStatuses, ...flushed };
		}, 120);
		return () => {
			clearInterval(timer);
			pendingLive.clear();
		};
	});

	// Replay playback advances the cursor on a fixed tick; colors flow
	// through the same projection bridge as live state.
	$effect(() => {
		if (!replayPlaying || historyNodes.length === 0) return;
		const timer = setInterval(
			() => {
				const last = historyNodes.length - 1;
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
			list.push({
				id: 'failed',
				label: t('domain.inspector.failedNodes'),
				ids: failedAll,
			});
		}
		if (criticalAll.length > 0) {
			list.push({
				id: 'critical',
				label: t('domain.inspector.criticalPath'),
				ids: criticalAll,
			});
		}
		if (decisionAll.length > 0) {
			list.push({
				id: 'decisions',
				label: t('domain.inspector.decisionPoints'),
				ids: decisionAll,
			});
		}
		if (slowEntries.length > 0) {
			list.push({
				id: 'slow',
				label: t('domain.inspector.slowNodes'),
				ids: [...new Set(slowEntries.map((entry) => entry.node))],
			});
		}
		if (activeOverlay === '__neighborhood') {
			list.push({
				id: '__neighborhood',
				label: t('domain.inspector.neighborhood'),
				ids: neighborhoodIds,
			});
		}
		return list;
	});

	async function loadGraph(id: string): Promise<void> {
		graphLoading = true;
		graphError = null;
		try {
			const snapshot = await loadGraphData(id);
			graphNodes = snapshot.nodes;
			graphEdges = snapshot.edges;
			failedNodes = snapshot.failedNodes;
			criticalPath = snapshot.criticalPath;
		} catch (e) {
			graphError = e instanceof Error ? e.message : t('domain.inspector.graphFailed');
		} finally {
			graphLoading = false;
		}
	}

	async function loadAnalysis(id: string): Promise<void> {
		analysisLoading = true;
		analysisError = null;
		try {
			const snapshot = await loadAnalysisData(id);
			slowNodes = snapshot.slowNodes;
			decisionPoints = snapshot.decisionPoints;
			failedNodes = snapshot.failedNodes;
			criticalPath = snapshot.criticalPath;
			efficiency = snapshot.efficiency;
		} catch (e) {
			analysisError = e instanceof Error ? e.message : t('domain.inspector.analysisFailed');
		} finally {
			analysisLoading = false;
		}
	}

	async function loadState(id: string): Promise<void> {
		stateLoading = true;
		stateError = null;
		try {
			const snapshot = await loadStateData(id);
			variables = snapshot.variables;
			historyNodes = snapshot.nodes;
		} catch (e) {
			stateError = e instanceof Error ? e.message : t('domain.inspector.stateFailed');
		} finally {
			stateLoading = false;
		}
	}

	async function loadNodeTraces(id: string): Promise<void> {
		seenTrace = id;
		nodeTracesError = null;
		nodeTracesLoading = true;
		try {
			const snapshot = await loadTraceData(id);
			nodeTraces = snapshot.items;
			nodeTracesSkipped = snapshot.skipped;
		} catch (e) {
			seenTrace = '';
			nodeTraces = [];
			nodeTracesError = e instanceof Error ? e.message : t('domain.inspector.tracesFailed');
		} finally {
			nodeTracesLoading = false;
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		try {
			const neighbors = await getExecutionGraphNeighbors(execution.id, id);
			neighborhoodIds = [id, ...neighbors.predecessors, ...neighbors.successors];
			activeOverlay = '__neighborhood';
		} catch (e) {
			toasts.error(
				t('domain.inspector.neighborhoodFailed'),
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
					toolsError = e instanceof Error ? e.message : t('domain.inspector.toolsFailed');
				});
		}
		if (tab === 'timeline' && seenTimeline !== id) {
			seenTimeline = id;
			timelineError = null;
			void getExecutionHistory(id, ['timeline'])
				.then((history) => {
					timeline = history.timeline;
				})
				.catch((e: unknown) => {
					seenTimeline = '';
					timeline = [];
					timelineError = e instanceof Error ? e.message : t('domain.inspector.timelineFailed');
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
		// The replay cursor walks history nodes, so the graph tab pulls
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
	});

	// Error-analysis stream: progressive records while the analysis tab is
	// visible. REST detail stays authoritative; the stream only appends.
	$effect(() => {
		const id = execution.id;
		if (tab !== 'analysis' || !id) return;
		liveErrors = [];
		liveErrorNotice = null;
		const stop = watchErrorAnalysis(id, {
			onRecord: (record) => {
				if (!liveErrors.some((item) => item.id === record.id)) {
					liveErrors = [...liveErrors, record].slice(-50);
				}
			},
			onError: (message) => {
				liveErrorNotice = message;
			},
		});
		return stop;
	});

	async function loadHierarchy(id: string): Promise<void> {
		hierarchyError = null;
		try {
			const snapshot = await loadHierarchyData(id);
			hierarchy = snapshot.hierarchy;
			subtree = snapshot.subtree;
		} catch (e: unknown) {
			seenHierarchy = null;
			hierarchy = null;
			subtree = null;
			hierarchyError = e instanceof Error ? e.message : t('domain.inspector.hierarchyFailed');
		}
	}

	const tone = $derived(statusTone(execution.status));
	const contextItems = $derived(variables);
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
			? (nodeTraces.find((trace) => trace.nodeId === execution.currentNode) ?? null)
			: null,
	);
	const liveMarkdown = $derived(liveTrace ? extractMarkdownText(liveTrace.output) : null);
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
			<div class="flex items-center justify-between text-micro text-muted-foreground">
				<span>{t('domain.inspector.progress')}</span>
				<span class="tabular-nums"
					>{t('domain.executionCard.tasks', {
						done: execution.tasksDone,
						total: execution.tasksTotal,
					})}</span
				>
			</div>
			<Progress value={execution.progress} tone={progressTone} />
		</div>

		<dl class="mt-3 grid grid-cols-2 gap-x-3 gap-y-2">
			<div>
				<dt class="text-micro text-muted-foreground">
					{t('domain.inspector.started')}
				</dt>
				<dd class="text-caption tabular-nums">
					{formatDateTime(execution.startedAt)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">
					{t('domain.inspector.duration')}
				</dt>
				<dd class="text-caption tabular-nums">
					{formatDuration(execution.durationMs)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">
					{t('domain.inspector.memoryPeak')}
				</dt>
				<dd class="text-caption tabular-nums">
					{formatBytes(execution.memoryPeakBytes)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">
					{t('domain.inspector.trigger')}
				</dt>
				<dd class="truncate text-caption">{execution.trigger ?? '—'}</dd>
			</div>
		</dl>
	</div>

	<Segmented items={kindTabs} bind:value={tab} size="sm" class="px-2" panelId="execution-panel" />

	<div
		id="execution-panel"
		role="tabpanel"
		aria-label={t('domain.inspector.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-3 py-3"
	>
		{#if tab === 'overview'}
			<div class="space-y-3">
				<ExecutionAnnotation executionId={execution.id} />
				<Card title={t('domain.inspector.context')}>
					<KeyValueList items={contextItems} dense />
				</Card>
				<Card title={t('domain.inspector.currentPosition')}>
					<p class="text-body">
						{execution.currentNode ?? t('domain.inspector.noActiveNode')}
					</p>
					<p class="mt-1 text-caption text-muted-foreground">
						{t('domain.inspector.failedAcrossTasks', {
							failed: execution.failedNodes,
							total: formatNumber(execution.tasksTotal),
						})}
					</p>
				</Card>
				<Card title={t('domain.inspector.liveOutput')}>
					{#if liveTrace}
						<div class="flex items-center justify-between gap-2">
							<p class="truncate font-mono text-caption text-foreground">
								{liveTrace.nodeName || liveTrace.nodeId}
							</p>
							{#if isLive}
								<span class="flex shrink-0 items-center gap-1.5 text-micro text-muted-foreground">
									<span class="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-running"></span>
									{t('domain.inspector.live')}
								</span>
							{/if}
						</div>
						<div class="mt-1.5">
							{#if liveMarkdown !== null}
								<StreamMarkdown content={liveMarkdown} done={!isLive} />
							{:else}
								<p class="text-caption break-words whitespace-pre-wrap text-foreground">
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
								onclick={() => execution.currentNode && openNodeTrace(execution.currentNode)}
							>
								{t('domain.inspector.openTrace')}
							</button>
						{/if}
					{:else if nodeTracesLoading}
						<p class="text-caption text-muted-foreground">
							{t('domain.inspector.loadingNodeOutput')}
						</p>
					{:else}
						<p class="text-caption text-muted-foreground">
							{isLive ? t('domain.inspector.liveOutputEmpty') : t('domain.inspector.openTraceHint')}
						</p>
					{/if}
				</Card>
			</div>
		{:else if tab === 'graph'}
			<div class="mb-2 flex flex-wrap items-center gap-2">
				{#if isLive}
					<span class="text-micro text-muted-foreground">
						{t('domain.inspector.liveColon', {
							state: streamState === 'open' ? t('domain.inspector.connected') : streamState,
						})}
						{Object.keys(liveStatuses).length > 0
							? `· ${t('domain.inspector.liveNodes', { count: Object.keys(liveStatuses).length })}`
							: ''}
					</span>
				{:else}
					<span class="text-micro text-muted-foreground">{t('domain.inspector.historicalRun')}</span
					>
				{/if}
				{#if kind === 'workflow' && execution.workflowId}
					<span class="mx-1 h-4 w-px bg-border"></span>
					<a
						href={`/workflows/${encodeURIComponent(execution.workflowId.replace(/^workflow:/, ''))}?tab=graph&run=${encodeURIComponent(execution.id)}`}
						class="text-micro text-foreground underline-offset-2 hover:underline"
					>
						View progress on the definition graph
					</a>
				{/if}
				{#if historyNodes.length > 0}
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
							{t('domain.inspector.startReplay')}
						</button>
					{:else}
						<button
							type="button"
							class="text-micro text-foreground underline-offset-2 hover:underline"
							onclick={() => (replayPlaying = !replayPlaying)}
						>
							{replayPlaying ? t('domain.inspector.pause') : t('domain.inspector.play')}
						</button>
						<label class="text-micro text-muted-foreground">
							{t('domain.inspector.speed')}
							<select
								bind:value={replaySpeed}
								class="ml-1 rounded border border-border bg-card text-micro text-foreground"
								aria-label={t('domain.inspector.replaySpeed')}
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
							{t('domain.inspector.stepBack')}
						</button>
						<button
							type="button"
							class="text-micro text-muted-foreground underline-offset-2 hover:underline"
							disabled={replayIndex >= historyNodes.length - 1}
							onclick={() => {
								replayPlaying = false;
								replayIndex = Math.min(historyNodes.length - 1, (replayIndex ?? -1) + 1);
							}}
						>
							{t('domain.inspector.stepForward')}
						</button>
						<input
							type="range"
							min={0}
							max={historyNodes.length - 1}
							value={replayIndex}
							oninput={(event) => {
								replayPlaying = false;
								replayIndex = Number(event.currentTarget.value);
							}}
							class="w-32 accent-current"
							aria-label={t('domain.inspector.replayPosition')}
						/>
						<span class="text-micro tabular-nums text-muted-foreground">
							{(replayIndex ?? 0) + 1}/{historyNodes.length}
						</span>
						<button
							type="button"
							class="text-micro text-muted-foreground underline-offset-2 hover:underline"
							onclick={() => {
								replayPlaying = false;
								replayIndex = null;
							}}
						>
							{t('domain.inspector.exitReplay')}
						</button>
					{/if}
				{/if}
			</div>
			{#await import('#lib/components/domain/GraphExplorer.svelte') then module}
				<module.default
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
					expandLabel={t('domain.inspector.revealNeighborhood')}
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
								{t('domain.inspector.showToolCalls')}
							</button>
							<button
								type="button"
								class="mt-1 text-micro text-foreground underline-offset-2 hover:underline"
								onclick={() => graphNodeId && openNodeTrace(graphNodeId)}
							>
								{t('domain.inspector.showNodeTrace')}
							</button>
							{#if selectedAnalysisKind}
								<button
									type="button"
									class="mt-1 text-micro text-foreground underline-offset-2 hover:underline"
									onclick={() => selectedAnalysisKind && revealInAnalysis(selectedAnalysisKind)}
								>
									{t('domain.inspector.showInAnalysis')}
								</button>
							{/if}
						{/if}
					{/snippet}
				</module.default>
			{/await}
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
					title={t('domain.inspector.timelineFailedTitle')}
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
						{t('domain.inspector.followingReplay', {
							shown: scopedTimeline.length,
							total: timeline.length,
						})}
					</p>
				{/if}
				<div class="flex items-start gap-3">
					<Timeline entries={scopedTimeline} class="min-w-0 flex-1" />
					<TimelineOutline entries={scopedTimeline} class="hidden w-44 xl:block" />
				</div>
			{/if}
		{:else if tab === 'tools'}
			{#if toolsError}
				<ErrorState
					title={t('domain.inspector.toolsFailedTitle')}
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
							{t('domain.inspector.followingReplay', {
								shown: filteredTools.length,
								total: toolCalls.length,
							})}
						</p>
					{/if}
					{#if toolFilter}
						<div class="flex items-center gap-2">
							<p class="text-micro text-muted-foreground">
								{t('domain.inspector.filteredBy', {
									filter: toolFilter,
									shown: filteredTools.length,
									total: toolCalls.length,
								})}
							</p>
							<button
								type="button"
								class="text-micro text-foreground underline-offset-2 hover:underline"
								onclick={() => (toolFilter = '')}
							>
								{t('domain.inspector.clearFilter')}
							</button>
						</div>
					{/if}
					{#snippet auditMember(entry: ToolCallEntry)}
						<div
							role="button"
							tabindex={0}
							aria-label={t('domain.inspector.locateTool', {
								name: entry.name,
							})}
							title={t('domain.inspector.clickToLocate')}
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
					{/snippet}
					{#if groupTools}
						{#each groupedTools as group (group.key)}
							{#if group.kind === 'single'}
								{#each group.members as entry (entry.id)}
									{@render auditMember(entry)}
								{/each}
							{:else}
								<ToolCallGroup
									{group}
									open={openGroups.has(group.key)}
									ontoggle={() => toggleGroup(group.key)}
									member={auditMember}
								/>
							{/if}
						{:else}
							<p class="text-caption text-muted-foreground">
								{toolCalls.length === 0
									? t('domain.inspector.noToolCalls')
									: t('domain.inspector.noToolCallsMatch')}
							</p>
						{/each}
					{:else}
						{#each filteredTools as entry (entry.id)}
							{@render auditMember(entry)}
						{:else}
							<p class="text-caption text-muted-foreground">
								{toolCalls.length === 0
									? t('domain.inspector.noToolCalls')
									: t('domain.inspector.noToolCallsMatch')}
							</p>
						{/each}
					{/if}
				</div>
			{/if}
		{:else if tab === 'analysis'}
			{#if analysisLoading}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else if analysisError}
				<ErrorState
					title={t('domain.inspector.analysisFailedTitle')}
					description={analysisError}
					onretry={() => void loadAnalysis(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-3">
					{#if liveErrors.length > 0}
						<Card title="Live error stream">
							<ul class="space-y-1.5">
								{#each liveErrors as record (record.id)}
									<li class="text-caption">
										<span class="font-mono text-destructive">{record.errorType ?? 'error'}</span>
										<span class="text-muted-foreground"> · {record.error}</span>
										{#if record.nodeId}
											<span class="font-mono text-micro text-muted-foreground">
												@{record.nodeId}</span
											>
										{/if}
									</li>
								{/each}
							</ul>
							{#if liveErrorNotice}
								<p class="mt-2 text-micro text-muted-foreground">
									{liveErrorNotice}
								</p>
							{/if}
						</Card>
					{/if}
					{#if efficiency}
						<Card title={t('domain.inspector.efficiency')}>
							<p class="text-caption text-muted-foreground">
								{t('domain.inspector.efficiencySummary', {
									executed: efficiency.executedSteps,
									optimal: efficiency.optimalSteps,
									ratio: efficiency.ratio.toFixed(2),
									wasteful: efficiency.wastefulNodes,
									retries: efficiency.retryCount,
								})}
							</p>
						</Card>
					{/if}
					<Card title={t('domain.inspector.slowNodes')}>
						<ul class="space-y-1.5" id="analysis-slow">
							{#each slowDisplay as node (node.node)}
								<li class="flex items-center justify-between gap-2 text-caption">
									<button
										type="button"
										class="truncate font-mono underline-offset-2 hover:underline"
										title={t('domain.inspector.locateOnGraph')}
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
									{t('domain.inspector.noSlowNodes')}
								</li>
							{/each}
						</ul>
					</Card>
					<Card title={t('domain.inspector.criticalPath')}>
						<ol class="flex flex-wrap items-center gap-1.5" id="analysis-critical">
							{#each criticalAll as node (node)}
								<li class="flex items-center gap-1.5">
									<button
										type="button"
										class="rounded border border-border px-1.5 py-0.5 font-mono text-micro underline-offset-2 hover:underline"
										title={t('domain.inspector.locateOnGraph')}
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
									{t('domain.inspector.noPathData')}
								</li>
							{/each}
						</ol>
					</Card>
					<Card title={t('domain.inspector.decisionPoints')}>
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
							{t('domain.inspector.failedNodesCount', {
								count: formatNumber(failedAll.length),
							})}
						</p>
					</Card>
				</div>
			{/if}
		{:else if tab === 'messages'}
			{#await import('./LoopMessagesPanel.svelte') then module}
				<module.default id={execution.id} />
			{/await}
		{:else if tab === 'variables'}
			{#await import('./LoopVariablesPanel.svelte') then module}
				<module.default id={execution.id} />
			{/await}
		{:else if tab === 'checkpoints'}
			{#await import('./LoopCheckpointsPanel.svelte') then module}
				<module.default id={execution.id} />
			{/await}
		{:else if tab === 'hierarchy'}
			{#if hierarchyError}
				<ErrorState
					title={t('domain.inspector.hierarchyFailedTitle')}
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
				<Skeleton lines={6} class="rounded-lg border border-border bg-card p-4" />
			{/if}
		{:else if tab === 'history'}
			{#key execution.id}
				{#await import('./ExecutionHistoryPanel.svelte') then module}
					<module.default executionId={execution.id} />
				{/await}
			{/key}
		{:else}
			{#if stateLoading}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else if stateError}
				<ErrorState
					title={t('domain.inspector.stateFailedTitle')}
					description={stateError}
					onretry={() => void loadState(execution.id)}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-3">
					<Card title={t('domain.history.variables')}>
						<KeyValueList items={variables} dense />
					</Card>
					<Card title="Nodes">
						<ol class="space-y-2">
							{#each historyNodes as node (node.nodeId)}
								<li
									class="flex items-start justify-between gap-2 border-b border-border/60 pb-2 last:border-0 last:pb-0"
								>
									<span class="min-w-0">
										<span class="block truncate font-mono text-caption">{node.nodeId}</span>
										<span class="text-micro text-muted-foreground">
											{node.nodeType} · {formatDuration(node.durationMs)}
										</span>
									</span>
									<StatusBadge status={node.error ? 'failed' : 'completed'} size="sm" dot={false} />
								</li>
							{:else}
								<li class="text-caption text-muted-foreground">
									{t('domain.inspector.callStackEmpty')}
								</li>
							{/each}
						</ol>
					</Card>
				</div>
			{/if}
		{/if}
	</div>
</div>
