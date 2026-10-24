import type {
	DisplayEdge,
	DisplayNode,
	GraphLayoutKind,
} from './display-model';
import {
	columnPositions,
	compactPositions,
	layeredPositions,
	rowPositions,
} from './layout';
import type { CanvasPosition } from './canvas-model';

/** Starting positions for the preset layouts; other kinds let Cytoscape decide. */
export function presetPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	layout: GraphLayoutKind,
): Record<string, CanvasPosition> {
	if (layout === 'columns') {
		return Object.fromEntries(columnPositions(nodes, edges));
	}
	if (layout === 'rows') {
		return Object.fromEntries(rowPositions(nodes, edges));
	}
	if (layout === 'compact') {
		return Object.fromEntries(compactPositions(nodes, edges));
	}
	if (layout === 'layered') {
		return Object.fromEntries(layeredPositions(nodes, edges));
	}
	return {};
}

export function canvasLayoutOptions(): Record<string, unknown> {
	return { name: 'preset', padding: 30, fit: true };
}
