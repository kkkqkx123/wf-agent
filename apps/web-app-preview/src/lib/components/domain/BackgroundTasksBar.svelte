<script lang="ts">
	import { chatStream } from '$lib/stores/stream-run.svelte';

	const backgroundTools = $derived(
		chatStream.tools.filter((tool) => tool.background?.sessionId),
	);

	function jumpTo(toolId: string): void {
		document
			.getElementById(`live-tool-${toolId}`)
			?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
	}
</script>

{#if backgroundTools.length > 0}
	<section
		aria-label="Background tasks"
		class="rounded-lg border border-border bg-card px-3 py-2"
	>
		<p class="text-micro uppercase tracking-wide text-muted-foreground">
			Background · {backgroundTools.length} running
		</p>
		<ul class="mt-1 space-y-1">
			{#each backgroundTools as tool (tool.id)}
				<li class="flex min-w-0 items-center gap-2">
					<span
						class="min-w-0 flex-1 truncate font-mono text-micro text-foreground"
						title={tool.background?.sessionId ?? ''}
					>
						{tool.name} · {tool.background?.sessionId}
					</span>
					<button
						type="button"
						onclick={() => jumpTo(tool.id)}
						class="shrink-0 text-micro text-muted-foreground transition-colors hover:text-foreground"
					>
						Jump
					</button>
				</li>
			{/each}
		</ul>
		<p class="mt-1 font-mono text-micro text-muted-foreground">
			Follow up with shell_output or shell_wait, confirm with shell_ack. Offline
			clients re-query shell_tasks after reconnect.
		</p>
	</section>
{/if}
