<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		title: string;
		description?: string;
		class?: string;
		actions?: Snippet;
		meta?: Snippet;
	}

	let {
		title,
		description,
		class: className = '',
		actions,
		meta,
	}: Props = $props();
</script>

<header
	class={cn(
		'flex flex-wrap items-start justify-between gap-3 px-4 py-3',
		className,
	)}
>
	<div class="min-w-0">
		<h1
			class="truncate text-heading font-semibold tracking-tight text-foreground"
		>
			{title}
		</h1>
		{#if description}
			<p class="mt-0.5 max-w-2xl text-caption text-muted-foreground">
				{description}
			</p>
		{/if}
		{#if meta}
			<div class="mt-1.5 flex flex-wrap items-center gap-2">
				{@render meta()}
			</div>
		{/if}
	</div>
	{#if actions}
		<div class="flex shrink-0 flex-wrap items-center gap-2">
			{@render actions()}
		</div>
	{/if}
</header>
