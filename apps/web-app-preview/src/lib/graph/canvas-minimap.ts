import type { MiniBounds, MiniOverview } from './canvas-model';

export const MINIMAP_AUTO_THRESHOLD = 100;
export const MINI_W = 148;
export const MINI_H = 104;

export interface MiniItem {
	id: string;
	x: number;
	y: number;
}

export interface MiniBox {
	id: string;
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

export interface MiniExtent {
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

/**
 * Snapshot node dots, group outlines plus the viewport for the minimap.
 * Structure only: execution colors stay on the main canvas so hot
 * updates never redraw the overview.
 */
export function computeMiniOverview(
	items: MiniItem[],
	boxes: MiniBox[],
	extent: MiniExtent,
): MiniOverview | null {
	if (items.length === 0 && boxes.length === 0) return null;
	let minX = Number.POSITIVE_INFINITY;
	let minY = Number.POSITIVE_INFINITY;
	let maxX = Number.NEGATIVE_INFINITY;
	let maxY = Number.NEGATIVE_INFINITY;
	for (const item of items) {
		minX = Math.min(minX, item.x);
		minY = Math.min(minY, item.y);
		maxX = Math.max(maxX, item.x);
		maxY = Math.max(maxY, item.y);
	}
	for (const box of boxes) {
		minX = Math.min(minX, box.x1);
		minY = Math.min(minY, box.y1);
		maxX = Math.max(maxX, box.x2);
		maxY = Math.max(maxY, box.y2);
	}
	const pad = 60;
	minX -= pad;
	minY -= pad;
	maxX += pad;
	maxY += pad;
	return {
		items,
		boxes,
		bounds: {
			minX,
			minY,
			w: maxX - minX || 1,
			h: maxY - minY || 1,
		},
		view: { x1: extent.x1, y1: extent.y1, x2: extent.x2, y2: extent.y2 },
	};
}

/** World coordinates projected onto the minimap surface. */
export function projectToMini(
	x: number,
	y: number,
	bounds: MiniBounds,
): { x: number; y: number } {
	return {
		x: ((x - bounds.minX) / bounds.w) * MINI_W,
		y: ((y - bounds.minY) / bounds.h) * MINI_H,
	};
}

/** Pointer position over the minimap translated back to world coordinates. */
export function miniPointerToWorld(
	clientX: number,
	clientY: number,
	rect: { left: number; top: number; width: number; height: number },
	bounds: MiniBounds | undefined,
): { x: number; y: number } {
	if (!bounds || rect.width === 0 || rect.height === 0) return { x: 0, y: 0 };
	return {
		x: bounds.minX + ((clientX - rect.left) / rect.width) * bounds.w,
		y: bounds.minY + ((clientY - rect.top) / rect.height) * bounds.h,
	};
}
