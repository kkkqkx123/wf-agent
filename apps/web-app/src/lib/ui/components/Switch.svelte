<script lang="ts">
	import { cn } from '../cn';

	interface Props {
		checked: boolean;
		label?: string;
		hideLabel?: boolean;
		disabled?: boolean;
		class?: string;
		onchange?: (checked: boolean) => void;
	}

	let {
		checked = $bindable(false),
		label,
		hideLabel = false,
		disabled = false,
		class: className = '',
		onchange,
	}: Props = $props();

	function toggle(): void {
		if (disabled) return;
		const next = !checked;
		checked = next;
		onchange?.(next);
	}
</script>

{#snippet control(labelled: boolean)}
	<button
		type="button"
		role="switch"
		aria-checked={checked}
		aria-label={label ?? 'Toggle setting'}
		{disabled}
		onclick={toggle}
		class={cn(
			'inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-transparent px-0.5 transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-60',
			checked ? 'bg-primary' : 'bg-input',
			!labelled && className,
		)}
	>
		<span
			class={cn(
				'pointer-events-none block h-3.5 w-3.5 rounded-full bg-background shadow-sm transition-transform duration-150',
				checked ? 'translate-x-4' : 'translate-x-0',
			)}
		></span>
	</button>
{/snippet}

{#if label && !hideLabel}
	<label class={cn('flex items-center justify-between gap-3 text-body', className)}>
		<span class="text-foreground">{label}</span>
		{@render control(true)}
	</label>
{:else}
	{@render control(false)}
{/if}
