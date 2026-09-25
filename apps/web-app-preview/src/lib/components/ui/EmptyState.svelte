<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import { cn } from '$lib/utils/cn';

	interface Props {
		icon?: IconName;
		title: string;
		description?: string;
		class?: string;
		actions?: Snippet;
	}

	let {
		icon = 'search',
		title,
		description,
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
		class="flex h-10 w-10 items-center justify-center rounded-full border border-border bg-muted text-muted-foreground"
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
