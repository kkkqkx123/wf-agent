<script lang="ts">
	import { tick } from 'svelte';
	import type { Snippet } from 'svelte';
	import { cn } from '../cn';
	import {
		resolveFloatingPosition,
		type FloatingAlign,
		type FloatingSide,
	} from '../floating';

	/**
	 * Body-teleported floating layer shared by pickers, menus and any
	 * future overlay. Teleporting out of the trigger tree keeps the panel
	 * clear of ancestor overflow and stacking contexts, while the shared
	 * resolver keeps flip and clamp behavior identical everywhere.
	 */
	export type FloatingCloseReason = 'outside' | 'escape';

	interface Props {
		/** Trigger the panel is anchored to; position follows it. */
		anchor?: HTMLElement | null;
		/** Render and position the panel while true. */
		open?: boolean;
		/** Preferred side; falls back to the opposite side on crowding. */
		side?: FloatingSide;
		/** Alignment along the anchor axis. */
		align?: FloatingAlign;
		/** Gap between the anchor and the panel, in pixels. */
		gap?: number;
		/** Minimum distance kept from the viewport edges, in pixels. */
		margin?: number;
		/** Fixed panel width (for example `18rem`); clamped to viewport. */
		width?: string;
		/** Hard ceiling for the panel height before viewport clamping. */
		maxHeight?: number;
		class?: string;
		role?: string;
		ariaLabel?: string;
		onClose?: (reason: FloatingCloseReason) => void;
		children: Snippet;
	}

	let {
		anchor = null,
		open = false,
		side = 'bottom',
		align = 'start',
		gap = 4,
		margin = 8,
		width,
		maxHeight,
		class: className = '',
		role = 'dialog',
		ariaLabel,
		onClose,
		children,
	}: Props = $props();

	let panel: HTMLElement | null = $state(null);
	let top = $state(0);
	let left = $state(0);
	let heightBudget = $state(320);
	let widthBudget = $state(320);
	let ready = $state(false);
	let frame = 0;

	/** Teleport keeps the panel outside ancestor overflow contexts. */
	function portal(node: HTMLElement): { destroy: () => void } {
		if (typeof document === 'undefined') return { destroy: () => {} };
		document.body.appendChild(node);
		return {
			destroy: () => {
				node.remove();
			},
		};
	}

	function place(): void {
		if (!open || !anchor || !panel || typeof window === 'undefined') return;
		const anchorRect = anchor.getBoundingClientRect();
		const measured = {
			width: panel.offsetWidth,
			height: panel.offsetHeight,
		};
		const viewport = { width: window.innerWidth, height: window.innerHeight };
		const resolved = resolveFloatingPosition(
			{ x: anchorRect.left, y: anchorRect.top, width: anchorRect.width, height: anchorRect.height },
			measured,
			viewport,
			{ side, align, gap, margin, maxHeight },
		);
		top = resolved.top;
		left = resolved.left;
		heightBudget = resolved.maxHeight;
		widthBudget = resolved.maxWidth;
		ready = true;
	}

	function schedulePlace(): void {
		if (typeof window === 'undefined') return;
		cancelAnimationFrame(frame);
		frame = requestAnimationFrame(() => place());
	}

	$effect(() => {
		if (open) {
			ready = false;
			void tick().then(() => place());
		} else {
			ready = false;
		}
	});

	$effect(() => {
		if (!open || typeof window === 'undefined') return;
		// Capture phase so an open panel claims Escape before ancestors
		// (for example the composer fullscreen toggle) observe it.
		const onKeydown = (event: KeyboardEvent): void => {
			if (!open) return;
			if (event.key === 'Escape') {
				event.preventDefault();
				event.stopPropagation();
				onClose?.('escape');
			}
		};
		const onPointerdown = (event: PointerEvent): void => {
			if (!open) return;
			const target = event.target as Node | null;
			if (target && (panel?.contains(target) || anchor?.contains(target))) return;
			onClose?.('outside');
		};
		const onScroll = (): void => schedulePlace();
		const onResize = (): void => schedulePlace();
		window.addEventListener('keydown', onKeydown, true);
		window.addEventListener('pointerdown', onPointerdown, true);
		window.addEventListener('scroll', onScroll, true);
		window.addEventListener('resize', onResize);
		return () => {
			cancelAnimationFrame(frame);
			window.removeEventListener('keydown', onKeydown, true);
			window.removeEventListener('pointerdown', onPointerdown, true);
			window.removeEventListener('scroll', onScroll, true);
			window.removeEventListener('resize', onResize);
		};
	});

	$effect(() => {
		if (!open || !panel || typeof ResizeObserver === 'undefined') return;
		// Content changes (filtering, async options) resize the panel, so
		// the anchor edge is re-resolved instead of drifting.
		const observer = new ResizeObserver(() => schedulePlace());
		observer.observe(panel);
		if (anchor) observer.observe(anchor);
		return () => observer.disconnect();
	});
</script>

{#if open}
	<div
		use:portal
		bind:this={panel}
		{role}
		aria-label={ariaLabel}
		data-floating-panel={side}
		class={cn(
			'animate-panel-in fixed overflow-hidden rounded-lg border border-border bg-popover text-body shadow-popover',
			className,
		)}
		style:top="{top}px"
		style:left="{left}px"
		style:width
		style:max-height="{heightBudget}px"
		style:max-width="{widthBudget}px"
		style:z-index="50"
		style:visibility={ready ? 'visible' : 'hidden'}
	>
		{@render children()}
	</div>
{/if}
