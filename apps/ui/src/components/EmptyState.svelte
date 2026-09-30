<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '../icons/Icon.svelte';
	import type { IconName } from '../icons/paths';
	import { cn } from '../cn';

	interface Props {
		icon?: IconName;
		title: string;
		description?: string;
		/** Brand tint for the icon medallion on primary list empties. */
		tone?: 'default' | 'brand';
		class?: string;
		actions?: Snippet;
	}

	let {
		icon = 'search',
		title,
		description,
		tone = 'default',
		class: className = '',
		actions,
	}: Props = $props();
</script>

<div
	class={cn(
		'flex flex-col items-center justify-center gap-2 px-6 py-10 text-center',
		className,
	)}
>
	<div
		class={cn(
			'flex h-10 w-10 items-center justify-center rounded-full border',
			tone === 'brand'
				? 'border-brand/30 bg-brand/10 text-brand'
				: 'border-border bg-muted text-muted-foreground',
		)}
	>
		<Icon name={icon} size={18} />
	</div>
	<h3 class="text-title font-medium text-foreground">{title}</h3>
	{#if description}
		<p class="max-w-sm text-caption text-muted-foreground">{description}</p>
	{/if}
	{#if actions}
		<div class="mt-2 flex items-center gap-2">{@render actions()}</div>
	{/if}
</div>
