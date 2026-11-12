<script lang="ts">
	import { setContext } from 'svelte';
	import LazyStreamMarkdown from '#lib/components/chat/LazyStreamMarkdown.svelte';
	import { MARKDOWN_DOC_PATH_CTX } from '#lib/components/chat/MarkdownImage.svelte';

	interface Props {
		content: string;
		docPath: string;
	}

	let { content, docPath }: Props = $props();

	setContext(MARKDOWN_DOC_PATH_CTX, () => docPath);
</script>

<div class="min-h-0 flex-1 overflow-auto p-4">
	<div class="mx-auto max-w-3xl">
		<LazyStreamMarkdown {content} done />
	</div>
</div>
