export interface TooltipPoint {
	x: number;
	y: number;
	text: string;
}

const HOVER_TIP_DELAY_MS = 350;
const HOVER_TIP_OFFSET_PX = 12;

/**
 * Hover tooltip for heat and decision labels. Shown after a short delay so
 * panning across nodes does not flicker; hidden on leave, drag, zoom,
 * pan and tap. The same text lives in the selected card, so callers keep
 * the tooltip hidden from assistive technology.
 */
export class HoverTip {
	current = $state<TooltipPoint | null>(null);
	private timer: ReturnType<typeof setTimeout> | null = null;

	request(
		id: string,
		clientX: number,
		clientY: number,
		labels: Record<string, string>,
		rect: DOMRect | null,
	): void {
		this.clear();
		const text = labels[id];
		if (!text || !rect) return;
		this.timer = setTimeout(() => {
			this.current = {
				x: clientX - rect.left + HOVER_TIP_OFFSET_PX,
				y: clientY - rect.top + HOVER_TIP_OFFSET_PX,
				text,
			};
			this.timer = null;
		}, HOVER_TIP_DELAY_MS);
	}

	clear(): void {
		if (this.timer !== null) {
			clearTimeout(this.timer);
			this.timer = null;
		}
		this.current = null;
	}
}
