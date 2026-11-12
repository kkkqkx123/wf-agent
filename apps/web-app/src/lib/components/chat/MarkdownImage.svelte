<script lang="ts">
	/**
	 * Markdown image node with click-to-enlarge. The parser already strips
	 * unsafe sources, the local allowlist keeps the custom renderer aligned
	 * with it so blocked svg data URLs never reach the lightbox.
	 */
	import { cn } from '#lib/ui/cn';
	import { isSafeMarkdownImageSrc } from '#lib/utils/image-preview';
	import { t } from '#lib/i18n';
	import ImageLightbox from './ImageLightbox.svelte';

	interface Props {
		node?: {
			src?: unknown;
			alt?: unknown;
			title?: unknown;
		};
		class?: string;
	}

	let { node = undefined, class: className = '' }: Props = $props();

	const rawSrc = $derived(typeof node?.src === 'string' ? node.src : '');
	const alt = $derived(typeof node?.alt === 'string' ? node.alt : '');
	const title = $derived(typeof node?.title === 'string' ? node.title : '');
	const safe = $derived(isSafeMarkdownImageSrc(rawSrc));

	let open = $state(false);
	let failed = $state(false);

	$effect(() => {
		void rawSrc;
		failed = false;
	});
</script>

{#if safe && rawSrc}
	<button
		type="button"
		onclick={() => (open = true)}
		title={t('domain.messageBubble.enlargeImage')}
		aria-label={t('domain.messageBubble.attachedImage')}
		class={cn(
			'my-2 block max-w-full overflow-hidden rounded-md border border-border bg-muted/40',
			className,
		)}
	>
		{#if failed}
			<span class="block px-3 py-2 text-caption text-destructive">
				{t('domain.messageBubble.imageLoadFailed')}
			</span>
		{:else}
			<img
				src={rawSrc}
				{alt}
				title={title || alt || undefined}
				loading="lazy"
				decoding="async"
				draggable="false"
				class="max-h-72 w-auto max-w-full object-contain"
				onerror={() => (failed = true)}
			/>
		{/if}
	</button>
	<ImageLightbox src={open && !failed ? rawSrc : null} {alt} onclose={() => (open = false)} />
{/if}
