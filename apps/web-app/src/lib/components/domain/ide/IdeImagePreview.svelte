<script lang="ts">
	import { onDestroy } from 'svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import { readFsRaw } from '#lib/services/fs';
	import {
		fileNameOf,
		mimeForPreviewableImage,
		objectUrlForBase64,
		objectUrlForSvgText,
		revokeObjectUrl,
	} from '#lib/utils/image-preview';
	import { t } from '#lib/i18n';

	interface Props {
		path: string;
		/** Svg source already loaded as text; skips the raw fetch. */
		svgText?: string | null;
	}

	let { path, svgText = null }: Props = $props();

	let objectUrl = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let nonce = $state(0);
	let fit = $state(true);
	let zoom = $state(1);
	let naturalWidth = $state(0);
	let naturalHeight = $state(0);
	let hasError = $state(false);

	const fileName = $derived(fileNameOf(path));
	const dimensions = $derived(
		naturalWidth > 0 && naturalHeight > 0 ? `${naturalWidth} × ${naturalHeight}` : null,
	);

	async function load(generation: number): Promise<void> {
		loading = true;
		error = null;
		hasError = false;
		revokeObjectUrl(objectUrl);
		objectUrl = null;
		try {
			let next: string;
			if (svgText !== null && svgText !== undefined) {
				next = objectUrlForSvgText(svgText);
			} else {
				const raw = await readFsRaw(path);
				if (generation !== loadGeneration) return;
				const mime = raw.mime || mimeForPreviewableImage(path);
				next = objectUrlForBase64(raw.content_base64, mime);
			}
			if (generation !== loadGeneration) {
				revokeObjectUrl(next);
				return;
			}
			objectUrl = next;
		} catch (e) {
			if (generation !== loadGeneration) return;
			error = e instanceof Error ? e.message : String(e);
		} finally {
			if (generation === loadGeneration) loading = false;
		}
	}

	let loadGeneration = 0;

	$effect(() => {
		void path;
		void svgText;
		void nonce;
		naturalWidth = 0;
		naturalHeight = 0;
		fit = true;
		zoom = 1;
		loadGeneration += 1;
		void load(loadGeneration);
	});

	onDestroy(() => revokeObjectUrl(objectUrl));

	function zoomIn(): void {
		fit = false;
		zoom = Math.min(4, Math.round((zoom + 0.25) * 100) / 100);
	}

	function zoomOut(): void {
		fit = false;
		zoom = Math.max(0.25, Math.round((zoom - 0.25) * 100) / 100);
	}

	function toggleFit(): void {
		fit = !fit;
		if (fit) zoom = 1;
	}

	function onImageLoad(event: Event): void {
		const image = event.currentTarget as HTMLImageElement | null;
		if (!image) return;
		if (image.naturalWidth === 0 && image.naturalHeight === 0) {
			hasError = true;
			return;
		}
		naturalWidth = image.naturalWidth;
		naturalHeight = image.naturalHeight;
	}

	function onImageError(): void {
		hasError = true;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div
		class="flex flex-wrap items-center gap-1 border-b border-border px-2 py-1 text-micro text-muted-foreground"
	>
		<span class="mr-1 font-medium text-foreground">{t('page.ide.imagePreview')}</span>
		{#if dimensions}
			<span class="tabular-nums">{dimensions}</span>
		{/if}
		<span class="min-w-0 flex-1"></span>
		<Button
			variant="ghost"
			size="sm"
			onclick={zoomOut}
			disabled={loading || fit}
			title={t('domain.graph.zoomOut')}
		>
			<Icon name="minus" size={13} />
		</Button>
		<span class="w-12 text-center tabular-nums">{Math.round(zoom * 100)}%</span>
		<Button
			variant="ghost"
			size="sm"
			onclick={zoomIn}
			disabled={loading || fit}
			title={t('domain.graph.zoomIn')}
		>
			<Icon name="plus" size={13} />
		</Button>
		<Button variant="ghost" size="sm" onclick={toggleFit} title={t('domain.graph.fitToView')}>
			<Icon name={fit ? 'minimize' : 'maximize'} size={13} />
			{fit ? t('page.ide.actualSize') : t('domain.graph.fitToView')}
		</Button>
		{#if objectUrl && !loading && !error && !hasError}
			<a
				href={objectUrl}
				download={fileName}
				class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-small font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				title={t('page.ide.download')}
			>
				<Icon name="download" size={13} />
				{t('page.ide.download')}
			</a>
		{/if}
	</div>

	<div class="checkerboard min-h-0 flex-1 overflow-auto p-4">
		{#if loading}
			<Skeleton shape="block" height="16rem" class="rounded-md" />
		{:else if error}
			<div class="mx-auto max-w-sm space-y-2 rounded-md border border-border bg-card p-4 text-center">
				<p class="text-body font-medium text-destructive">{t('page.ide.imageLoadFailed')}</p>
				<p class="text-caption break-words text-muted-foreground">{error}</p>
				<Button
					variant="outline"
					size="sm"
					onclick={() => {
						nonce += 1;
					}}
				>
					{t('common.retry')}
				</Button>
			</div>
		{:else if objectUrl && !hasError}
			<div class="flex min-h-full items-center justify-center">
				<img
					src={objectUrl}
					alt={fileName}
					title={fileName}
					loading="lazy"
					decoding="async"
					draggable="false"
					onload={onImageLoad}
					onerror={onImageError}
					class={fit
						? 'max-h-full max-w-full rounded border border-border bg-card object-contain'
						: 'rounded border border-border bg-card'}
					style={fit ? undefined : `width: ${Math.round(zoom * 100)}%; max-width: none;`}
				/>
			</div>
		{:else}
			<div class="mx-auto max-w-sm space-y-2 rounded-md border border-border bg-card p-4 text-center">
				<p class="text-body font-medium text-destructive">{t('page.ide.imageLoadFailed')}</p>
				<Button
					variant="outline"
					size="sm"
					onclick={() => {
						nonce += 1;
					}}
				>
					{t('common.retry')}
				</Button>
			</div>
		{/if}
	</div>
</div>

<style>
	.checkerboard {
		background-image:
			linear-gradient(45deg, var(--muted) 25%, transparent 25%),
			linear-gradient(-45deg, var(--muted) 25%, transparent 25%),
			linear-gradient(45deg, transparent 75%, var(--muted) 75%),
			linear-gradient(-45deg, transparent 75%, var(--muted) 75%);
		background-size: 16px 16px;
		background-position: 0 0, 0 8px, 8px -8px, -8px 0;
	}
</style>
