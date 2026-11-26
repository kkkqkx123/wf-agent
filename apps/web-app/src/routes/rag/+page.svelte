<script lang="ts">
	import { onMount } from 'svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import { ApiHttpError } from '#lib/api/envelope';
	import { t } from '#lib/i18n';
	import { toasts } from '#lib/stores/toast.svelte';
	import {
		deleteRagSource,
		ingestRagSource,
		ragDiagnose,
		ragPreview,
		ragSources,
		ragStats,
		reindexRagSource,
		type RagPreviewHit,
		type RagSourceStatusEntry,
		type RagStats,
	} from '#lib/services/rag';

	let collection = $state('kb');
	let namespace = $state('kb');
	let stats = $state<RagStats | null>(null);
	let sources = $state<RagSourceStatusEntry[]>([]);
	let unavailable = $state(false);
	let error = $state('');

	let query = $state('');
	let topK = $state('5');
	let excerptChars = $state('200');
	let hits = $state<RagPreviewHit[]>([]);
	let causes = $state<string[]>([]);
	let previewed = $state(false);

	let source = $state('');
	let uri = $state('');
	let kind = $state('url');
	let converter = $state('');
	let options = $state('');
	let force = $state(false);
	let busy = $state(false);

	const scope = () => ({ collection, namespace });

	function isUnavailable(err: unknown): boolean {
		return (
			err instanceof ApiHttpError && (err.code === 'SERVICE_UNAVAILABLE' || err.status === 503)
		);
	}

	function messageOf(err: unknown): string {
		return err instanceof Error ? err.message : String(err);
	}

	async function load(): Promise<void> {
		error = '';
		unavailable = false;
		const [statsResult, sourcesResult] = await Promise.allSettled([
			ragStats(scope()),
			ragSources(scope()),
		]);
		if (statsResult.status === 'fulfilled') {
			stats = statsResult.value;
		} else if (isUnavailable(statsResult.reason)) {
			unavailable = true;
		} else {
			error = t('page.rag.loadFailed', { message: messageOf(statsResult.reason) });
		}
		if (sourcesResult.status === 'fulfilled') {
			sources = sourcesResult.value;
		} else if (isUnavailable(sourcesResult.reason)) {
			unavailable = true;
		} else {
			error = t('page.rag.loadFailed', { message: messageOf(sourcesResult.reason) });
		}
	}

	async function preview(): Promise<void> {
		previewed = true;
		hits = [];
		try {
			hits = await ragPreview({
				...scope(),
				query,
				top_k: Number(topK),
				excerpt_chars: Number(excerptChars),
			});
		} catch (err) {
			toasts.error(t('page.rag.actionFailed', { message: messageOf(err) }));
		}
	}

	async function diagnose(): Promise<void> {
		causes = [];
		try {
			const result = await ragDiagnose({
				query,
				top_k: Number(topK),
				collection,
				namespace,
			});
			causes = result.likely_causes;
		} catch (err) {
			toasts.error(t('page.rag.actionFailed', { message: messageOf(err) }));
		}
	}

	function parseOptions(): Record<string, unknown> {
		if (!options.trim()) return {};
		return JSON.parse(options) as Record<string, unknown>;
	}

	async function runAction(action: () => Promise<string>): Promise<void> {
		busy = true;
		try {
			toasts.success(await action());
			await load();
		} catch (err) {
			toasts.error(t('page.rag.actionFailed', { message: messageOf(err) }));
		} finally {
			busy = false;
		}
	}

	function ingest(): Promise<string> {
		return ingestRagSource({
			source,
			uri,
			kind,
			collection,
			namespace,
			converter: converter.trim() || null,
			options: parseOptions(),
			force,
		}).then((result) => t('page.rag.ingested', { source: result.source, chunks: result.chunks }));
	}

	function reindex(entry: RagSourceStatusEntry): Promise<string> {
		return reindexRagSource({
			source: entry.state.source,
			uri: entry.state.uri,
			kind: entry.state.kind,
			collection: entry.state.collection,
			namespace: entry.state.namespace,
			converter: entry.state.converter,
			options: {},
			force: true,
		}).then((result) => t('page.rag.ingested', { source: result.source, chunks: result.chunks }));
	}

	function remove(entry: RagSourceStatusEntry): Promise<string> {
		return deleteRagSource({
			source: entry.state.source,
			collection: entry.state.collection,
			namespace: entry.state.namespace,
		}).then((count) => t('page.rag.deleted', { count }));
	}

	onMount(() => {
		void load();
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.rag.title')} description={t('page.rag.description')}>
		{#snippet actions()}
			<Button variant="outline" size="sm" onclick={() => void load()}>
				{t('page.rag.refresh')}
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 pb-6">
		{#if unavailable}
			<p
				class="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-caption text-warning"
			>
				{t('page.rag.unavailable')}
			</p>
		{:else if error}
			<p
				class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-caption text-destructive"
			>
				{error}
			</p>
		{/if}

		<Card title={t('page.rag.scope')}>
			<div class="flex flex-wrap items-end gap-3">
				<div class="w-48">
					<Input label={t('page.rag.collection')} bind:value={collection} size="sm" />
				</div>
				<div class="w-48">
					<Input label={t('page.rag.namespace')} bind:value={namespace} size="sm" />
				</div>
				<Button size="sm" onclick={() => void load()}>{t('page.rag.refresh')}</Button>
			</div>
		</Card>

		<div class="grid gap-4 lg:grid-cols-2">
			<Card title={t('page.rag.stats')}>
				{#if stats}
					<dl class="grid grid-cols-2 gap-3 text-caption">
						<div>
							<dt class="text-muted-foreground">{t('page.rag.collection')}</dt>
							<dd class="font-mono text-foreground">{stats.collection}</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('page.rag.namespace')}</dt>
							<dd class="font-mono text-foreground">{stats.namespace}</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('page.rag.points')}</dt>
							<dd class="tabular-nums text-foreground">{stats.point_count}</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('page.rag.keywordPoints')}</dt>
							<dd class="tabular-nums text-foreground">
								{stats.keyword_point_count ?? '—'}
							</dd>
						</div>
					</dl>
				{:else}
					<p class="text-caption text-muted-foreground">—</p>
				{/if}
			</Card>

			<Card title={t('page.rag.addSource')}>
				<div class="space-y-3">
					<div class="grid gap-2 sm:grid-cols-2">
						<Input label={t('page.rag.sourceId')} bind:value={source} size="sm" />
						<Input label={t('page.rag.kind')} bind:value={kind} size="sm" />
					</div>
					<Input label={t('page.rag.sourceUri')} bind:value={uri} size="sm" />
					<Input label={t('page.rag.converter')} bind:value={converter} size="sm" />
					<Input label={t('page.rag.options')} bind:value={options} size="sm" />
					<label class="flex items-center gap-2 text-caption text-foreground">
						<input type="checkbox" bind:checked={force} />
						{t('page.rag.force')}
					</label>
					<Button
						size="sm"
						disabled={busy || !source.trim() || !uri.trim() || !kind.trim()}
						onclick={() => void runAction(ingest)}
					>
						{busy ? t('page.rag.busy') : t('page.rag.submit')}
					</Button>
				</div>
			</Card>
		</div>

		<Card title={t('page.rag.sources')}>
			{#if sources.length === 0}
				<p class="text-caption text-muted-foreground">{t('page.rag.emptySources')}</p>
			{:else}
				<table class="w-full text-left text-caption">
					<thead class="text-muted-foreground">
						<tr>
							<th class="py-1 pr-2">{t('page.rag.source')}</th>
							<th class="py-1 pr-2">{t('page.rag.kind')}</th>
							<th class="py-1 pr-2">{t('page.rag.uri')}</th>
							<th class="py-1 pr-2">{t('page.rag.status')}</th>
							<th class="py-1 pr-2">{t('page.rag.chunks')}</th>
							<th class="py-1 pr-2">{t('page.rag.points')}</th>
							<th class="py-1"></th>
						</tr>
					</thead>
					<tbody>
						{#each sources as entry (entry.state.source)}
							<tr class="border-t border-border align-top">
								<td class="py-1.5 pr-2 font-mono text-foreground">{entry.state.source}</td>
								<td class="py-1.5 pr-2">{entry.state.kind}</td>
								<td class="max-w-64 truncate py-1.5 pr-2 font-mono">{entry.state.uri}</td>
								<td class="py-1.5 pr-2">
									<span class={entry.state.status === 'failed' ? 'text-destructive' : ''}>
										{entry.state.status}
									</span>
									{#if entry.state.error}
										<span class="block text-destructive">{entry.state.error}</span>
									{/if}
								</td>
								<td class="py-1.5 pr-2 tabular-nums">{entry.state.chunks}</td>
								<td class="py-1.5 pr-2 tabular-nums">{entry.point_count}</td>
								<td class="py-1.5">
									<div class="flex gap-1">
										<Button
											variant="ghost"
											size="sm"
											disabled={busy}
											onclick={() => void runAction(() => reindex(entry))}
										>
											{t('page.rag.reindex')}
										</Button>
										<Button
											variant="ghost"
											size="sm"
											disabled={busy}
											onclick={() => void runAction(() => remove(entry))}
										>
											{t('page.rag.delete')}
										</Button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</Card>

		<div class="grid gap-4 lg:grid-cols-2">
			<Card title={t('page.rag.preview')}>
				<div class="space-y-3">
					<Input label={t('page.rag.query')} bind:value={query} size="sm" />
					<div class="grid grid-cols-2 gap-2">
						<Input label={t('page.rag.topK')} bind:value={topK} size="sm" />
						<Input label={t('page.rag.excerptChars')} bind:value={excerptChars} size="sm" />
					</div>
					<div class="flex gap-2">
						<Button size="sm" onclick={() => void preview()}>
							{t('page.rag.previewAction')}
						</Button>
						<Button variant="outline" size="sm" onclick={() => void diagnose()}>
							{t('page.rag.diagnoseAction')}
						</Button>
					</div>
					{#if previewed && hits.length === 0}
						<p class="text-caption text-muted-foreground">{t('page.rag.noHits')}</p>
					{/if}
					<ul class="space-y-2">
						{#each hits as hit (hit.chunk_id)}
							<li class="rounded-md border border-border px-2 py-1.5 text-caption">
								<div class="flex items-center justify-between gap-2 font-mono text-micro">
									<span>{hit.chunk_id}</span>
									<span class="tabular-nums">{hit.score.toFixed(3)}</span>
								</div>
								<p class="mt-1 break-words text-foreground">{hit.excerpt}</p>
								<p class="mt-0.5 truncate font-mono text-micro text-muted-foreground">
									{hit.source}
								</p>
							</li>
						{/each}
					</ul>
				</div>
			</Card>

			<Card title={t('page.rag.diagnose')}>
				{#if causes.length === 0}
					<p class="text-caption text-muted-foreground">{t('page.rag.likelyCauses')}</p>
				{:else}
					<ul class="list-disc space-y-1 pl-4 text-caption">
						{#each causes as cause}
							<li>{cause}</li>
						{/each}
					</ul>
				{/if}
			</Card>
		</div>
	</div>
</div>
