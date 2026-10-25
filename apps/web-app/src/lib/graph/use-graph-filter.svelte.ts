import type { DisplayEdge, DisplayNode } from './display-model';

/** Filtered graph view for one node/edge set. */
export interface FilteredGraph {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
}

/**
 * Runes filter state for graph explorers. Query matches label, id and group;
 * hidden kinds drop whole families. Edges survive only when both endpoints
 * survive, so the view never shows dangling links.
 */
export class GraphFilterState {
	query = $state('');
	hiddenKinds = $state<string[]>([]);

	filter(nodes: DisplayNode[], edges: DisplayEdge[]): FilteredGraph {
		const hidden = new Set(this.hiddenKinds);
		const needle = this.query.trim().toLowerCase();
		const keptNodes = nodes.filter((node) => {
			if (hidden.has(node.kind || 'unknown')) return false;
			if (
				needle &&
				!node.label.toLowerCase().includes(needle) &&
				!node.id.toLowerCase().includes(needle) &&
				!(node.groupId ?? '').toLowerCase().includes(needle)
			) {
				return false;
			}
			return true;
		});
		const kept = new Set(keptNodes.map((node) => node.id));
		return {
			nodes: keptNodes,
			edges: edges.filter(
				(edge) => kept.has(edge.source) && kept.has(edge.target),
			),
		};
	}

	toggleKind(kind: string): void {
		this.hiddenKinds = this.hiddenKinds.includes(kind)
			? this.hiddenKinds.filter((entry) => entry !== kind)
			: [...this.hiddenKinds, kind];
	}

	showAllKinds(): void {
		this.hiddenKinds = [];
	}

	clear(): void {
		this.query = '';
		this.hiddenKinds = [];
	}

	get active(): boolean {
		return this.query.trim() !== '' || this.hiddenKinds.length > 0;
	}
}

/** Create an isolated filter state for one graph view. */
export function useGraphFilter(): GraphFilterState {
	return new GraphFilterState();
}
