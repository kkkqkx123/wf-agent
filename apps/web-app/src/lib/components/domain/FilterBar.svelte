<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		query: string;
		status: string;
		statusOptions: Array<{ value: string; label: string }>;
		placeholder?: string;
		class?: string;
		trailing?: Snippet;
	}

	let {
		query = $bindable(''),
		status = $bindable(''),
		statusOptions,
		placeholder = 'Filter…',
		class: className = '',
		trailing,
	}: Props = $props();
</script>

<div class={cn('flex flex-wrap items-center gap-2', className)}>
	<div class="relative min-w-48 flex-1">
		<Icon
			name="search"
			size={14}
			class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
		/>
		<Input bind:value={query} {placeholder} class="pl-8" />
	</div>
	<Select
		bind:value={status}
		options={statusOptions}
		placeholder="All statuses"
		class="w-40"
	/>
	{#if trailing}
		<div class="flex items-center gap-2">{@render trailing()}</div>
	{/if}
</div>
