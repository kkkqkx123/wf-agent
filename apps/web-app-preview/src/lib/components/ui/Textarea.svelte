<script lang="ts">
	import type { HTMLTextareaAttributes } from 'svelte/elements';
	import { INPUT_BASE } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props extends HTMLTextareaAttributes {
		value?: string;
		class?: string;
	}

	let {
		value = $bindable(''),
		class: className = '',
		...rest
	}: Props = $props();
</script>

<textarea
	bind:value
	class={cn(INPUT_BASE, 'min-h-20 resize-y py-2 leading-relaxed', className)}
	{...rest}></textarea>
