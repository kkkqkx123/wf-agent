<script lang="ts">
	import { cn } from '#lib/ui/cn';

	interface Props {
		label: string;
		/** Keyboard nudge per arrow press, in pixels. */
		step?: number;
		disabled?: boolean;
		class?: string;
		pillClass?: string;
		/** Extra pill classes while a drag is in progress. */
		pillActiveClass?: string;
		/** Positive delta grows the region above the handle. */
		onResize?: (delta: number) => void;
		onReset?: () => void;
	}

	let {
		label,
		step = 20,
		disabled = false,
		class: className = '',
		pillClass = '',
		pillActiveClass = '',
		onResize,
		onReset,
	}: Props = $props();

	let dragging = $state(false);
	let lastY = 0;

	function start(event: PointerEvent): void {
		if (disabled) return;
		dragging = true;
		lastY = event.clientY;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		event.preventDefault();
	}

	function move(event: PointerEvent): void {
		if (!dragging) return;
		const delta = lastY - event.clientY;
		lastY = event.clientY;
		if (delta !== 0) onResize?.(delta);
	}

	function end(): void {
		dragging = false;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (disabled) return;
		if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
			event.preventDefault();
			onResize?.(event.key === 'ArrowUp' ? step : -step);
		} else if (event.key === 'Home' && onReset) {
			event.preventDefault();
			onReset();
		}
	}
</script>

<!-- Focusable ARIA separator: standard pattern for a resizable splitter handle -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
	role="separator"
	aria-orientation="horizontal"
	aria-label={label}
	tabindex="0"
	class={cn('flex cursor-ns-resize items-center justify-center', className)}
	onpointerdown={start}
	onpointermove={move}
	onpointerup={end}
	onpointercancel={end}
	ondblclick={() => onReset?.()}
	{onkeydown}
>
	<span class={cn('rounded-full transition-colors', pillClass, dragging && pillActiveClass)}></span>
</div>
