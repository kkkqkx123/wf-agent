<script lang="ts">
	import { cn } from '#lib/ui/cn';

	interface Props {
		label: string;
		/** Keyboard nudge per arrow press, in pixels. */
		step?: number;
		disabled?: boolean;
		/** Split axis: `y` grows the region above the handle, `x` grows the region before it. */
		axis?: 'x' | 'y';
		/** Negate the delta so dragging toward the region shrinks it (right-side panes). */
		invert?: boolean;
		class?: string;
		pillClass?: string;
		/** Extra pill classes while a drag is in progress. */
		pillActiveClass?: string;
		/** Positive delta grows the target region. */
		onResize?: (delta: number) => void;
		onReset?: () => void;
	}

	let {
		label,
		step = 20,
		disabled = false,
		axis = 'y',
		invert = false,
		class: className = '',
		pillClass = '',
		pillActiveClass = '',
		onResize,
		onReset,
	}: Props = $props();

	let dragging = $state(false);
	let last = 0;

	function applyDelta(delta: number): void {
		if (delta !== 0) onResize?.(invert ? -delta : delta);
	}

	function start(event: PointerEvent): void {
		if (disabled) return;
		dragging = true;
		last = axis === 'y' ? event.clientY : event.clientX;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		event.preventDefault();
	}

	function move(event: PointerEvent): void {
		if (!dragging) return;
		const current = axis === 'y' ? event.clientY : event.clientX;
		const delta = axis === 'y' ? last - current : current - last;
		last = current;
		applyDelta(delta);
	}

	function end(): void {
		dragging = false;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (disabled) return;
		if (axis === 'y' && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
			event.preventDefault();
			applyDelta(event.key === 'ArrowUp' ? step : -step);
		} else if (axis === 'x' && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
			event.preventDefault();
			applyDelta(event.key === 'ArrowRight' ? step : -step);
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
	aria-orientation={axis === 'y' ? 'horizontal' : 'vertical'}
	aria-label={label}
	tabindex="0"
	class={cn(
		'flex items-center justify-center',
		axis === 'y' ? 'cursor-ns-resize' : 'cursor-ew-resize',
		className,
	)}
	onpointerdown={start}
	onpointermove={move}
	onpointerup={end}
	onpointercancel={end}
	ondblclick={() => onReset?.()}
	{onkeydown}
>
	<span class={cn('rounded-full transition-colors', pillClass, dragging && pillActiveClass)}></span>
</div>
