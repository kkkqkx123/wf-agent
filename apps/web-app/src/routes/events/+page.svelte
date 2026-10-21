<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import {
		listEvents,
		listDependencies,
		getDiagnostics,
		deleteAllEvents,
	} from '#lib/services/events';
	import type { Dependency, Diagnostic, EventRecord } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatNumber,
		formatRelativeTime,
	} from '#lib/utils/format';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { t } from '#lib/i18n';

	const TABS = [
		{ id: 'stream', label: t('page.events.stream') },
		{ id: 'dependencies', label: t('page.events.dependencies') },
		{ id: 'operations', label: t('page.events.operations') },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'stream',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, { tab: tab === 'stream' ? '' : tab });
	});
	let query = $state('');
	let expandedId = $state<string | null>(null);
	let events = $state<EventRecord[]>([]);
	let dependencies = $state<Dependency[]>([]);
	let diagnostics = $state<Diagnostic[]>([]);
	let loading = $state(false);
	let loadError = $state<string | null>(null);

	/** Segment sources already pulled, so a tab loads once. */
	let seenStream = $state(false);
	let seenDeps = $state(false);
	let seenOps = $state(false);

	onMount(() => {
		void loadTab(tab);
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		const pending =
			current === 'stream'
				? !seenStream
				: current === 'dependencies'
					? !seenDeps
					: !seenOps;
		if (!pending) return;
		loading = true;
		loadError = null;
		try {
			if (current === 'stream') {
				seenStream = true;
				const eventPage = await listEvents({ limit: 200 });
				events = eventPage.items;
			} else if (current === 'dependencies') {
				seenDeps = true;
				dependencies = await listDependencies();
			} else {
				seenOps = true;
				diagnostics = await getDiagnostics();
			}
		} catch (e) {
			console.error('Failed to load events segment:', e);
			loadError = e instanceof Error ? e.message : t('page.events.requestFailed');
		} finally {
			loading = false;
		}
	}

	async function reload(): Promise<void> {
		seenStream = false;
		seenDeps = false;
		seenOps = false;
		await loadTab(tab);
	}

	async function clearEvents(): Promise<void> {
		try {
			await deleteAllEvents();
			await reload();
			toasts.success(t('page.events.cleared'));
		} catch {
			toasts.error(t('page.events.deletionRequiresConfirmation'));
		}
	}

	const filtered = $derived(
		events.filter((event) => {
			const needle = query.trim().toLowerCase();
			return (
				!needle ||
				event.type.toLowerCase().includes(needle) ||
				event.source.toLowerCase().includes(needle) ||
				event.payload.toLowerCase().includes(needle)
			);
		}),
	);

	function emptyCopyFor(current: string): {
		title: string;
		description: string;
	} {
		if (current === 'dependencies') {
			return {
				title: t('page.events.noDependencies'),
				description:
					t('page.events.noDependenciesDesc'),
			};
		}
		if (current === 'operations') {
			return {
				title: t('page.events.noDiagnostics'),
				description:
					t('page.events.noDiagnosticsDesc'),
			};
		}
		return {
			title: query.trim() ? t('page.events.noMatching') : t('page.events.noCaptured'),
			description: query.trim()
				? t('page.events.noMatchingDesc')
				: t('page.events.noCapturedDesc'),
		};
	}

	const isEmpty = $derived(
		tab === 'stream'
			? filtered.length === 0
			: tab === 'dependencies'
				? dependencies.length === 0
				: diagnostics.length === 0,
	);

	const emptyCopy = $derived(emptyCopyFor(tab));
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={t('page.events.title')}
		description={t('page.events.description')}
	>
		{#snippet meta()}
			<span
				class="flex items-center gap-1.5 text-caption text-muted-foreground"
			>
				<span
					class="h-1.5 w-1.5 rounded-full bg-running animate-pulse-dot"
					aria-hidden="true"
				></span>
				stream subscribed
			</span>
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label={t('common.refresh')}
				onclick={() => void reload()}
			/>
			<Button variant="outline" size="sm" onclick={() => void clearEvents()}>
				<Icon name="trash" size={13} />
				Delete filtered
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		bind:value={tab}
		class="px-4"
		panelId="events-panel"
	/>

	<div
		id="events-panel"
		role="tabpanel"
		aria-label={t('page.events.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
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
						placeholder={t('page.events.filterPlaceholder')}
						class="pl-8"
					/>
				</div>
				<span class="shrink-0 text-caption text-muted-foreground"
					>{filtered.length} events</span
				>
			</div>
		{/if}

		<PageState
			{loading}
			error={loadError}
			empty={isEmpty}
			emptyTitle={emptyCopy.title}
			emptyDescription={emptyCopy.description}
			onretry={() => void reload()}
		>
			{#if tab === 'stream'}
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
										<Badge variant="outline" class="shrink-0 text-[0.625rem]"
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
						{/each}
					</ul>
				</Card>
			{:else if tab === 'dependencies'}
				<Card title={t('page.events.callersImpact')}>
					<ul class="divide-y divide-border">
						{#each dependencies as dependency (dependency.id)}
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
						{/each}
					</ul>
				</Card>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each diagnostics as diagnostic (diagnostic.name)}
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

				<Card title={t('page.events.runtimeOperations')} class="mt-3">
					<div class="flex flex-wrap gap-2">
						<Button
							variant="outline"
							size="sm"
							onclick={() => toasts.info(t('page.events.storageDiagnosticsQueued'))}
						>
							<Icon name="database" size={13} />
							{t('page.events.storageDiagnostics')}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => toasts.info(t('page.events.systemDiagnosticsQueued'))}
						>
							<Icon name="gauge" size={13} />
							{t('page.events.systemDiagnostics')}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => toasts.info(t('page.events.metricsSnapshotQueued'))}
						>
							<Icon name="chart" size={13} />
							{t('page.events.metricsSnapshot')}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() =>
								toasts.warning(t('page.events.expiryCleanupConfirm'))}
						>
							<Icon name="clock" size={13} />
							{t('page.events.cleanExpired')}
						</Button>
					</div>
				</Card>
			{/if}
		</PageState>
	</div>
</div>
