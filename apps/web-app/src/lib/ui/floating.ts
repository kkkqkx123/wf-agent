/**
 * Shared viewport-aware positioning for floating layers (picker panels,
 * menus, tooltips). Every overlay that escapes an overflow or stacking
 * context resolves through this module so flip and clamp behavior stays
 * identical across the product.
 */

export type FloatingSide = 'top' | 'bottom' | 'left' | 'right';
export type FloatingAlign = 'start' | 'center' | 'end';

export interface FloatingRect {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface FloatingSize {
	width: number;
	height: number;
}

export interface ViewportSize {
	width: number;
	height: number;
}

export interface FloatingOptions {
	/** Preferred side of the anchor; falls back to the opposite side. */
	side?: FloatingSide;
	/** Alignment along the anchor axis. */
	align?: FloatingAlign;
	/** Gap between the anchor and the panel, in pixels. */
	gap?: number;
	/** Minimum distance kept from the viewport edges, in pixels. */
	margin?: number;
	/** Hard ceiling for the panel height before viewport clamping. */
	maxHeight?: number;
	/** Hard ceiling for the panel width before viewport clamping. */
	maxWidth?: number;
}

export interface FloatingPlacement {
	top: number;
	left: number;
	/** Side the panel actually landed on after flip resolution. */
	side: FloatingSide;
	/** Height budget the panel must fit inside (caller applies as max-height). */
	maxHeight: number;
	/** Width budget the panel must fit inside (caller applies as max-width). */
	maxWidth: number;
}

function oppositeSide(side: FloatingSide): FloatingSide {
	if (side === 'top') return 'bottom';
	if (side === 'bottom') return 'top';
	if (side === 'left') return 'right';
	return 'left';
}

function spaceOn(
	side: FloatingSide,
	anchor: FloatingRect,
	viewport: ViewportSize,
	gap: number,
	margin: number,
): number {
	if (side === 'top') return Math.max(0, anchor.y - gap - margin);
	if (side === 'bottom')
		return Math.max(0, viewport.height - (anchor.y + anchor.height) - gap - margin);
	if (side === 'left') return Math.max(0, anchor.x - gap - margin);
	return Math.max(0, viewport.width - (anchor.x + anchor.width) - gap - margin);
}

/**
 * Resolve a fixed position for a panel anchored to a rectangle. The
 * preferred side wins when it fits; otherwise the opposite side wins when
 * it offers more room. Both axes are clamped into the viewport so the
 * panel never renders off-screen.
 */
export function resolveFloatingPosition(
	anchor: FloatingRect,
	panel: FloatingSize,
	viewport: ViewportSize,
	options: FloatingOptions = {},
): FloatingPlacement {
	const side = options.side ?? 'bottom';
	const align = options.align ?? 'start';
	const gap = options.gap ?? 8;
	const margin = options.margin ?? 8;

	const heightBudget = Math.min(
		panel.height,
		options.maxHeight ?? panel.height,
		Math.max(0, viewport.height - margin * 2),
	);
	const widthBudget = Math.min(
		panel.width,
		options.maxWidth ?? panel.width,
		Math.max(0, viewport.width - margin * 2),
	);

	const vertical = side === 'top' || side === 'bottom';
	const crossBudget = vertical ? widthBudget : heightBudget;
	const crossViewport = vertical ? viewport.width : viewport.height;
	const crossAnchorStart = vertical ? anchor.x : anchor.y;
	const crossAnchorSize = vertical ? anchor.width : anchor.height;

	let cross =
		align === 'center'
			? crossAnchorStart + (crossAnchorSize - crossBudget) / 2
			: align === 'end'
				? crossAnchorStart + crossAnchorSize - crossBudget
				: crossAnchorStart;
	cross = Math.min(Math.max(cross, margin), Math.max(margin, crossViewport - margin - crossBudget));

	const flipped = oppositeSide(side);
	const roomPreferred = spaceOn(side, anchor, viewport, gap, margin);
	const roomFlipped = spaceOn(flipped, anchor, viewport, gap, margin);
	const mainBudget = vertical ? heightBudget : widthBudget;
	const resolvedSide = roomPreferred >= mainBudget || roomPreferred >= roomFlipped ? side : flipped;

	let main: number;
	if (resolvedSide === 'top') {
		const available = Math.max(0, anchor.y - gap - margin);
		main = anchor.y - gap - Math.min(mainBudget, available);
	} else if (resolvedSide === 'bottom') {
		main = anchor.y + anchor.height + gap;
	} else if (resolvedSide === 'left') {
		const available = Math.max(0, anchor.x - gap - margin);
		main = anchor.x - gap - Math.min(mainBudget, available);
	} else {
		main = anchor.x + anchor.width + gap;
	}

	const mainViewport = vertical ? viewport.height : viewport.width;
	main = Math.min(Math.max(main, margin), Math.max(margin, mainViewport - margin - mainBudget));
	const clampedMain = Math.min(mainBudget, Math.max(0, mainViewport - margin * 2));

	if (vertical) {
		return {
			top: Math.round(main),
			left: Math.round(cross),
			side: resolvedSide,
			maxHeight: Math.round(clampedMain),
			maxWidth: Math.round(widthBudget),
		};
	}
	return {
		top: Math.round(cross),
		left: Math.round(main),
		side: resolvedSide,
		maxHeight: Math.round(heightBudget),
		maxWidth: Math.round(clampedMain),
	};
}
