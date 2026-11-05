<script lang="ts">
	interface Props {
		/** DOM id so keyboard handlers can focus the filter (`git-filter-<panel>`). */
		id: string;
		label: string;
		placeholder: string;
		value?: string;
	}

	let { id, label, placeholder, value = $bindable('') }: Props = $props();
</script>

<input
	{id}
	type="text"
	aria-label={label}
	{placeholder}
	bind:value
	class="mb-1 w-full border border-border bg-card px-1.5 py-0.5 font-mono text-micro text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
/>
