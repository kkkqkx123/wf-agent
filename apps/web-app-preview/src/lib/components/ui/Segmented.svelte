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
	}

	let {
		items,
		value = $bindable(''),
		size = 'md',
		class: className = '',
		onchange,
		trailing,
	}: Props = $props();

	function select(id: string): void {
		value = id;
		onchange?.(id);
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
		class="flex items-center gap-1 overflow-x-auto scrollbar-none"
	>
		{#each items as item (item.id)}
			<button
				type="button"
				role="tab"
				aria-selected={value === item.id}
				onclick={() => select(item.id)}
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
