<script lang="ts">
	import type { Snippet } from 'svelte';
	import { SURFACE_PANEL } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props {
		title?: string;
		description?: string;
		class?: string;
		bodyClass?: string;
		interactive?: boolean;
		actions?: Snippet;
		children: Snippet;
		footer?: Snippet;
	}

	let {
		title,
		description,
		class: className = '',
		bodyClass = '',
		interactive = false,
		actions,
		children,
		footer,
	}: Props = $props();
</script>

<section
	class={cn(
		SURFACE_PANEL,
		interactive && 'transition-colors hover:border-ring/40',
		className,
	)}
>
	{#if title || actions}
		<header
			class="flex items-start justify-between gap-3 border-b border-border px-4 py-2.5"
		>
			<div class="min-w-0">
				{#if title}
					<h3 class="truncate text-title font-semibold text-foreground">
						{title}
					</h3>
				{/if}
				{#if description}
					<p class="mt-0.5 text-caption text-muted-foreground">{description}</p>
				{/if}
			</div>
			{#if actions}
				<div class="flex shrink-0 items-center gap-1">{@render actions()}</div>
			{/if}
		</header>
	{/if}
	<div class={cn('px-4 py-3', bodyClass)}>
		{@render children()}
	</div>
	{#if footer}
		<footer
			class="border-t border-border px-4 py-2.5 text-caption text-muted-foreground"
		>
			{@render footer()}
		</footer>
	{/if}
</section>
