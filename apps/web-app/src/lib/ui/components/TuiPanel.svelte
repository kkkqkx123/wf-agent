<script lang="ts">
	/**
	 * Terminal-style panel for dense operational surfaces (git, terminal).
	 * Business cards should use Card instead: Card carries rounded corners
	 * and a header/footer rhythm, while TuiPanel keeps square corners and
	 * an overlapped mono title for a terminal look.
	 */
	import type { Snippet } from 'svelte';
	import { cn } from '../cn';

	interface Props {
		title: string;
		description?: string;
		count?: number;
		focused?: boolean;
		class?: string;
		bodyClass?: string;
		actions?: Snippet;
		children: Snippet;
		footer?: Snippet;
	}

	let {
		title,
		description,
		count,
		focused = false,
		class: className = '',
		bodyClass = '',
		actions,
		children,
		footer,
	}: Props = $props();
</script>

<section
	class={cn(
		'relative rounded-none border bg-card text-card-foreground',
		focused ? 'border-info/70' : 'border-border',
		className,
	)}
>
	<span
		aria-hidden="true"
		class={cn(
			'absolute -top-2 left-2 bg-card px-1 font-mono text-micro font-semibold tracking-wider uppercase',
			focused ? 'text-info' : 'text-muted-foreground',
		)}
	>
		{title}{count !== undefined ? ` (${count})` : ''}
	</span>
	{#if actions}
		<span class="absolute -top-2.5 right-2 flex items-center gap-1 bg-card px-1">
			{@render actions()}
		</span>
	{/if}
	{#if description}
		<p class="px-2 pt-2.5 font-mono text-micro text-muted-foreground">{description}</p>
	{/if}
	<div class={cn('px-2 py-1.5', bodyClass)}>
		{@render children()}
	</div>
	{#if footer}
		<footer class="border-t border-border px-2 py-1.5 font-mono text-micro text-muted-foreground">
			{@render footer()}
		</footer>
	{/if}
</section>
