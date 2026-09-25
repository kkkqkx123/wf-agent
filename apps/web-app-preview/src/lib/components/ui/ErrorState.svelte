<script lang="ts">
	import type { Snippet } from 'svelte';
	import Button from './Button.svelte';
	import EmptyState from './EmptyState.svelte';

	interface Props {
		title: string;
		description?: string;
		class?: string;
		onretry?: () => void;
		actions?: Snippet;
	}

	let {
		title,
		description,
		class: className = '',
		onretry,
		actions: extra,
	}: Props = $props();
</script>

<EmptyState icon="alert-triangle" {title} {description} class={className}>
	{#snippet actions()}
		{#if onretry}
			<Button variant="link" size="sm" onclick={onretry}>Retry</Button>
		{/if}
		{#if extra}
			{@render extra()}
		{/if}
	{/snippet}
</EmptyState>
