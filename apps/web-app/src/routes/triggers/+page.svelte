<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import DataTable from '#lib/ui/components/DataTable.svelte';
	import type { Column } from '#lib/ui/components/table';
	import JsonEditor from '#lib/ui/components/JsonEditor.svelte';
	import { jsonErrorLine } from '#lib/services/templates';
	import {
		cleanupTriggerExecutions,
		listTriggerExecutions,
		listHooks,
		fireHook,
	} from '#lib/services/triggers';
	import type { Hook, TriggerRecord } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t, message } from '#lib/i18n';
	import { formatDateTime, formatRelativeTime } from '#lib/utils/format';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { watchListInvalidation } from '#lib/services/list-invalidation';

	const TABS = [
		{ id: 'records', label: message('page.triggers.recordsTab') },
		{ id: 'hooks', label: message('page.triggers.hooksTab') },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'records',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, {
			tab: tab === 'records' ? '' : tab,
		});
	});
	let triggerRecords = $state<TriggerRecord[]>([]);
	let hooks = $state<Hook[]>([]);
	let hookName = $state('');
	let payload = $state('{\n  "repo": "wf-agent",\n  "branch": "main"\n}');
	let payloadError = $state<string | null>(null);
	let payloadEditor = $state<{ scrollToLine: (line: number) => void } | null>(
		null,
	);
	let dispatchBusy = $state(false);
	let lastDispatch = $state<string | null>(null);
	let recordsError = $state<string | null>(null);
	let recordsLoading = $state(false);
	let hooksError = $state<string | null>(null);
	let hooksLoading = $state(false);
	let cleanupArmed = $state(false);
	let cleanupBusy = $state(false);

	const payloadErrorLine = $derived(
		payloadError ? jsonErrorLine(payload, payloadError) : null,
	);

	/** Segment sources already pulled, so a tab loads once. */
	let seenRecords = $state(false);
	let seenHooks = $state(false);
	let triggerUpdates = $state(false);
	let stopTriggerLive: (() => void) | null = null;

	onMount(() => {
		void loadTab(tab);
		stopTriggerLive = watchListInvalidation({
			onDirty: () => {
				if (recordsLoading || tab !== 'records') {
					triggerUpdates = true;
					return;
				}
				if (typeof document !== 'undefined' && document.hidden) {
					triggerUpdates = true;
					return;
				}
				seenRecords = false;
				void loadTab('records');
			},
		});
	});

	onDestroy(() => {
		stopTriggerLive?.();
		stopTriggerLive = null;
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		if (current === 'records' && !seenRecords) {
			seenRecords = true;
			recordsLoading = true;
			recordsError = null;
			try {
				const executions = await listTriggerExecutions({ limit: 200 });
				const merged = [...executions.items];
				const seen: string[] = [];
				triggerRecords = merged.filter((row) => {
					if (seen.includes(row.id)) return false;
					seen.push(row.id);
					return true;
				});
			} catch (e) {
				seenRecords = false;
				triggerRecords = [];
				recordsError =
					e instanceof Error ? e.message : t('page.triggers.recordsLoadFailed');
			} finally {
				recordsLoading = false;
			}
		} else if (current === 'hooks' && !seenHooks) {
			seenHooks = true;
			hooksLoading = true;
			hooksError = null;
			try {
				hooks = await listHooks();
				if (!hookName && hooks.length > 0) hookName = hooks[0].name;
			} catch (e) {
				seenHooks = false;
				hooks = [];
				hooksError =
					e instanceof Error ? e.message : t('page.triggers.hooksLoadFailed');
			} finally {
				hooksLoading = false;
			}
		}
	}

	async function reload(): Promise<void> {
		seenRecords = false;
		seenHooks = false;
		triggerUpdates = false;
		await loadTab(tab);
	}

	async function runCleanup(): Promise<void> {
		cleanupBusy = true;
		try {
			const removed = await cleanupTriggerExecutions();
			toasts.success(t('page.triggers.cleanedUp', { count: removed }));
			cleanupArmed = false;
			seenRecords = false;
			await loadTab('records');
		} catch (e) {
			toasts.error(
				t('page.triggers.cleanupFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			cleanupBusy = false;
		}
	}

	async function dispatch(): Promise<void> {
		if (!hookName || dispatchBusy) return;
		let parsed: unknown;
		try {
			parsed = JSON.parse(payload);
			payloadError = null;
		} catch (e) {
			payloadError =
				e instanceof Error ? e.message : t('page.triggers.invalidJson');
			return;
		}
		dispatchBusy = true;
		try {
			const result = await fireHook(hookName, parsed);
			lastDispatch = t('page.triggers.dispatched', {
				hook: hookName,
				status: result.status,
			});
			toasts.success(lastDispatch);
		} catch (e) {
			lastDispatch = null;
			toasts.error(
				t('page.triggers.dispatchFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			dispatchBusy = false;
		}
	}

	const recordColumns: Column<TriggerRecord>[] = [
		{
			key: 'trigger',
			header: t('page.triggers.columnTrigger'),
			text: (record) => record.triggerName,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'workflow',
			header: t('page.triggers.columnWorkflow'),
			text: (record) => record.workflowName,
		},
		{
			key: 'execution',
			header: t('page.triggers.columnExecution'),
			text: (record) => record.executionId,
			cellClass: 'font-mono text-caption text-muted-foreground',
		},
		{
			key: 'status',
			header: t('page.triggers.columnStatus'),
			cell: recordStatusCell,
		},
		{
			key: 'fired',
			header: t('page.triggers.columnFired'),
			align: 'right',
			text: (record) => formatRelativeTime(record.firedAt),
			cellClass: 'text-caption text-muted-foreground',
		},
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={t('page.triggers.title')}
		description={t('page.triggers.description')}
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label={t('common.refresh')}
				onclick={() => void reload()}
			/>
			{#if cleanupArmed}
				<Button
					variant="outline"
					size="sm"
					disabled={cleanupBusy}
					onclick={() => void runCleanup()}
				>
					{t('page.triggers.cleanupConfirm')}
				</Button>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => (cleanupArmed = false)}
				>
					{t('page.executionDetail.keep')}
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					onclick={() => (cleanupArmed = true)}
				>
					<Icon name="trash" size={13} />
					{t('page.triggers.cleanup')}
				</Button>
			{/if}
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		bind:value={tab}
		class="px-4"
		panelId="triggers-panel"
	/>

	<div
		id="triggers-panel"
		role="tabpanel"
		aria-label={t('page.triggers.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if triggerUpdates && tab === 'records'}
			<div
				class="mb-3 flex items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2"
			>
				<span class="text-caption text-muted-foreground">
					New trigger activity arrived.
				</span>
				<Button
					size="sm"
					variant="outline"
					onclick={() => {
						triggerUpdates = false;
						seenRecords = false;
						void loadTab('records');
					}}
				>
					Refresh
				</Button>
			</div>
		{/if}
		{#if tab === 'records'}
			<PageState
				loading={recordsLoading}
				error={recordsError}
				errorTitle={t('page.triggers.recordsLoadFailed')}
				empty={triggerRecords.length === 0}
				emptyTitle={t('page.triggers.noRecords')}
				emptyDescription={t('page.triggers.noRecordsDesc')}
				onretry={() => void reload()}
			>
				<Card bodyClass="p-0">
					<DataTable
						columns={recordColumns}
						rows={triggerRecords}
						rowKey={(record) => record.id}
						virtualize={false}
					/>
				</Card>
			</PageState>
		{:else}
			<div class="grid gap-3 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
				<Card title={t('page.triggers.hooks')}>
					<PageState
						loading={hooksLoading}
						error={hooksError}
						errorTitle={t('page.triggers.hooksLoadFailed')}
						empty={hooks.length === 0}
						emptyTitle={t('page.triggers.noHooks')}
						emptyDescription={t('page.triggers.noHooksDesc')}
						onretry={() => void reload()}
					>
						<ul class="space-y-1">
							{#each hooks as hook (hook.name)}
								<li>
									<button
										type="button"
										onclick={() => (hookName = hook.name)}
										class="w-full rounded-md border px-2.5 py-2 text-left transition-colors {hookName ===
										hook.name
											? 'border-[hsl(var(--ring))] bg-accent'
											: 'border-transparent hover:bg-accent/60'}"
									>
										<span class="block font-mono text-caption">{hook.name}</span
										>
										<span class="block text-micro text-muted-foreground"
											>{hook.description}</span
										>
										<span class="mt-1 flex items-center gap-2">
											<StatusBadge status={hook.lastStatus} size="sm" />
											<span class="text-micro text-muted-foreground">
												{t('page.triggers.deliveries', {
													count: hook.deliveries,
												})}
											</span>
										</span>
									</button>
								</li>
							{/each}
						</ul>
					</PageState>
				</Card>

				<Card title={t('page.triggers.dispatchPayload')}>
					<div class="space-y-2">
						<label class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>{t('page.triggers.hookName')}{#if hooks.length === 0}
									<span class="ml-1 text-micro"
										>{t('page.triggers.manualMode')}</span
									>{/if}</span
							>
							<Input
								bind:value={hookName}
								placeholder={hooks.length === 0
									? t('page.triggers.hookNameManual')
									: t('page.triggers.hookNamePick')}
							/>
						</label>
						<div class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>{t('page.triggers.payload')}</span
							>
							<JsonEditor
								bind:this={payloadEditor}
								bind:value={payload}
								errorLine={payloadErrorLine}
								placeholder={'{\n  "repo": "wf-agent"\n}'}
								label={t('page.triggers.hookPayload')}
								minHeight="10rem"
							/>
						</div>
						{#if payloadError}
							<div
								class="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
							>
								<p class="text-caption text-destructive">
									{t('page.triggers.syntaxError', {
										line: payloadErrorLine ? ` (${payloadErrorLine})` : '',
										error: payloadError,
									})}
								</p>
								{#if payloadErrorLine}
									<Button
										variant="ghost"
										size="sm"
										onclick={() =>
											payloadEditor?.scrollToLine(payloadErrorLine ?? 1)}
									>
										{t('page.resources.toolRun.goToLine')}
									</Button>
								{/if}
							</div>
						{/if}
						<div class="flex items-center gap-2">
							<Button
								size="sm"
								disabled={dispatchBusy || !hookName.trim()}
								onclick={() => void dispatch()}
							>
								<Icon name="zap" size={13} />
								{dispatchBusy
									? t('page.triggers.sending')
									: t('page.triggers.sendTest')}
							</Button>
						</div>
						{#if lastDispatch}
							<p
								class="rounded-md border border-border bg-muted/40 px-2 py-1.5 text-caption"
							>
								{lastDispatch}
							</p>
						{/if}
						<p class="text-micro text-muted-foreground">
							{t('page.triggers.lastDelivery')}
							{formatDateTime(
								hooks.find((hook) => hook.name === hookName)?.lastDeliveredAt ??
									null,
							)}
						</p>
					</div>
				</Card>
			</div>
		{/if}
	</div>
</div>

{#snippet recordStatusCell(record: TriggerRecord)}
	<StatusBadge status={record.status} size="sm" />
{/snippet}
