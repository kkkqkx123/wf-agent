<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { Snippet } from 'svelte';
	import { cn } from '../cn';

	interface Props {
		text: string;
		side?: 'top' | 'bottom' | 'left' | 'right';
		class?: string;
		children: Snippet;
	}

	let { text, side = 'top', class: className = '', children }: Props = $props();

	let trigger: HTMLElement | null = $state(null);
	let bubble: HTMLElement | null = $state(null);
	let open = $state(false);
	let position = $state({ top: 0, left: 0 });

	const GAP = 8;
	const MARGIN = 4;

	function resolvePosition(rect: DOMRect, width: number, height: number): { top: number; left: number } {
		const viewportWidth = window.innerWidth;
		const viewportHeight = window.innerHeight;
		let top: number;
		let left: number;

		if (side === 'right') {
			top = rect.top + (rect.height - height) / 2;
			left = rect.right + GAP;
			if (left + width > viewportWidth - MARGIN) {
				left = rect.left - GAP - width;
			}
		} else if (side === 'left') {
			top = rect.top + (rect.height - height) / 2;
			left = rect.left - GAP - width;
			if (left < MARGIN) {
				left = rect.right + GAP;
			}
		} else if (side === 'bottom') {
			left = rect.left + (rect.width - width) / 2;
			top = rect.bottom + GAP;
			if (top + height > viewportHeight - MARGIN) {
				top = rect.top - GAP - height;
			}
		} else {
			left = rect.left + (rect.width - width) / 2;
			top = rect.top - GAP - height;
			if (top < MARGIN) {
				top = rect.bottom + GAP;
			}
		}

		top = Math.min(Math.max(top, MARGIN), Math.max(MARGIN, viewportHeight - MARGIN - height));
		left = Math.min(Math.max(left, MARGIN), Math.max(MARGIN, viewportWidth - MARGIN - width));
		return { top, left };
	}

	function place(): void {
		if (!trigger || !bubble) return;
		const rect = trigger.getBoundingClientRect();
		const width = bubble.offsetWidth;
		const height = bubble.offsetHeight;
		position = resolvePosition(rect, width, height);
	}

	function handleWindowChange(): void {
		hide();
	}

	function show(): void {
		if (!text || open) return;
		if (typeof window === 'undefined') return;
		place();
		open = true;
		window.addEventListener('scroll', handleWindowChange, true);
		window.addEventListener('resize', handleWindowChange);
	}

	function hide(): void {
		if (!open) return;
		open = false;
		if (typeof window === 'undefined') return;
		window.removeEventListener('scroll', handleWindowChange, true);
		window.removeEventListener('resize', handleWindowChange);
	}

	onDestroy(() => {
		if (typeof window === 'undefined') return;
		window.removeEventListener('scroll', handleWindowChange, true);
		window.removeEventListener('resize', handleWindowChange);
	});

	$effect(() => {
		const node = trigger;
		if (!node) return;
		node.addEventListener('pointerenter', show);
		node.addEventListener('pointerleave', hide);
		node.addEventListener('focusin', show);
		node.addEventListener('focusout', hide);
		return () => {
			node.removeEventListener('pointerenter', show);
			node.removeEventListener('pointerleave', hide);
			node.removeEventListener('focusin', show);
			node.removeEventListener('focusout', hide);
		};
	});
</script>

<span bind:this={trigger} class={cn('inline-flex', className)}>
	{@render children()}
	<span
		bind:this={bubble}
		role="tooltip"
		style:top="{position.top}px"
		style:left="{position.left}px"
		class={cn(
			'pointer-events-none fixed z-50 whitespace-nowrap rounded-md border border-border bg-popover px-2 py-1 text-micro text-popover-foreground shadow-popover transition-opacity duration-150',
			open ? 'visible opacity-100' : 'invisible opacity-0',
		)}
	>
		{text}
	</span>
</span>
