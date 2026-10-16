import { getGraphAnalysis, getGraphNeighbors } from '$lib/services/graph';
import type { GraphAnalysisResult } from '$lib/types/models';
import type { GraphOverlay } from '$lib/graph/display-model';
import { toasts } from '$lib/stores/toast.svelte';

/**
 * Selection and analysis state for the workflow graph tab. Owned by the
 * route so switching away from the tab and back keeps the selected node and
 * the loaded analysis, exactly as the page-level state did before the split.
 * A detail reload (promotion, versions restore) calls reset() which clears
 * the selection and re-runs the analysis.
 */
export class WorkflowGraphState {
	selection = $state<string | null>(null);
	activeOverlay = $state<string | null>(null);
	analysis = $state<GraphAnalysisResult | null>(null);
	analysisError = $state<string | null>(null);
	neighborhoodIds = $state<string[]>([]);

	private loadedId = '';

	async load(id: string): Promise<void> {
		this.loadedId = id;
		this.selection = null;
		this.activeOverlay = null;
		await this.loadAnalysis(id);
	}

	async loadAnalysis(id: string): Promise<void> {
		this.analysis = null;
		this.analysisError = null;
		try {
			this.analysis = await getGraphAnalysis(id);
		} catch (e) {
			this.analysisError =
				e instanceof Error ? e.message : 'Analysis failed to load.';
		}
	}

	async expandNeighborhood(workflowId: string, id: string): Promise<void> {
		if (!workflowId) return;
		try {
			const neighbors = await getGraphNeighbors(workflowId, id);
			this.neighborhoodIds = [
				id,
				...neighbors.predecessors,
				...neighbors.successors,
			];
			this.activeOverlay = '__neighborhood';
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
