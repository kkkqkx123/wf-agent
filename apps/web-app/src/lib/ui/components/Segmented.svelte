<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '../cn';

	interface Item {
		id: string;
		label: string | (() => string);
		count?: number;
	}

	interface Props {
		items: Item[];
		value: string;
		size?: 'sm' | 'md';
		class?: string;
		onchange?: (id: string) => void;
		trailing?: Snippet;
		panelId?: string;
	}

	let {
		items,
		value = $bindable(''),
		size = 'md',
		class: className = '',
		onchange,
		trailing,
		panelId,
	}: Props = $props();

	function select(id: string): void {
		value = id;
		onchange?.(id);
	}

	function onKeyDown(event: KeyboardEvent): void {
		const current = items.findIndex((item) => item.id === value);
		if (current < 0) return;
		let next: number;
		if (event.key === 'ArrowRight' || event.key === 'ArrowDown')
			next = (current + 1) % items.length;
		else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp')
			next = (current - 1 + items.length) % items.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = items.length - 1;
		else return;
		event.preventDefault();
		select(items[next].id);
		document.getElementById(`segment-${items[next].id}`)?.focus();
	}
</script>

<div
	class={cn('relative flex items-center justify-between gap-2 border-b border-border', className)}
>
	<div
		role="tablist"
		aria-label="Sections"
		class="segmented-scroll flex items-center gap-1 overflow-x-auto scrollbar-none"
	>
		{#each items as item (item.id)}
			<button
				type="button"
				id={`segment-${item.id}`}
				role="tab"
				aria-selected={value === item.id}
				aria-controls={panelId}
				tabindex={value === item.id ? 0 : -1}
				onclick={() => select(item.id)}
				onkeydown={onKeyDown}
				class={cn(
					'relative inline-flex shrink-0 items-center gap-1.5 rounded-md px-2.5 font-medium transition-colors duration-150',
					size === 'sm' ? 'h-7 text-small' : 'h-8.5 text-body',
					value === item.id
						? 'bg-accent text-accent-foreground'
						: 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
				)}
			>
				<span>{typeof item.label === 'function' ? item.label() : item.label}</span>
				{#if item.count !== undefined}
					<span class="rounded-full bg-muted px-1.5 text-micro tabular-nums text-muted-foreground">
						{item.count}
					</span>
				{/if}
			</button>
		{/each}
	</div>
	<span
		aria-hidden="true"
		class="segmented-fade-left pointer-events-none absolute top-0 bottom-0 left-0 w-6"
	></span>
	<span
		aria-hidden="true"
		class="segmented-fade-right pointer-events-none absolute top-0 right-0 bottom-0 w-6"
	></span>
	{#if trailing}
		<div class="flex shrink-0 items-center gap-1 pb-1 pl-6">
			{@render trailing()}
		</div>
	{/if}
</div>

<style>
	.segmented-scroll {
		mask-image: linear-gradient(
			to right,
			transparent 0,
			black 1.25rem,
			black calc(100% - 1.25rem),
			transparent 100%
		);
	}
	.segmented-fade-left,
	.segmented-fade-right {
		opacity: 0.9;
	}
	.segmented-fade-left {
		background: linear-gradient(to right, hsl(var(--background)), hsl(var(--background) / 0));
	}
	.segmented-fade-right {
		background: linear-gradient(to left, hsl(var(--background)), hsl(var(--background) / 0));
	}
	@media (prefers-reduced-motion: reduce) {
		.segmented-scroll {
			mask-image: none;
		}
	}
</style>
