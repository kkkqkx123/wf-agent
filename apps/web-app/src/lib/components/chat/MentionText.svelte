<script lang="ts">
	import type { SvelteRenderableNode, SvelteRenderContext } from 'markstream-svelte';
	import { resolveStreamingTextState } from 'markstream-core';
	import { resolveMentionFileTarget, textRuns } from '#lib/utils/mentions';
	import { buildIdeUrl } from '#lib/utils/ide-url';
	import MentionBadge from './MentionBadge.svelte';

	interface Props {
		node: SvelteRenderableNode;
		context?: SvelteRenderContext;
		indexKey?: string | number;
		typewriter?: boolean;
	}

	let { node, context = undefined, indexKey = undefined }: Props = $props();

	let previousKey = '';
	let previousContent = '';
	let deltaClass = 'markstream-svelte-text__stream__delta--a';

	const content = $derived(
		String(
			(node as Record<string, unknown>)?.content ?? (node as Record<string, unknown>)?.raw ?? '',
		),
	);
	const centered = $derived(Boolean((node as Record<string, unknown>)?.center));
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
				? 'markstream-svelte-text__stream__delta--b'
				: 'markstream-svelte-text__stream__delta--a';
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

	function stableHref(text: string, kind: string): string | null {
		const target = resolveMentionFileTarget(text, kind);
		if (!target) return null;
		return buildIdeUrl(target.path, target.lineStart, null);
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
				<MentionBadge text={run.text} kind={run.kind} href={stableHref(run.text, run.kind)} />
			</span>
		{:else}
			{run.text}
		{/if}
	{/each}
	{#if streamInfo.deltaContent}
		<span
			class="markstream-svelte-text__stream__delta text-node-stream-delta {streamInfo.deltaClass}"
		>
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
