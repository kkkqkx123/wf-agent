<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import type { MessageAttachment } from '#lib/types/models';
	import { t } from '#lib/i18n';

	interface Props {
		attachments: MessageAttachment[];
		onremove: (index: number) => void;
	}

	let { attachments, onremove }: Props = $props();
</script>

{#if attachments.length > 0}
	<div class="mb-2 flex flex-wrap gap-1.5">
		{#each attachments as file, index (index)}
			<span
				class="flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-1 font-mono text-micro text-muted-foreground"
			>
				<Icon name="file" size={12} />
				{file.name}
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
