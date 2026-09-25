<script lang="ts">
	import type { Snippet } from 'svelte';
	import { badgeClass, type BadgeSize, type BadgeVariant } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props {
		variant?: BadgeVariant;
		size?: BadgeSize;
		class?: string;
		children: Snippet;
	}

	let {
		variant = 'neutral',
		size = 'md',
		class: className = '',
		children,
	}: Props = $props();
</script>

<span class={cn(badgeClass(variant, size), className)}>
	{@render children()}
</span>
