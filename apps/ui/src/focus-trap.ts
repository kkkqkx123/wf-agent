const FOCUSABLE_SELECTOR = [
	'a[href]',
	'button:not([disabled])',
	'input:not([disabled])',
	'select:not([disabled])',
	'textarea:not([disabled])',
	'[tabindex]:not([tabindex="-1"])',
].join(', ');

export interface FocusTrap {
	destroy(): void;
}

/**
 * Keeps Tab focus cycling inside an overlay for as long as it is open, then
 * hands focus back to whatever owned it before the overlay appeared.
 */
export function trapFocus(container: HTMLElement): FocusTrap {
	const previous = document.activeElement as HTMLElement | null;

	const focusable = (): HTMLElement[] =>
		Array.from(
			container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
		).filter((element) => element.offsetParent !== null);

	const initial = focusable()[0] ?? container;
	initial.focus();

	function onkeydown(event: KeyboardEvent): void {
		if (event.key !== 'Tab') return;
		const items = focusable();
		if (items.length === 0) {
			event.preventDefault();
			container.focus();
			return;
		}
		const first = items[0];
		const last = items[items.length - 1];
		const active = document.activeElement;
		if (!event.shiftKey && active === last) {
			event.preventDefault();
			first.focus();
		} else if (event.shiftKey && (active === first || active === container)) {
			event.preventDefault();
			last.focus();
		}
	}

	document.addEventListener('keydown', onkeydown, true);

	return {
		destroy(): void {
			document.removeEventListener('keydown', onkeydown, true);
			if (previous?.isConnected) previous.focus();
		},
	};
}
