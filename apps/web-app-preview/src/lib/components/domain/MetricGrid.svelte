<script lang="ts">
	import { toneText } from '@wf-agent/ui/components/variants';
	import type { Metric } from '$lib/types/models';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		metrics: Metric[];
		columns?: number;
		class?: string;
	}

	let { metrics, columns = 6, class: className = '' }: Props = $props();

	// A metric value without an explicit tone keeps full foreground strength.
	const NEUTRAL_TEXT = 'text-foreground';
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
					toneText(metric.tone, NEUTRAL_TEXT),
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
