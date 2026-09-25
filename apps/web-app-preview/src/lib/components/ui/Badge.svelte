<script lang="ts">
	import type { Snippet } from 'svelte';
	import { badgeClass, type BadgeVariant } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props {
		variant?: BadgeVariant;
		class?: string;
		children: Snippet;
	}

	let {
		variant = 'neutral',
		class: className = '',
		children,
	}: Props = $props();
</script>

<span class={cn(badgeClass(variant), className)}>
	{@render children()}
</span>
