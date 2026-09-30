import type { CanvasMove, CanvasPosition } from './canvas-model';
import type { GroupTitle } from './group-view';

export function roundPosition(position: CanvasPosition): CanvasPosition {
	return { x: Math.round(position.x), y: Math.round(position.y) };
}

/**
 * Moves for a collapsed-title drag: the title plus every member shifted
 * by the same delta. Members without a stored position are stacked under
 * the new title position instead of jumping to the origin.
 */
export function titleDragMoves(
	titleId: string,
	next: CanvasPosition,
	grabStart: ReadonlyMap<string, CanvasPosition>,
	positions: Record<string, CanvasPosition> | undefined,
	groupTitles: Record<string, GroupTitle>,
): CanvasMove[] {
	const title = groupTitles[titleId];
	const start = grabStart.get(titleId) ?? positions?.[titleId] ?? next;
	const delta = { x: next.x - start.x, y: next.y - start.y };
	const moves: CanvasMove[] = [{ id: titleId, position: next }];
	(title?.memberIds ?? []).forEach((memberId, index) => {
		const base = positions?.[memberId];
		moves.push({
			id: memberId,
			position: base
				? { x: base.x + delta.x, y: base.y + delta.y }
				: {
						x: next.x - 120 + (index % 4) * 80,
						y: next.y + 60 + Math.floor(index / 4) * 60,
					},
		});
	});
	return moves;
}
