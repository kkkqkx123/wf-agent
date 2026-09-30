import type {
	DisplayEdge,
	DisplayNode,
	GraphLayoutKind,
} from './display-model';
import { columnPositions, layeredPositions } from './layout';
import type { CanvasPosition } from './canvas-model';

/** Starting positions for the preset layouts; other kinds let Cytoscape decide. */
export function presetPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	layout: GraphLayoutKind,
): Record<string, CanvasPosition> {
	if (layout === 'columns') {
		return Object.fromEntries(columnPositions(nodes));
	}
	if (layout === 'layered') {
		return Object.fromEntries(layeredPositions(nodes, edges));
	}
	return {};
}

export function canvasLayoutOptions(
	layout: GraphLayoutKind,
): Record<string, unknown> {
	switch (layout) {
		case 'columns':
		case 'layered':
			return { name: 'preset', padding: 30, fit: true };
		case 'force':
			return {
				name: 'cose',
				padding: 30,
				animate: false,
				randomize: true,
				fit: true,
			};
		case 'grid':
			return { name: 'grid', padding: 30, fit: true, avoidOverlap: true };
		default:
			return { name: 'preset', padding: 30, fit: true };
	}
}
