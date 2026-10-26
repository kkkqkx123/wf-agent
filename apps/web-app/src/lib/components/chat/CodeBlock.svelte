<script lang="ts">
	/**
	 * Code block rendered by the app's own highlighter instead of the
	 * markstream default. It receives the parsed node from the markdown
	 * renderer through the `code_block` custom-component slot, reads the
	 * fenced language and source, and paints lexical tokens. Streaming
	 * re-renders only re-tokenize the current text.
	 */
	import { cn } from '#lib/ui/cn';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { highlightCode, normalizeLanguage, type TokenKind, type HighlightLine } from '#lib/utils/highlight';
	import { t } from '#lib/i18n';

	interface Props {
		node?: {
			language?: unknown;
			code?: unknown;
			raw?: unknown;
			content?: unknown;
		};
		class?: string;
	}

	let { node = undefined, class: className = '' }: Props = $props();

	const rawLanguage = $derived(typeof node?.language === 'string' ? node.language.trim() : '');
	const code = $derived(
		typeof node?.code === 'string'
			? node.code
			: typeof node?.raw === 'string'
				? node.raw
				: typeof node?.content === 'string'
					? node.content
					: '',
	);
	let lines = $state<HighlightLine[]>([]);
	$effect(() => {
		const currentCode = code;
		const currentLanguage = rawLanguage;
		let cancelled = false;
		highlightCode(currentCode, currentLanguage).then((result) => {
			if (!cancelled) lines = result;
		});
		return () => {
			cancelled = true;
		};
	});
	// Flatten the token lines into one stream with explicit newline entries so
	// the template emits variable text rather than a per-line string literal.
	type RenderToken = { kind: TokenKind | 'newline'; text: string };
	const tokens = $derived.by(() => {
		const out: RenderToken[] = [];
		lines.forEach((line, lineIndex) => {
			if (lineIndex > 0) out.push({ kind: 'newline', text: '\n' });
			for (const span of line) out.push(span);
		});
		return out;
	});
	const label = $derived((rawLanguage || normalizeLanguage(rawLanguage) || 'text').toUpperCase());
	// Fence languages whose dedicated renderer is not bundled: the source is
	// still shown highlighted, and the header states why it is not drawn.
	const DEGRADED_LANGUAGES = new Set([
		'mermaid',
		'd2',
		'plantuml',
		'graphviz',
		'infographic',
		'math',
		'latex',
		'katex',
	]);
	const degraded = $derived(DEGRADED_LANGUAGES.has(normalizeLanguage(rawLanguage)));

	const TOKEN_CLASS: Record<TokenKind | 'newline', string> = {
		plain: '',
		newline: '',
		keyword: 'text-code-keyword',
		string: 'text-code-string',
		comment: 'text-code-comment',
		number: 'text-code-number',
		function: 'text-code-function',
		type: 'text-code-type',
	};

	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	async function copy(): Promise<void> {
		try {
			await navigator.clipboard.writeText(code);
			copied = true;
			if (copyTimer) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => (copied = false), 1200);
		} catch {
			copied = false;
		}
	}
</script>

<div class={cn('my-2 overflow-hidden rounded-md border border-border bg-muted/40', className)}>
	<div class="flex items-center justify-between gap-2 border-b border-border/60 px-2.5 py-1">
		<span class="flex items-center gap-2">
			<span class="font-mono text-micro uppercase tracking-wide text-muted-foreground">{label}</span
			>
			{#if degraded}
				<span class="text-micro text-muted-foreground/70">
					{t('chat.codeBlock.sourceOnly')}
				</span>
			{/if}
		</span>
		<button
			type="button"
			onclick={copy}
			aria-label={t('chat.codeBlock.copyCode')}
			class="flex items-center gap-1 rounded px-1.5 py-0.5 text-micro text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
		>
			<Icon name={copied ? 'check' : 'copy'} size={12} />
			{copied ? t('common.copied') : t('common.copy')}
		</button>
	</div>
	<pre
		data-scrollable="true"
		class="overflow-x-auto px-3 py-2 font-mono text-micro leading-relaxed text-foreground"><code
			>{#each tokens as token, index (index)}{#if TOKEN_CLASS[token.kind]}<span
						class={TOKEN_CLASS[token.kind]}>{token.text}</span
					>{:else}{token.text}{/if}{/each}</code
		></pre>
</div>
