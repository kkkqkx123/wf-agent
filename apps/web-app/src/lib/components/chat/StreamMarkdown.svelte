<script lang="ts">
	import MarkdownRender, { type CustomComponentMap } from 'markstream-svelte';
	import 'markstream-svelte/index.css';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { cn } from '#lib/ui/cn';
	import CodeBlock from './CodeBlock.svelte';
	import MentionText from './MentionText.svelte';

	interface Props {
		content: string;
		done?: boolean;
		class?: string;
	}

	let { content, done = true, class: className = '' }: Props = $props();

	const isDark = $derived(resolvedTheme() === 'dark');

	/** The app paints fenced code blocks itself so they carry lexical
	 * highlighting without pulling in an optional markstream peer. */
	const components = {
		code_block: CodeBlock,
		text: MentionText,
	} as CustomComponentMap;
</script>

<div class={cn('text-body', className)}>
	<MarkdownRender {content} final={done} {isDark} customComponents={components} />
</div>
