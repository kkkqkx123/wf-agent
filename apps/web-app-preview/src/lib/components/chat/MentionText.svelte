<script lang="ts">
	import type { SvelteRenderableNode, SvelteRenderContext } from 'markstream-svelte';
	import { resolveStreamingTextState } from 'markstream-core';
	import { textRuns } from '#lib/utils/mentions';
	import MentionBadge from './MentionBadge.svelte';

	interface Props {
		node: SvelteRenderableNode;
		context?: SvelteRenderContext;
		indexKey?: string | number;
		typewriter?: boolean;
	}

	let {
		node,
		context = undefined,
		indexKey = undefined,
		typewriter = undefined,
	}: Props = $props();

	let previousKey = '';
	let previousContent = '';
	let deltaClass = 'markstream-svelte-text__stream-delta--a';

	const content = $derived(String((node as any)?.content ?? (node as any)?.raw ?? ''));
	const centered = $derived(Boolean((node as any)?.center));
	const streamKey = $derived(
		String(context?.customId ?? 'global') +
			':' +
			String(context?.streamRenderVersion ?? 0) +
			':' +
			String(indexKey ?? 'node'),
	);
	const fadeEnabled = $derived(context?.fade !== false);

	const streamInfo = $derived.by(() => {
		const state = context?.textStreamState;
		const previous = streamKey === previousKey ? previousContent : (state?.get(streamKey) ?? '');
		const result = resolveStreamingTextState({
			nextContent: content,
			previousContent: previous,
			typewriterEnabled: fadeEnabled,
		});
		if (result.appended)
			deltaClass = deltaClass.endsWith('--a')
				? 'markstream-svelte-text__stream-delta--b'
				: 'markstream-svelte-text__stream-delta--a';
		previousKey = streamKey;
		previousContent = content;
		state?.set(streamKey, content);
		return {
			stableContent: result.settledContent,
			deltaContent: result.streamedDelta,
			deltaClass,
		};
	});

	function renderRuns(text: string) {
		return textRuns(text);
	}
</script>

<span
	data-typewriter={context?.typewriter === true ? '1' : undefined}
	class:markstream-svelte-text--centered={centered}
	class="markstream-svelte-text-node text-node"
>
	{#each renderRuns(streamInfo.stableContent) as run, i (i)}
		{#if run.kind}
			<span class="mx-0.5 inline-flex">
				<MentionBadge text={run.text} kind={run.kind} />
			</span>
		{:else}
			{run.text}
		{/if}
	{/each}
	{#if streamInfo.deltaContent}
		<span class="markstream-svelte-text__stream-delta text-node-stream-delta {streamInfo.deltaClass}">
			{#each renderRuns(streamInfo.deltaContent) as run, i (i)}
				{#if run.kind}
					<span class="mx-0.5 inline-flex">
						<MentionBadge text={run.text} kind={run.kind} />
					</span>
				{:else}
					{run.text}
				{/if}
			{/each}
		</span>
	{/if}
</span>