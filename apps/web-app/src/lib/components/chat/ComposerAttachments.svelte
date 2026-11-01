<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import type { ImageAttachment, MessageAttachment } from '#lib/types/models';
	import { t } from '#lib/i18n';

	interface Props {
		attachments: MessageAttachment[];
		images: ImageAttachment[];
		onremove: (index: number) => void;
		onremoveImage: (index: number) => void;
	}

	let { attachments, images, onremove, onremoveImage }: Props = $props();
</script>

{#if attachments.length > 0 || images.length > 0}
	<div class="mb-2 flex flex-wrap gap-1.5">
		{#each images as image, index (index)}
			<span
				class="flex items-center gap-1.5 rounded-md border border-border bg-muted/50 p-1"
			>
				<img
					src={image.dataUrl}
					alt={image.name}
					title={image.name}
					class="h-10 w-10 rounded object-cover"
				/>
				<button
					type="button"
					aria-label={t('chat.composer.removeAttachment')}
					class="pr-1 font-mono text-micro text-muted-foreground hover:text-foreground"
					onclick={() => onremoveImage(index)}
				>
					<Icon name="x" size={12} />
				</button>
			</span>
		{/each}
		{#each attachments as file, index (index)}
			<span
				class="flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-1 font-mono text-micro text-muted-foreground"
			>
				<Icon name="file" size={12} />
				{file.name}
				{#if file.truncated}
					<span class="rounded bg-accent px-1" title={t('chat.composer.truncatedHint')}>
						{t('chat.composer.truncated')}
					</span>
				{/if}
				<button
					type="button"
					aria-label={t('chat.composer.removeAttachment')}
					class="hover:text-foreground"
					onclick={() => onremove(index)}
				>
					<Icon name="x" size={12} />
				</button>
			</span>
		{/each}
	</div>
{/if}
