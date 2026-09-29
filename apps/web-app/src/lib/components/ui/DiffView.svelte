<script lang="ts">
	import { cn } from '$lib/utils/cn';

	export interface DiffLine {
		type: 'add' | 'del' | 'context' | 'meta';
		text: string;
	}

	interface Props {
		lines: DiffLine[];
		title?: string;
		class?: string;
	}

	let { lines, title, class: className = '' }: Props = $props();

	const ROW_TONE = {
		add: 'bg-success/10 text-success',
		del: 'bg-destructive/10 text-destructive',
		context: 'text-foreground',
		meta: 'bg-muted text-muted-foreground',
	} as const;

	const SIGN = { add: '+', del: '-', context: ' ', meta: '@' } as const;
</script>

<div
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-card',
		className,
	)}
>
	{#if title}
		<p
			class="border-b border-border px-3 py-1.5 font-mono text-micro text-muted-foreground"
		>
			{title}
		</p>
	{/if}
	<pre
		class="max-h-80 overflow-auto text-micro leading-5">{#each lines as line, index (index)}<div
				class={cn(
					'flex gap-2 px-3 font-mono whitespace-pre',
					ROW_TONE[line.type],
				)}><span class="w-2 shrink-0 select-none opacity-70"
					>{SIGN[line.type]}</span
				><span class="min-w-0">{line.text}</span></div>{/each}</pre>
</div>
