<script lang="ts">
	import type { HTMLInputAttributes } from 'svelte/elements';
	import { INPUT_BASE } from './variants';
	import { cn } from '../cn';

	interface Props extends Omit<HTMLInputAttributes, 'size'> {
		value?: string;
		size?: 'sm' | 'md';
		label?: string;
		class?: string;
	}

	const generatedId = $props.id();

	let {
		value = $bindable(''),
		size = 'md',
		label,
		id = generatedId,
		class: className = '',
		...rest
	}: Props = $props();
</script>

{#snippet control()}
	<input
		{id}
		bind:value
		class={cn(
			INPUT_BASE,
			size === 'sm' ? 'h-7 text-small' : 'h-8.5',
			className,
		)}
		{...rest}
	/>
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
