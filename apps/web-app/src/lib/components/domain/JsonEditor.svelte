<script lang="ts">
	import { cn } from '$lib/utils/cn';

	interface Props {
		value?: string;
		errorLine?: number | null;
		placeholder?: string;
		label?: string;
		class?: string;
		minHeight?: string;
	}

	let {
		value = $bindable(''),
		errorLine = null,
		placeholder = '',
		label = 'JSON editor',
		class: className = '',
		minHeight = '16rem',
	}: Props = $props();

	interface Token {
		text: string;
		tone: string;
	}

	const TONE_CLASS: Record<string, string> = {
		key: 'text-info',
		string: 'text-success',
		number: 'text-warning',
		literal: 'text-primary',
		punct: 'text-muted-foreground',
		plain: 'text-foreground',
	};

	const TOKEN_PATTERN =
		/("(?:(?!["\\])|.)*")|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([{}[\]:,])/g;

	function tokenize(text: string): Token[] {
		const tokens: Token[] = [];
		let cursor = 0;
		TOKEN_PATTERN.lastIndex = 0;
		for (
			let match = TOKEN_PATTERN.exec(text);
			match !== null;
			match = TOKEN_PATTERN.exec(text)
		) {
			if (match.index > cursor) {
				tokens.push({ text: text.slice(cursor, match.index), tone: 'plain' });
			}
			const [full, quoted, numeric, literal] = match;
			if (quoted) {
				const rest = text.slice(match.index + full.length);
				const isKey = /^\s*:/.test(rest);
				tokens.push({ text: full, tone: isKey ? 'key' : 'string' });
			} else if (numeric) {
				tokens.push({ text: full, tone: 'number' });
			} else if (literal) {
				tokens.push({ text: full, tone: 'literal' });
			} else {
				tokens.push({ text: full, tone: 'punct' });
			}
			cursor = match.index + full.length;
		}
		if (cursor < text.length) {
			tokens.push({ text: text.slice(cursor), tone: 'plain' });
		}
		if (tokens.length === 0) tokens.push({ text: ' ', tone: 'plain' });
		return tokens;
	}

	let scroller: HTMLDivElement | null = $state(null);
	let area: HTMLTextAreaElement | null = $state(null);
	let gutterInner: HTMLDivElement | null = $state(null);

	const lines = $derived(value.split('\n'));
	const rowCount = $derived(Math.max(lines.length, 1));
	const tokens = $derived(tokenize(value));

	function syncScroll(): void {
		if (!scroller || !gutterInner) return;
		gutterInner.style.transform = `translateY(${-scroller.scrollTop}px)`;
	}

	function insertIndent(event: KeyboardEvent): void {
		if (event.key !== 'Tab' || !area) return;
		event.preventDefault();
		const start = area.selectionStart ?? value.length;
		const end = area.selectionEnd ?? value.length;
		value = `${value.slice(0, start)}  ${value.slice(end)}`;
		queueMicrotask(() => {
			area?.setSelectionRange(start + 2, start + 2);
		});
	}

	export function scrollToLine(line: number): void {
		if (!scroller || !gutterInner) return;
		const row = gutterInner.children[line - 1] as HTMLElement | undefined;
		if (row) {
			scroller.scrollTop = row.offsetTop - scroller.clientHeight / 3;
			area?.focus({ preventScroll: true });
		}
	}

	export function focus(): void {
		area?.focus();
	}
</script>

<div
	class={cn(
		'flex overflow-hidden rounded-md border border-input bg-card',
		className,
	)}
	style:min-height={minHeight}
>
	<div class="w-11 shrink-0 overflow-hidden bg-muted/40 select-none">
		<div bind:this={gutterInner} class="px-2 py-2 text-right">
			{#each lines.map((_, idx) => idx) as idx (idx)}
				<div
					class={cn(
						'font-mono text-small leading-6 tabular-nums',
						errorLine === idx + 1
							? 'rounded bg-destructive/15 font-medium text-destructive'
							: 'text-muted-foreground',
					)}
				>
					{idx + 1}
				</div>
			{/each}
		</div>
	</div>
	<div
		bind:this={scroller}
		onscroll={syncScroll}
		class="min-w-0 flex-1 overflow-auto"
	>
		<div class="grid w-max min-w-full">
			<pre
				aria-hidden="true"
				class="col-start-1 row-start-1 px-3 py-2 font-mono text-small leading-6 whitespace-pre"
			>{#each tokens as token, index (index)}<span class={TONE_CLASS[token.tone]}>{token.text}</span>{/each}</pre>
			<textarea
				bind:this={area}
				bind:value
				aria-label={label}
				{placeholder}
				wrap="off"
				rows={rowCount}
				spellcheck={false}
				autocomplete="off"
				autocapitalize="off"
				onkeydown={insertIndent}
				class="col-start-1 row-start-1 w-full resize-none overflow-hidden bg-transparent px-3 py-2 font-mono text-small leading-6 whitespace-pre text-transparent caret-foreground outline-none placeholder:text-muted-foreground"
			></textarea>
		</div>
	</div>
</div>
