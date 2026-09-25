<script lang="ts">
	import type { Metric } from '$lib/types/models';
	import { cn } from '$lib/utils/cn';

	interface Props {
		metrics: Metric[];
		columns?: number;
		class?: string;
	}

	let { metrics, columns = 6, class: className = '' }: Props = $props();

	const TONE_TEXT = {
		success: 'text-success',
		danger: 'text-destructive',
		warning: 'text-warning',
		running: 'text-running',
		info: 'text-info',
		neutral: 'text-foreground',
	} as const;
</script>

<div
	class={cn('grid gap-2', className)}
	style:grid-template-columns="repeat(auto-fit, minmax(min(100%, 9rem), 1fr))"
	style:--max-columns={columns}
>
	{#each metrics as metric (metric.label)}
		<div class="rounded-lg border border-border bg-card px-3 py-2.5">
			<p class="text-micro uppercase tracking-wide text-muted-foreground">
				{metric.label}
			</p>
			<p
				class={cn(
					'mt-1 text-heading font-semibold tabular-nums',
					TONE_TEXT[metric.tone ?? 'neutral'],
				)}
			>
				{metric.value}
			</p>
			{#if metric.delta}
				<p class="mt-0.5 truncate text-micro text-muted-foreground">
					{metric.delta}
				</p>
			{/if}
		</div>
	{/each}
</div>
