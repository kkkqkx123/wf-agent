<script lang="ts">
	import { onMount } from 'svelte';
	import { cn } from '$lib/utils/cn';

	export interface ContextMenuItem {
		id: string;
		label: string;
		disabled?: boolean;
		danger?: boolean;
	}

	interface Props {
		x: number;
		y: number;
		title: string;
		items: ContextMenuItem[];
		onaction: (id: string) => void;
		onclose: () => void;
	}

	let { x, y, title, items, onaction, onclose }: Props = $props();

	// Focus follows the current item list; hover and arrow keys override it.
	// The menu remounts on every open, so no effect is needed to resync.
	const firstEnabled = $derived(items.findIndex((item) => !item.disabled));
	let touched = $state(false);
	let hovered = $state(-1);
	const focused = $derived(touched ? hovered : firstEnabled);
	let menu: HTMLDivElement | null = $state(null);
	onMount(() => {
		menu?.focus();
	});

	function point(delta: number): void {
		const base = focused >= 0 ? focused : 0;
		let next = base;
		for (let step = 0; step < items.length; step += 1) {
			next = (next + delta + items.length) % items.length;
			if (!items[next]?.disabled) break;
		}
		hovered = next;
		touched = true;
	}

	function onKey(event: KeyboardEvent): void {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			point(1);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			point(-1);
		}
	}

	function hover(index: number): void {
		hovered = index;
		touched = true;
	}
</script>

<button
	type="button"
	aria-label="Close menu"
	class="fixed inset-0 z-40 cursor-default bg-transparent"
	onclick={onclose}
></button>
<div
	bind:this={menu}
	role="menu"
	aria-label={title}
	tabindex={-1}
	class="fixed z-50 w-52 rounded-lg border border-border bg-card p-1 shadow-lg outline-none"
	style:left={`${Math.max(4, Math.min(x, window.innerWidth - 216))}px`}
	style:top={`${Math.max(4, Math.min(y, window.innerHeight - 40 - items.length * 32))}px`}
	onkeydown={onKey}
>
	<p class="px-2 py-1 text-micro text-muted-foreground">{title}</p>
	{#each items as item, index (item.id)}
		<button
			type="button"
			role="menuitem"
			disabled={item.disabled}
			class={cn(
				'flex w-full items-center rounded-md px-2 py-1.5 text-left text-caption',
				item.danger ? 'text-destructive' : 'text-foreground',
				!item.disabled && focused === index && 'bg-accent',
				item.disabled && 'cursor-not-allowed opacity-40',
			)}
			onmouseenter={() => {
				if (!item.disabled) hover(index);
			}}
			onfocus={() => {
				if (!item.disabled) hover(index);
			}}
			onclick={() => onaction(item.id)}
		>
			{item.label}
		</button>
	{/each}
</div>
