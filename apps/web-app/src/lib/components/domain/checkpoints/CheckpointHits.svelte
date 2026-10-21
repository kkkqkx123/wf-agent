<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import type { Checkpoint } from '#lib/types/models';

	let { rows }: { rows: Checkpoint[] } = $props();
</script>

{#if rows.length > 0}
	<ul class="max-h-40 space-y-1 overflow-y-auto">
		{#each rows as checkpoint (checkpoint.id)}
			<li class="flex flex-wrap items-center gap-x-2 text-micro">
				<Badge variant="neutral">{checkpoint.kind}</Badge>
				<span class="font-mono">{checkpoint.id.slice(0, 10)}</span>
				<span class="min-w-0 flex-1 truncate text-muted-foreground">
					{checkpoint.executionId}
				</span>
				<span class="shrink-0 text-muted-foreground">{checkpoint.status}</span>
			</li>
		{/each}
	</ul>
{/if}
