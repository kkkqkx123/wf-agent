/** Shared geometry types for the graph canvas and its logic modules. */

export interface CanvasPosition {
	x: number;
	y: number;
}

export interface CanvasMove {
	id: string;
	position: CanvasPosition;
}

export interface CanvasContext {
	kind: 'node' | 'edge' | 'blank';
	id: string | null;
	x: number;
	y: number;
}

export interface MiniBounds {
	minX: number;
	minY: number;
	w: number;
	h: number;
}

export interface MiniOverview {
	items: Array<{ id: string; x: number; y: number }>;
	boxes: Array<{
		id: string;
		x1: number;
		y1: number;
		x2: number;
		y2: number;
	}>;
	bounds: MiniBounds;
	view: { x1: number; y1: number; x2: number; y2: number };
}

export interface ConnectSpot {
	id: string;
	x: number;
	y: number;
}

export interface ConnectDrag {
	source: string;
	sx: number;
	sy: number;
	px: number;
	py: number;
	target: string | null;
}
