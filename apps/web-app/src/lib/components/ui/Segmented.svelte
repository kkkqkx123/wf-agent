<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '$lib/utils/cn';

	interface Item {
		id: string;
		label: string;
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
	class={cn(
		'flex items-center justify-between gap-2 border-b border-border',
		className,
	)}
>
	<div
		role="tablist"
		aria-label="Sections"
		class="flex items-center gap-1 overflow-x-auto scrollbar-none"
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
					'relative inline-flex items-center gap-1.5 rounded-md px-2.5 font-medium transition-colors duration-150',
					size === 'sm' ? 'h-7 text-small' : 'h-8.5 text-body',
					value === item.id
						? 'bg-accent text-accent-foreground'
						: 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
				)}
			>
				<span>{item.label}</span>
				{#if item.count !== undefined}
					<span
						class="rounded-full bg-muted px-1.5 text-micro tabular-nums text-muted-foreground"
					>
						{item.count}
					</span>
				{/if}
			</button>
		{/each}
	</div>
	{#if trailing}
		<div class="flex shrink-0 items-center gap-1 pb-1">
			{@render trailing()}
		</div>
	{/if}
</div>
