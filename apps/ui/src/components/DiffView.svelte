<script lang="ts">
	import { cn } from '../cn';

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

	/** Unified (one column) or side-by-side (old | new) layout. */
	let sideBySide = $state(false);
	/** Render spaces/tabs visibly instead of raw. */
	let showWhitespace = $state(false);

	const ROW_TONE = {
		add: 'bg-success/10 text-success',
		del: 'bg-destructive/10 text-destructive',
		context: 'text-foreground',
		meta: 'bg-muted text-muted-foreground',
	} as const;

	const SIGN = { add: '+', del: '-', context: ' ', meta: '@' } as const;

	function visibleText(text: string): string {
		if (!showWhitespace) return text;
		return text.replace(/ /g, '·').replace(/\t/g, '→\t');
	}

	interface PairedRow {
		left: DiffLine | null;
		right: DiffLine | null;
		full: DiffLine | null;
	}

	/**
	 * Pair consecutive del/add runs into old|new rows so side-by-side
	 * stays aligned. Context and meta lines span both columns.
	 */
	function pairedRows(input: DiffLine[]): PairedRow[] {
		const rows: PairedRow[] = [];
		let index = 0;
		while (index < input.length) {
			const line = input[index];
			if (line.type === 'del') {
				const dels: DiffLine[] = [];
				while (index < input.length && input[index].type === 'del') {
					dels.push(input[index]);
					index += 1;
				}
				const adds: DiffLine[] = [];
				while (index < input.length && input[index].type === 'add') {
					adds.push(input[index]);
					index += 1;
				}
				const width = Math.max(dels.length, adds.length);
				for (let row = 0; row < width; row += 1) {
					rows.push({
						left: dels[row] ?? null,
						right: adds[row] ?? null,
						full: null,
					});
				}
			} else if (line.type === 'add') {
				rows.push({ left: null, right: line, full: null });
				index += 1;
			} else {
				rows.push({ left: null, right: null, full: line });
				index += 1;
			}
		}
		return rows;
	}

	const rows = $derived(pairedRows(lines));
</script>

<div
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-card',
		className,
	)}
>
	<div
		class="flex flex-wrap items-center gap-2 border-b border-border px-3 py-1.5"
	>
		{#if title}
			<p
				class="min-w-0 flex-1 truncate font-mono text-micro text-muted-foreground"
			>
				{title}
			</p>
		{/if}
		<div
			class="ml-auto flex shrink-0 items-center gap-1"
			role="group"
			aria-label="Diff view options"
		>
			<button
				type="button"
				aria-pressed={sideBySide}
				title={sideBySide ? 'Unified view' : 'Side-by-side view'}
				class={cn(
					'rounded px-1.5 py-0.5 font-mono text-micro transition-colors hover:bg-accent',
					sideBySide ? 'bg-accent text-foreground' : 'text-muted-foreground',
				)}
				onclick={() => (sideBySide = !sideBySide)}
			>
				{sideBySide ? 'unified' : 'split'}
			</button>
			<button
				type="button"
				aria-pressed={showWhitespace}
				title={showWhitespace ? 'Hide whitespace' : 'Show whitespace'}
				class={cn(
					'rounded px-1.5 py-0.5 font-mono text-micro transition-colors hover:bg-accent',
					showWhitespace
						? 'bg-accent text-foreground'
						: 'text-muted-foreground',
				)}
				onclick={() => (showWhitespace = !showWhitespace)}
			>
				ws
			</button>
		</div>
	</div>
	{#if sideBySide}
		<div class="grid max-h-80 grid-cols-2 overflow-auto text-micro leading-5">
			<div
				class="border-r border-border px-2 py-1 font-mono text-micro text-muted-foreground select-none"
			>
				old
			</div>
			<div
				class="px-2 py-1 font-mono text-micro text-muted-foreground select-none"
			>
				new
			</div>
			{#each rows as row, index (index)}
				{#if row.full}
					<div
						class={cn(
							'col-span-2 px-3 font-mono whitespace-pre',
							ROW_TONE[row.full.type],
						)}
					>
						{visibleText(row.full.text)}
					</div>
				{:else}
					<div
						class={cn(
							'min-w-0 overflow-x-auto px-3 font-mono whitespace-pre',
							row.left ? ROW_TONE[row.left.type] : 'bg-muted/40',
						)}
					>
						{row.left ? visibleText(row.left.text) : ''}
					</div>
					<div
						class={cn(
							'min-w-0 overflow-x-auto px-3 font-mono whitespace-pre',
							row.right ? ROW_TONE[row.right.type] : 'bg-muted/40',
						)}
					>
						{row.right ? visibleText(row.right.text) : ''}
					</div>
				{/if}
			{/each}
		</div>
	{:else}
		<pre
			class="max-h-80 overflow-auto text-micro leading-5">{#each lines as line, index (index)}<div
					class={cn(
						'flex gap-2 px-3 font-mono whitespace-pre',
						ROW_TONE[line.type],
					)}><span class="w-2 shrink-0 select-none opacity-70"
						>{SIGN[line.type]}</span
					><span class="min-w-0">{visibleText(line.text)}</span
					></div>{/each}</pre>
	{/if}
</div>
