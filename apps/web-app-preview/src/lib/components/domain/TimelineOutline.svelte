<script lang="ts">
	import type { TimelineEntry } from '$lib/types/models';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		entries: TimelineEntry[];
		class?: string;
	}

	let { entries, class: className = '' }: Props = $props();

	/** Short timelines read top to bottom; the rail only pays off past this. */
	const OUTLINE_MIN_ENTRIES = 12;

	const groups = $derived.by(() => {
		const ordered: Array<{
			kind: string;
			title: string;
			count: number;
			firstId: string;
		}> = [];
		for (const entry of entries) {
			const group = ordered.find((item) => item.kind === entry.kind);
			if (group) group.count += 1;
			else
				ordered.push({
					kind: entry.kind,
					title: entry.title || entry.kind,
					count: 1,
					firstId: entry.id,
				});
		}
		return ordered;
	});

	function jump(id: string): void {
		document
			.getElementById(`timeline-${id}`)
			?.scrollIntoView({ block: 'start', behavior: 'smooth' });
	}
</script>

{#if entries.length >= OUTLINE_MIN_ENTRIES}
	<nav aria-label="Timeline outline" class={cn('shrink-0', className)}>
		<p class="mb-1 text-micro uppercase tracking-wide text-muted-foreground">
			Outline
		</p>
		<ul class="space-y-0.5">
			{#each groups as group (group.kind)}
				<li>
					<button
						type="button"
						onclick={() => jump(group.firstId)}
						title={group.kind}
						class="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-caption text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground"
					>
						<span class="min-w-0 truncate">{group.title || 'Event'}</span>
						<span class="shrink-0 tabular-nums text-micro">{group.count}</span>
					</button>
				</li>
			{/each}
		</ul>
	</nav>
{/if}
