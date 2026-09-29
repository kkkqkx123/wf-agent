<script lang="ts">
	import type { HTMLTextareaAttributes } from 'svelte/elements';
	import { INPUT_BASE } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props extends HTMLTextareaAttributes {
		value?: string;
		label?: string;
		class?: string;
	}

	const generatedId = $props.id();

	let {
		value = $bindable(''),
		label,
		id = generatedId,
		class: className = '',
		...rest
	}: Props = $props();
</script>

{#snippet control()}
	<textarea
		{id}
		bind:value
		class={cn(INPUT_BASE, 'min-h-20 resize-y py-2 leading-relaxed', className)}
		{...rest}></textarea>
{/snippet}

{#if label}
	<div class="space-y-1">
		<label for={id} class="block text-caption text-muted-foreground">
			{label}
		</label>
		{@render control()}
	</div>
{:else}
	{@render control()}
{/if}
