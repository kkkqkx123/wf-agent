<script lang="ts">
	import type { TimelineEntry } from '$lib/types/models';
	import StatusBadge from './StatusBadge.svelte';
	import { statusTone } from '$lib/utils/status';
	import { formatDateTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		entries: TimelineEntry[];
		class?: string;
	}

	let { entries, class: className = '' }: Props = $props();

	const TONE_DOT = {
		success: 'bg-success',
		danger: 'bg-destructive',
		running: 'bg-running',
		warning: 'bg-warning',
		info: 'bg-info',
		neutral: 'bg-muted-foreground',
	} as const;
</script>

<ol class={cn('relative space-y-3 pl-5', className)}>
	<span
		class="absolute bottom-2 left-1.5 top-2 w-px bg-border"
		aria-hidden="true"
	></span>
	{#each entries as entry (entry.id)}
		{@const tone = statusTone(entry.status)}
		<li class="relative">
			<span
				class={cn(
					'absolute -left-3.5 top-1.5 h-2.5 w-2.5 rounded-full ring-2 ring-[hsl(var(--card))]',
					TONE_DOT[tone],
					tone === 'running' && 'animate-pulse-dot',
				)}
			></span>
			<div class="flex flex-wrap items-center gap-2">
				<span class="text-body font-medium text-foreground">{entry.title}</span>
				<span
					class="rounded border border-border px-1.5 text-micro text-muted-foreground"
					>{entry.kind}</span
				>
				<StatusBadge status={entry.status} size="sm" />
			</div>
			<p class="mt-0.5 text-caption text-muted-foreground">{entry.detail}</p>
			<time
				class="mt-0.5 block text-micro tabular-nums text-muted-foreground/80"
			>
				{formatDateTime(entry.at)}
			</time>
		</li>
	{/each}
</ol>
