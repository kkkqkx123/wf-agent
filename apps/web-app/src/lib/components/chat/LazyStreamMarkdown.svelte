<script lang="ts">
	interface Props {
		content: string;
		done?: boolean;
		class?: string;
	}

	let { content, done = true, class: className = '' }: Props = $props();
</script>

{#await import('$lib/components/chat/StreamMarkdown.svelte') then module}
	<module.default {content} {done} class={className} />
{:catch}
	<pre class="whitespace-pre-wrap text-body">{content}</pre>
{/await}