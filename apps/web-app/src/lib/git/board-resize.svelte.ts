/** Draggable and keyboard-resizable width state for the board/main split. Width is a proportion of the container width, so the sidebar keeps a constant share when the window resizes. */
export class BoardResize {
	readonly default: number;
	readonly min: number;
	readonly max: number;
	width = $state(0);
	private readonly storageKey: string;

	constructor(options: { storageKey: string; default: number; min: number; max: number }) {
		this.storageKey = options.storageKey;
		this.default = options.default;
		this.min = options.min;
		this.max = options.max;
		this.width = this.load();
	}

	/** CSS value for the board width variable (percentage of the container). */
	get cssValue(): string {
		return `${(this.width * 100).toFixed(3)}%`;
	}

	private clamp(value: number): number {
		return Math.min(this.max, Math.max(this.min, value));
	}

	private load(): number {
		if (typeof localStorage === 'undefined') return this.default;
		try {
			const stored = localStorage.getItem(this.storageKey);
			if (stored === null) return this.default;
			const parsed = Number(stored);
			if (!Number.isFinite(parsed)) return this.default;
			// Legacy pixel values predate the proportional layout; discard them.
			if (parsed <= 0 || parsed > 1) return this.default;
			return this.clamp(parsed);
		} catch {
			return this.default;
		}
	}

	private persist(reset = false): void {
		try {
			if (reset) localStorage.removeItem(this.storageKey);
			else localStorage.setItem(this.storageKey, String(this.width));
		} catch {
			// Storage unavailable; keep the in-memory width.
		}
	}

	/** Restore the default proportion and drop the stored override. */
	reset(): void {
		this.width = this.default;
		this.persist(true);
	}

	/** Adjust the proportion by a ratio delta and persist it. */
	nudge(delta: number): void {
		this.width = this.clamp(this.width + delta);
		this.persist();
	}

	/** Pointer-drag handler for the divider; converts pixel movement to a ratio. */
	startDrag(event: PointerEvent): void {
		const handle = event.currentTarget as HTMLElement | null;
		if (!handle) return;
		event.preventDefault();
		handle.setPointerCapture(event.pointerId);
		const containerWidth =
			handle.parentElement?.getBoundingClientRect().width ?? window.innerWidth;
		if (!(containerWidth > 0)) return;
		const startX = event.clientX;
		const startWidth = this.width;
		const onMove = (moveEvent: PointerEvent): void => {
			this.width = this.clamp(startWidth + (moveEvent.clientX - startX) / containerWidth);
		};
		const onUp = (upEvent: PointerEvent): void => {
			this.persist();
			handle.releasePointerCapture(upEvent.pointerId);
			handle.removeEventListener('pointermove', onMove);
			handle.removeEventListener('pointerup', onUp);
			handle.removeEventListener('pointercancel', onUp);
		};
		handle.addEventListener('pointermove', onMove);
		handle.addEventListener('pointerup', onUp);
		handle.addEventListener('pointercancel', onUp);
	}

	/** Divider keyboard support: arrow keys resize, Home restores the default. */
	onKeydown(event: KeyboardEvent): void {
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			this.nudge(event.key === 'ArrowRight' ? 0.02 : -0.02);
		} else if (event.key === 'Home') {
			event.preventDefault();
			this.reset();
		}
	}
}
