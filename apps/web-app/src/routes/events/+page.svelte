<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import {
		listEvents,
		listDependencies,
		getDiagnostics,
		deleteAllEvents,
		getEventSize,
	} from '$lib/services/events';
	import type { EventRecord } from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { live } from '$lib/stores/live.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatNumber,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'stream', label: 'Event stream' },
		{ id: 'dependencies', label: 'Dependencies' },
		{ id: 'operations', label: 'Operations' },
	];

	const initial = parseListParams(page.url);
	let tab = $state(initial.tab ?? 'stream');
	let query = $state(initial.q ?? '');
	let expandedId = $state<string | null>(null);
	let purgeOpen = $state(false);
	let purging = $state(false);

	const list = createCollection((params) => listEvents(params));
	const deps = createResource(() => listDependencies());
	const diagnostics = createResource(() => getDiagnostics());

	// Live events prepend the REST page; ids are deduped against the page.
	let liveEvents = $state<EventRecord[]>([]);

	const merged = $derived.by(() => {
		const seen = new Set(liveEvents.map((event) => event.id));
		return [
			...liveEvents,
			...list.items.filter((event) => !seen.has(event.id)),
		];
	});

	const filtered = $derived(
		merged.filter((event) => {
			const needle = query.trim().toLowerCase();
			return (
				!needle ||
				event.type.toLowerCase().includes(needle) ||
				event.source.toLowerCase().includes(needle) ||
				event.payload.toLowerCase().includes(needle)
			);
		}),
	);

	async function purge(): Promise<void> {
		purging = true;
		try {
			await deleteAllEvents();
			toasts.success('Event store purged');
			purgeOpen = false;
			liveEvents = [];
			await list.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Failed to purge events');
		} finally {
			purging = false;
		}
	}

	async function metricsSnapshot(): Promise<void> {
		try {
			const size = await getEventSize();
			toasts.info(`${formatNumber(size)} events stored`);
		} catch (e) {
			toasts.error(
				e instanceof Error ? e.message : 'Failed to read event size',
			);
		}
	}

	$effect(() => {
		gotoWithParams(page.url, {
			tab,
			q: query,
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void list.loadPages(Number(initial.page) || 1);
		void deps.reload();
		void diagnostics.reload();
		const unsubscribe = live.subscribe((event) => {
			if (!behavior.streamFollow) return;
			liveEvents = [event, ...liveEvents].slice(0, 200);
		});
		return unsubscribe;
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Events & system"
		description="Event stream, dependency impact and runtime diagnostics."
	>
		{#snippet meta()}
			<span
				class="flex items-center gap-1.5 text-caption text-muted-foreground"
			>
				<span
					class={cn(
						'h-1.5 w-1.5 rounded-full',
						live.state === 'open' && 'bg-running animate-pulse-dot',
						live.state === 'connecting' && 'bg-warning',
						live.state === 'reconnecting' && 'bg-warning animate-pulse-dot',
						live.state === 'closed' && 'bg-muted-foreground',
					)}
				></span>
				{live.state === 'open'
					? 'stream live'
					: live.state === 'connecting'
						? 'stream connecting'
						: live.state === 'reconnecting'
							? 'stream reconnecting'
							: 'stream idle'}
			</span>
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => {
					list.reload();
					deps.reload();
					diagnostics.reload();
				}}
			/>
			<Button variant="outline" size="sm" onclick={() => (purgeOpen = true)}>
				<Icon name="trash" size={13} />
				Purge all events
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'stream'}
			<div class="mb-3 flex items-center gap-2">
				<div class="relative flex-1">
					<Icon
						name="search"
						size={14}
						class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
					/>
					<Input
						bind:value={query}
						placeholder="Filter by type, source or payload…"
						class="pl-8"
					/>
				</div>
				<span class="shrink-0 text-caption text-muted-foreground"
					>{filtered.length} events</span
				>
			</div>

			{#if list.loading && list.loaded === 0}
				<Card bodyClass="p-3">
					<div class="space-y-2">
						{#each Array.from({ length: 8 }, (_, position) => position) as index (index)}
							<Skeleton shape="block" height="34px" class="rounded-md" />
						{/each}
					</div>
				</Card>
			{:else if list.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load events"
					description={list.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => list.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				<Card bodyClass="p-0">
					<ul class="divide-y divide-border">
						{#each filtered as event (event.id)}
							<li>
								<button
									type="button"
									onclick={() =>
										(expandedId = expandedId === event.id ? null : event.id)}
									class="flex w-full items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-accent/40"
								>
									<span class="w-40 shrink-0 truncate font-mono text-caption"
										>{event.type}</span
									>
									<span
										class="min-w-0 flex-1 truncate text-caption text-muted-foreground"
									>
										{event.source}
									</span>
									{#if event.executionId}
										<Badge variant="outline" size="sm" class="shrink-0"
											>{event.executionId}</Badge
										>
									{/if}
									<span
										class="w-28 shrink-0 text-right text-micro tabular-nums text-muted-foreground"
									>
										{formatRelativeTime(event.at)}
									</span>
									<Icon
										name="chevron-down"
										size={13}
										class="shrink-0 text-muted-foreground transition-transform {expandedId ===
										event.id
											? 'rotate-180'
											: ''}"
									/>
								</button>
								{#if expandedId === event.id}
									<div
										class="animate-panel-in border-t border-border bg-muted/40 px-3 py-2"
									>
										<pre
											class="overflow-x-auto font-mono text-micro">{event.payload}</pre>
										<p class="mt-1 text-micro text-muted-foreground">
											{formatDateTime(event.at)}
										</p>
									</div>
								{/if}
							</li>
						{:else}
							<EmptyState
								icon="activity"
								title="No events match"
								description="Events appear here live once the stream delivers them."
								class="py-8"
							/>
						{/each}
					</ul>
				</Card>
			{/if}

			<LoadMorePager
				shown={list.loaded}
				hasMore={list.hasMore}
				loading={list.loading}
				pageSize={list.pageSize}
				onloadmore={() => list.loadMore()}
				class="mt-3 rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'dependencies'}
			{#if deps.loading && !deps.data}
				<Card bodyClass="p-3">
					<Skeleton lines={5} />
				</Card>
			{:else if deps.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load dependencies"
					description={deps.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => deps.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				<Card title="Callers and impact">
					<ul class="divide-y divide-border">
						{#each deps.data ?? [] as dependency (dependency.id)}
							<li
								class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0"
							>
								<div class="min-w-0">
									<p class="truncate text-caption">
										<span class="font-mono">{dependency.caller}</span>
										<Icon
											name="arrow-right"
											size={12}
											class="mx-1 inline text-muted-foreground"
										/>
										<span class="font-mono">{dependency.callee}</span>
									</p>
									<p class="mt-0.5 text-micro text-muted-foreground">
										{dependency.kind} · {formatRelativeTime(
											dependency.lastCalledAt,
										)}
									</p>
								</div>
								<span
									class="shrink-0 text-caption tabular-nums text-muted-foreground"
								>
									{formatNumber(dependency.calls)} calls
								</span>
							</li>
						{:else}
							<EmptyState
								icon="link"
								title="No dependencies recorded"
								class="py-6"
							/>
						{/each}
					</ul>
				</Card>
			{/if}
		{:else}
			{#if diagnostics.loading && !diagnostics.data}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each Array.from({ length: 3 }, (_, position) => position) as index (index)}
						<Skeleton shape="block" height="110px" class="rounded-lg" />
					{/each}
				</div>
			{:else if diagnostics.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load diagnostics"
					description={diagnostics.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button
							variant="link"
							size="sm"
							onclick={() => diagnostics.reload()}>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each diagnostics.data ?? [] as diagnostic (diagnostic.name)}
						<Card title={diagnostic.name}>
							{#snippet actions()}
								<StatusBadge status={diagnostic.status} size="sm" />
							{/snippet}
							<p class="text-heading font-semibold tabular-nums">
								{diagnostic.value}
							</p>
							<p class="mt-1 text-caption text-muted-foreground">
								{diagnostic.detail}
							</p>
						</Card>
					{/each}
				</div>

				<Card title="Runtime operations" class="mt-3">
					<div class="flex flex-wrap gap-2">
						<Button variant="outline" size="sm" onclick={metricsSnapshot}>
							<Icon name="chart" size={13} />
							Metrics snapshot
						</Button>
					</div>
				</Card>
			{/if}
		{/if}
	</div>
</div>

<Dialog bind:open={purgeOpen} title="Purge all events">
	<p class="text-caption text-muted-foreground">
		This deletes the whole event store through the backend. It cannot filter by
		the current search and the deletion is irreversible.
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (purgeOpen = false)}>
			Cancel
		</Button>
		<Button variant="destructive" size="sm" disabled={purging} onclick={purge}>
			{purging ? 'Purging…' : 'Purge everything'}
		</Button>
	{/snippet}
</Dialog>
