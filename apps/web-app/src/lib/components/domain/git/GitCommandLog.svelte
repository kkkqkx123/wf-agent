<script lang="ts">
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let entries = $derived([...workbench.actionLog].reverse());
</script>

<TuiPanel title="Command log" count={workbench.actionLog.length}>
	<ul class="max-h-40 space-y-0.5 overflow-y-auto">
		{#each entries as entry (entry.seq)}
			<li class="flex items-baseline gap-1.5 px-1 py-px font-mono text-micro hover:bg-accent/50">
				<span class="shrink-0 font-semibold text-foreground">{entry.action}</span>
				{#if entry.summary}
					<span class="min-w-0 flex-1 truncate text-muted-foreground">{entry.summary}</span>
				{/if}
				<span class="min-w-0 flex-1 truncate text-right text-muted-foreground">{entry.result}</span>
			</li>
		{/each}
	</ul>
</TuiPanel>
