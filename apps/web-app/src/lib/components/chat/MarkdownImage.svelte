<script module lang="ts">
	/** Svelte context key carrying a thunk for the owning markdown document path. */
	export const MARKDOWN_DOC_PATH_CTX = 'ide-markdown-doc-path';
	export type MarkdownDocPathFn = () => string;
</script>

<script lang="ts">
	/**
	 * Markdown image node with click-to-enlarge. The parser already strips
	 * unsafe sources, the local allowlist keeps the custom renderer aligned
	 * with it so blocked svg data URLs never reach the lightbox.
	 *
	 * Workspace-relative sources resolve against the owning document when
	 * the IDE preview provides one through context; every other consumer
	 * keeps the raw source behavior.
	 */
	import { getContext, onDestroy } from 'svelte';
	import { cn } from '#lib/ui/cn';
	import {
		isSafeMarkdownImageSrc,
		isWorkspaceRelativeImageSrc,
		mimeForPreviewableImage,
		objectUrlForBase64,
		resolveWorkspaceImagePath,
		revokeObjectUrl,
	} from '#lib/utils/image-preview';
	import { readFsRaw } from '#lib/services/fs';
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
	const docPathFn = getContext<MarkdownDocPathFn | null>(MARKDOWN_DOC_PATH_CTX) ?? null;
	const docPath = $derived(docPathFn?.() ?? null);

	let open = $state(false);
	let failed = $state(false);
	let blobUrl = $state<string | null>(null);
	let blobLoading = $state(false);
	let generation = 0;

	const resolvedPath = $derived(
		docPath && isWorkspaceRelativeImageSrc(rawSrc)
			? resolveWorkspaceImagePath(rawSrc, docPath)
			: null,
	);
	const displaySrc = $derived(blobUrl ?? rawSrc);

	$effect(() => {
		void rawSrc;
		void blobUrl;
		failed = false;
	});

	$effect(() => {
		const target = resolvedPath;
		generation += 1;
		const current = generation;
		revokeObjectUrl(blobUrl);
		blobUrl = null;
		if (!target) return;
		blobLoading = true;
		void readFsRaw(target)
			.then((raw) => {
				if (current !== generation) return;
				const mime = raw.mime || mimeForPreviewableImage(target);
				blobUrl = objectUrlForBase64(raw.content_base64, mime);
			})
			.catch(() => {
				if (current !== generation) return;
				blobUrl = null;
			})
			.finally(() => {
				if (current === generation) blobLoading = false;
			});
	});

	onDestroy(() => revokeObjectUrl(blobUrl));
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
		{:else if resolvedPath && blobLoading && !blobUrl}
			<span class="block px-3 py-2 text-caption text-muted-foreground">
				{alt || rawSrc}
			</span>
		{:else}
			<img
				src={displaySrc}
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
	<ImageLightbox src={open && !failed ? displaySrc : null} {alt} onclose={() => (open = false)} />
{/if}
