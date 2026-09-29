<script lang="ts">
	import type { HTMLSelectAttributes } from 'svelte/elements';
	import Icon from '$lib/components/icons/Icon.svelte';
	import { cn } from '$lib/utils/cn';

	interface Option {
		value: string;
		label: string;
		disabled?: boolean;
	}

	interface Props extends Omit<
		HTMLSelectAttributes,
		'size' | 'children' | 'value' | 'onchange'
	> {
		value: string;
		options: Option[];
		size?: 'sm' | 'md';
		placeholder?: string;
		label?: string;
		class?: string;
		onchange?: (value: string) => void;
	}

	const generatedId = $props.id();

	let {
		value = $bindable(''),
		options,
		size = 'md',
		placeholder = 'Select…',
		label,
		id = generatedId,
		class: className = '',
		onchange,
		...rest
	}: Props = $props();
</script>

{#snippet control()}
	<select
		{id}
		bind:value
		onchange={() => onchange?.(value)}
		class={cn(
			'w-full appearance-none rounded-md border border-input bg-card pl-2.5 pr-7 text-body text-foreground transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))] disabled:cursor-not-allowed disabled:opacity-60',
			size === 'sm' ? 'h-7 text-small' : 'h-8.5',
		)}
		{...rest}
	>
		{#if placeholder}
			<option value="" disabled>{placeholder}</option>
		{/if}
		{#each options as option (option.value)}
			<option value={option.value} disabled={option.disabled}
				>{option.label}</option
			>
		{/each}
	</select>
	<Icon
		name="chevron-down"
		size={14}
		class="pointer-events-none absolute right-2 text-muted-foreground"
	/>
{/snippet}

{#if label}
	<div class={cn('space-y-1', className)}>
		<label for={id} class="block text-caption text-muted-foreground">
			{label}
		</label>
		<div class="relative inline-flex w-full items-center">
			{@render control()}
		</div>
	</div>
{:else}
	<!-- Unlabelled selects keep the original inline-flex wrapper so existing
	     width classes keep sizing the control itself. -->
	<div class={cn('relative inline-flex items-center', className)}>
		{@render control()}
	</div>
{/if}
