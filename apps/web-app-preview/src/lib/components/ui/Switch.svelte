<script lang="ts">
	import { cn } from '$lib/utils/cn';

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
		checked = !checked;
		onchange?.(checked);
	}
</script>

{#if label && !hideLabel}
	<label
		class={cn('flex items-center justify-between gap-3 text-body', className)}
	>
		<span class="text-foreground">{label}</span>
		<button
			type="button"
			role="switch"
			aria-checked={checked}
			aria-label={label}
			{disabled}
			onclick={toggle}
			class={cn(
				'relative h-5 w-9 shrink-0 rounded-full border transition-colors duration-150 disabled:opacity-60',
				checked ? 'border-transparent bg-primary' : 'border-border bg-muted',
			)}
		>
			<span
				class={cn(
					'absolute top-0.5 h-3.5 w-3.5 rounded-full bg-card shadow-sm transition-transform duration-150',
					checked ? 'translate-x-4.5' : 'translate-x-0.5',
				)}
			></span>
		</button>
	</label>
{:else}
	<button
		type="button"
		role="switch"
		aria-checked={checked}
		aria-label={label ?? 'Toggle setting'}
		{disabled}
		onclick={toggle}
		class={cn(
			'relative h-5 w-9 shrink-0 rounded-full border transition-colors duration-150 disabled:opacity-60',
			checked ? 'border-transparent bg-primary' : 'border-border bg-muted',
			className,
		)}
	>
		<span
			class={cn(
				'absolute top-0.5 h-3.5 w-3.5 rounded-full bg-card shadow-sm transition-transform duration-150',
				checked ? 'translate-x-4.5' : 'translate-x-0.5',
			)}
		></span>
	</button>
{/if}
