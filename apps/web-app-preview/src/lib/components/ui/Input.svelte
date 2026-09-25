<script lang="ts">
	import type { HTMLInputAttributes } from 'svelte/elements';
	import { INPUT_BASE } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props extends Omit<HTMLInputAttributes, 'size'> {
		value?: string;
		size?: 'sm' | 'md';
		class?: string;
	}

	let {
		value = $bindable(''),
		size = 'md',
		class: className = '',
		...rest
	}: Props = $props();
</script>

<input
	bind:value
	class={cn(INPUT_BASE, size === 'sm' ? 'h-7 text-small' : 'h-8.5', className)}
	{...rest}
/>
