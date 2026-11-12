<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		src: string | null;
		alt?: string;
		onclose: () => void;
	}

	let { src, alt = '', onclose }: Props = $props();

	let failed = $state(false);

	$effect(() => {
		void src;
		failed = false;
	});

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape' && src) {
			event.stopPropagation();
			onclose();
		}
	}

	function download(): void {
		if (!src) return;
		const anchor = document.createElement('a');
		anchor.href = src;
		anchor.download = alt || 'image';
		anchor.click();
	}
</script>

<svelte:window {onkeydown} />

{#if src}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
		<button
			type="button"
			aria-label={t('common.closeDialog')}
			class="absolute inset-0 cursor-default bg-black/70"
			onclick={onclose}
		></button>
		<div
			role="dialog"
			aria-modal="true"
			aria-label={alt || t('domain.messageBubble.attachedImage')}
			class="relative max-h-full max-w-3xl overflow-auto rounded-lg border border-border bg-card p-3 shadow-xl"
		>
			<div class="mb-2 flex items-center gap-1">
				<span class="min-w-0 flex-1 truncate text-caption text-muted-foreground">
					{alt || t('domain.messageBubble.attachedImage')}
				</span>
				<button
					type="button"
					onclick={download}
					title={t('page.ide.download')}
					aria-label={t('page.ide.download')}
					class="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				>
					<Icon name="download" size={14} />
				</button>
				<button
					type="button"
					onclick={onclose}
					title={t('common.close')}
					aria-label={t('common.close')}
					class="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				>
					<Icon name="x" size={14} />
				</button>
			</div>
			{#if failed}
				<p class="px-2 py-6 text-center text-caption text-destructive">
					{t('domain.messageBubble.imageLoadFailed')}
				</p>
			{:else}
				<img
					{src}
					{alt}
					class="max-h-[75vh] w-auto max-w-full rounded object-contain"
					loading="eager"
					decoding="async"
					draggable="false"
					onerror={() => (failed = true)}
				/>
			{/if}
		</div>
	</div>
{/if}
