<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import type { OutgoingMessage } from '#lib/types/models';
	import { t } from '#lib/i18n';

	interface Props {
		queue: OutgoingMessage[];
		onmove: (index: number, delta: number) => void;
		onedit: (index: number) => void;
		onremove: (index: number) => void;
	}

	let { queue, onmove, onedit, onremove }: Props = $props();
</script>

{#if queue.length > 0}
	<ul class="mb-2 space-y-1">
		{#each queue as item, index (index)}
			<li
				class="flex items-center gap-2 rounded-md border border-border bg-muted/50 px-2 py-1 text-caption text-muted-foreground"
			>
				<span class="font-mono text-micro">#{index + 1}</span>
				<span class="min-w-0 flex-1 truncate">{item.text}</span>
				{#if item.attachments.length > 0}
					<span class="shrink-0 font-mono text-micro">
						{t('chat.composer.fileCount', { count: item.attachments.length })}
					</span>
				{/if}
				<button
					type="button"
					aria-label={t('chat.composer.moveQueuedUp')}
					disabled={index === 0}
					class="shrink-0 hover:text-foreground disabled:opacity-40"
					onclick={() => onmove(index, -1)}
				>
					<Icon name="chevron-up" size={13} />
				</button>
				<button
					type="button"
					aria-label={t('chat.composer.moveQueuedDown')}
					disabled={index === queue.length - 1}
					class="shrink-0 hover:text-foreground disabled:opacity-40"
					onclick={() => onmove(index, 1)}
				>
					<Icon name="chevron-down" size={13} />
				</button>
				<button
					type="button"
					aria-label={t('chat.composer.editQueued')}
					class="shrink-0 hover:text-foreground"
					onclick={() => onedit(index)}
				>
					<Icon name="pencil" size={13} />
				</button>
				<button
					type="button"
					aria-label={t('chat.composer.removeQueued')}
					class="shrink-0 hover:text-foreground"
					onclick={() => onremove(index)}
				>
					<Icon name="x" size={13} />
				</button>
			</li>
		{/each}
	</ul>
{/if}
