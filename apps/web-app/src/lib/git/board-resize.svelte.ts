/** Draggable and keyboard-resizable width state for the board/main split. */
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

	private clamp(value: number): number {
		return Math.min(this.max, Math.max(this.min, Math.round(value)));
	}

	private load(): number {
		if (typeof localStorage === 'undefined') return this.default;
		try {
			const stored = localStorage.getItem(this.storageKey);
			if (stored === null) return this.default;
			const parsed = Number(stored);
			return Number.isFinite(parsed) ? this.clamp(parsed) : this.default;
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

	/** Restore the default width and drop the stored override. */
	reset(): void {
		this.width = this.default;
		this.persist(true);
	}

	/** Adjust the width by a keyboard step and persist it. */
	nudge(delta: number): void {
		this.width = this.clamp(this.width + delta);
		this.persist();
	}

	/** Pointer-drag handler for the divider; captures the pointer on the handle. */
	startDrag(event: PointerEvent): void {
		const handle = event.currentTarget as HTMLElement | null;
		if (!handle) return;
		event.preventDefault();
		handle.setPointerCapture(event.pointerId);
		const startX = event.clientX;
		const startWidth = this.width;
		const onMove = (moveEvent: PointerEvent): void => {
			this.width = this.clamp(startWidth + moveEvent.clientX - startX);
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
			this.nudge(event.key === 'ArrowRight' ? 20 : -20);
		} else if (event.key === 'Home') {
			event.preventDefault();
			this.reset();
		}
	}
}
