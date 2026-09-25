<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import { cn } from '$lib/utils/cn';

	interface Option {
		value: string;
		label: string;
		disabled?: boolean;
	}

	interface Props {
		value: string;
		options: Option[];
		size?: 'sm' | 'md';
		placeholder?: string;
		class?: string;
		onchange?: (value: string) => void;
	}

	let {
		value = $bindable(''),
		options,
		size = 'md',
		placeholder = 'Select…',
		class: className = '',
		onchange,
	}: Props = $props();
</script>

<div class={cn('relative inline-flex items-center', className)}>
	<select
		bind:value
		onchange={() => onchange?.(value)}
		class={cn(
			'w-full appearance-none rounded-md border border-input bg-card pl-2.5 pr-7 text-body text-foreground transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring',
			size === 'sm' ? 'h-7 text-small' : 'h-8.5',
		)}
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
</div>
