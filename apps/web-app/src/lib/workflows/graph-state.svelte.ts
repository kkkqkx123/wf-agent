import { getGraphAnalysis, getGraphNeighbors } from '#lib/services/graph';
import {
	getExecutionHistory,
	getUnifiedExecutionDetail,
	listUnifiedExecutions,
} from '#lib/services/executions';
import { loadAnalysisData } from '#lib/services/inspector-loaders';
import type { DefinitionExecutionSignals } from '#lib/graph/execution-projection';
import type { GraphAnalysisResult } from '#lib/types/models';
import type { GraphOverlay } from '#lib/graph/display-model';
import { toasts } from '#lib/stores/toast.svelte';

/**
 * Selection and analysis state for the workflow graph tab. Owned by the
 * route so switching away from the tab and back keeps the selected node and
 * the loaded analysis, exactly as the page-level state did before the split.
 * A detail reload (promotion, versions restore) calls reset() which clears
 * the selection and re-runs the analysis.
 *
 * The execution overlay is a read-only projection of the latest run onto
 * the definition topology: the execution domain stays the only writer of
 * run state, and this tab only reads one snapshot of it.
 */
export class WorkflowGraphState {
	selection = $state<string | null>(null);
	activeOverlay = $state<string | null>(null);
	analysis = $state<GraphAnalysisResult | null>(null);
	analysisError = $state<string | null>(null);
	neighborhoodIds = $state<string[]>([]);
	executionOverlay = $state<DefinitionExecutionSignals | null>(null);
	overlayRunId = $state<string | null>(null);
	overlayLoading = $state(false);
	overlayError = $state<string | null>(null);

	async load(id: string): Promise<void> {
		this.selection = null;
		this.activeOverlay = null;
		this.executionOverlay = null;
		this.overlayRunId = null;
		this.overlayError = null;
		await this.loadAnalysis(id);
	}

	async loadAnalysis(id: string): Promise<void> {
		this.analysis = null;
		this.analysisError = null;
		try {
			this.analysis = await getGraphAnalysis(id);
		} catch (e) {
			this.analysisError = e instanceof Error ? e.message : 'Analysis failed to load.';
		}
	}

	async expandNeighborhood(workflowId: string, id: string): Promise<void> {
		if (!workflowId) return;
		try {
			const neighbors = await getGraphNeighbors(workflowId, id);
			this.neighborhoodIds = [id, ...neighbors.predecessors, ...neighbors.successors];
			this.activeOverlay = '__neighborhood';
			toasts.success(
				`Neighborhood: ${neighbors.predecessors.length} in · ${neighbors.successors.length} out`,
			);
		} catch (e) {
			toasts.error('Neighborhood failed to load', e instanceof Error ? e.message : undefined);
		}
	}

	/**
	 * Project the latest run of one definition onto the definition graph.
	 * Read-only: unified detail and history nodes merge into overlay
	 * signals without writing back to the definition.
	 */
	async loadLatestRunOverlay(definitionId: string): Promise<void> {
		if (!definitionId || this.overlayLoading) return;
		this.overlayLoading = true;
		this.overlayError = null;
		let runId: string | null;
		try {
			const page = await listUnifiedExecutions({
				limit: 1,
				definitionId,
			});
			runId = page.items[0]?.id ?? null;
		} catch (e) {
			this.overlayError = e instanceof Error ? e.message : 'Latest run failed to load.';
			this.overlayLoading = false;
			return;
		}
		this.overlayLoading = false;
		if (!runId) {
			this.executionOverlay = null;
			this.overlayRunId = null;
			this.overlayError = 'No runs yet for this definition.';
			return;
		}
		await this.loadRunOverlay(runId);
	}

	/**
	 * Project one specific run onto the definition graph. Used for deep links
	 * from the execution domain (`?run=`), where the latest run may not be
	 * the run the user came from. Read-only like the latest-run variant.
	 */
	async loadRunOverlay(runId: string): Promise<void> {
		if (!runId || this.overlayLoading) return;
		this.overlayLoading = true;
		this.overlayError = null;
		try {
			const [unified, history, analysis] = await Promise.all([
				getUnifiedExecutionDetail(runId),
				getExecutionHistory(runId, ['nodes']).catch(() => null),
				loadAnalysisData(runId).catch(() => null),
			]);
			const record =
				typeof unified.record === 'object' && unified.record !== null
					? (unified.record as Record<string, unknown>)
					: {};
			const currentNode =
				typeof record.current_node_id === 'string' ? record.current_node_id : null;
			const historyNodes = history?.nodes ?? [];
			const failed = historyNodes.filter((node) => node.error).map((node) => node.nodeId);
			const executed = historyNodes.map((node) => node.nodeId);
			this.executionOverlay = {
				runId,
				currentNode,
				failedNodes: [...failed, ...(analysis?.failedNodes ?? [])].filter(
					(value, index, array) => array.indexOf(value) === index,
				),
				criticalPath: analysis?.criticalPath ?? [],
				slowNodes: (analysis?.slowNodes ?? []).map((entry) => ({
					node: entry.node,
					durationMs: entry.durationMs,
				})),
				decisionPoints: analysis?.decisionPoints ?? [],
				executedNodes: executed,
				liveStatuses: {},
			};
			this.overlayRunId = runId;
		} catch (e) {
			this.overlayError = e instanceof Error ? e.message : 'Run overlay failed to load.';
		} finally {
			this.overlayLoading = false;
		}
	}

	clearExecutionOverlay(): void {
		this.executionOverlay = null;
		this.overlayRunId = null;
		this.overlayError = null;
	}

	get overlays(): GraphOverlay[] {
		const analysis = this.analysis;
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
	}

	get effectiveOverlays(): GraphOverlay[] {
		if (this.activeOverlay === '__neighborhood') {
			return [
				...this.overlays,
				{
					id: '__neighborhood',
					label: 'Neighborhood',
					ids: this.neighborhoodIds,
				},
			];
		}
		return this.overlays;
	}

	jumpToNodeRow(id: string): void {
		this.selection = id;
		requestAnimationFrame(() => {
			document
				.getElementById(`wf-node-${id}`)
				?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
		});
	}
}
