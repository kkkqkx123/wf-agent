<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		text: string;
		side?: 'top' | 'bottom';
		class?: string;
		children: Snippet;
	}

	let { text, side = 'top', class: className = '', children }: Props = $props();
</script>

<span class={cn('group/tt relative inline-flex', className)}>
	{@render children()}
	<span
		role="tooltip"
		class={cn(
			'pointer-events-none absolute left-1/2 z-50 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-popover px-2 py-1 text-micro text-popover-foreground opacity-0 shadow-popover transition-opacity duration-150 group-hover/tt:opacity-100',
			side === 'top' ? 'bottom-full mb-1.5' : 'top-full mt-1.5',
		)}
	>
		{text}
	</span>
</span>
