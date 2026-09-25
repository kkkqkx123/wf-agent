<script lang="ts">
	import MarkdownRender from 'markstream-svelte';
	import 'markstream-svelte/index.css';
	import { resolvedTheme } from '$lib/stores/theme.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		content: string;
		done?: boolean;
		class?: string;
	}

	let { content, done = true, class: className = '' }: Props = $props();

	const isDark = $derived(resolvedTheme() === 'dark');
</script>

<div class={cn('text-body', className)}>
	<MarkdownRender {content} final={done} {isDark} />
</div>
