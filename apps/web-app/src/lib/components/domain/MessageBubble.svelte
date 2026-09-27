<script lang="ts">
	import type { LoopMessage } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import { splitAttachments } from '$lib/utils/attachments';
	import { textRuns } from '$lib/utils/mentions';
	import { formatDateTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		message: LoopMessage;
		class?: string;
		onretry?: () => void;
		oncontinue?: () => void;
		onfeedback?: (kind: 'up' | 'down') => void;
	}

	let {
		message,
		class: className = '',
		onretry,
		oncontinue,
		onfeedback,
	}: Props = $props();

	const ROLE_ICON: Record<LoopMessage['role'], IconName> = {
		user: 'user',
		assistant: 'sparkles',
		system: 'terminal',
		tool: 'blocks',
	};

	/** The small label every inline reference in a bubble shares. */
	const TAG =
		'rounded border border-border/70 bg-muted/60 px-1.5 text-micro text-muted-foreground';

	let expandedIndex = $state<number | null>(null);

	const isUser = $derived(message.role === 'user');
	const isAssistant = $derived(message.role === 'assistant');
	const showActions = $derived(
		isAssistant && (onretry || oncontinue || onfeedback),
	);
	const body = $derived(
		isUser
			? splitAttachments(message.content)
			: { text: message.content, attachments: [] },
	);
	const runs = $derived(textRuns(body.text));
	const expanded = $derived(
		expandedIndex === null ? null : body.attachments[expandedIndex],
	);
</script>

<article class={cn('flex gap-2.5', isUser && 'flex-row-reverse', className)}>
	<div
		class={cn(
			'mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-border',
			isUser
				? 'bg-primary text-primary-foreground'
				: 'bg-muted text-muted-foreground',
		)}
	>
		<Icon name={ROLE_ICON[message.role]} size={13} />
	</div>

	<div class={cn('min-w-0 max-w-[min(46rem,88%)]', isUser && 'text-right')}>
		<div
			class={cn(
				'w-full rounded-lg border px-3 py-2 text-left text-body',
				isUser
					? 'border-transparent bg-primary text-primary-foreground'
					: message.role === 'tool'
						? 'border-border bg-muted font-mono text-caption'
						: 'border-border bg-card text-card-foreground',
			)}
		>
			{#if message.toolName}
				<p class="mb-1 {TAG} inline-block uppercase tracking-wide">
					{message.toolName}
				</p>
			{/if}
			{#if isAssistant}
				<StreamMarkdown content={message.content} done />
			{:else}
				<p class="whitespace-pre-wrap break-words">
					{#each runs as run, index (index)}
						{#if run.kind}
							<span class={cn(TAG, 'mx-0.5 font-mono not-italic')}
								>{run.text}</span
							>
						{:else}
							{run.text}
						{/if}
					{/each}
				</p>
			{/if}
		</div>

		{#if body.attachments.length > 0}
			<div class="mt-1 flex flex-wrap gap-1.5 {isUser ? 'justify-end' : ''}">
				{#each body.attachments as file, index (`${file.name}-${index}`)}
					<button
						type="button"
						onclick={() =>
							(expandedIndex = expandedIndex === index ? null : index)}
						class={cn(
							TAG,
							'flex items-center gap-1 font-mono transition-colors hover:bg-accent hover:text-foreground',
							expandedIndex === index && 'border-info/60 text-info',
						)}
					>
						<Icon name="file" size={12} />
						{file.name}
					</button>
				{/each}
			</div>
			{#if expanded}
				<pre
					class="mt-1 max-h-56 overflow-auto rounded-md border border-border bg-muted px-2 py-1.5 text-left font-mono text-micro text-foreground">{expanded.content}</pre>
			{/if}
		{/if}

		{#if showActions}
			<div class="mt-1 flex items-center gap-0.5 {isUser ? 'justify-end' : ''}">
				{#if onretry}
					<IconButton
						icon="refresh"
						label="Retry this answer"
						compact
						onclick={onretry}
					/>
				{/if}
				{#if oncontinue}
					<IconButton
						icon="arrow-right"
						label="Continue this answer"
						compact
						onclick={oncontinue}
					/>
				{/if}
				{#if onfeedback}
					<IconButton
						icon="check"
						label="Helpful"
						compact
						onclick={() => onfeedback('up')}
					/>
					<IconButton
						icon="x"
						label="Not helpful"
						compact
						onclick={() => onfeedback('down')}
					/>
				{/if}
			</div>
		{/if}
		<p
			class="mt-0.5 flex items-center gap-2 text-micro text-muted-foreground {isUser
				? 'justify-end'
				: ''}"
		>
			<time>{formatDateTime(message.createdAt)}</time>
		</p>
	</div>
</article>
