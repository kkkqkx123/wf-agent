<script lang="ts">
	import { cn } from '$lib/utils/cn';

	interface Props {
		value: number;
		tone?: 'default' | 'success' | 'danger' | 'warning' | 'running';
		showLabel?: boolean;
		class?: string;
	}

	let {
		value,
		tone = 'default',
		showLabel = false,
		class: className = '',
	}: Props = $props();

	const clamped = $derived(
		Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0)),
	);
	const percent = $derived(Math.round(clamped * 100));

	const TONE_BAR = {
		default: 'bg-primary',
		success: 'bg-success',
		danger: 'bg-destructive',
		warning: 'bg-warning',
		running: 'bg-running',
	} as const;
</script>

<div class={cn('flex items-center gap-2', className)}>
	<div
		class="h-1.5 w-full overflow-hidden rounded-full bg-muted"
		role="progressbar"
		aria-valuenow={percent}
		aria-valuemin={0}
		aria-valuemax={100}
	>
		<div
			class={cn(
				'h-full rounded-full transition-[width] duration-300',
				TONE_BAR[tone],
			)}
			style:width="{percent}%"
		></div>
	</div>
	{#if showLabel}
		<span
			class="w-9 shrink-0 text-right text-micro tabular-nums text-muted-foreground"
			>{percent}%</span
		>
	{/if}
</div>
