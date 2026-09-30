<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import type { Column } from '@wf-agent/ui/components/table';
	import JsonEditor from '@wf-agent/ui/components/JsonEditor.svelte';
	import { jsonErrorLine } from '$lib/services/templates';
	import {
		cleanupTriggerExecutions,
		listTriggerHistory,
		listTriggerExecutions,
		listHooks,
		fireHook,
	} from '$lib/services/triggers';
	import type { Hook, TriggerRecord } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime, formatRelativeTime } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const TABS = [
		{ id: 'records', label: 'Trigger records' },
		{ id: 'hooks', label: 'Hook test dispatch' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'records',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'records' ? '' : tab });
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

	onMount(() => {
		void loadTab(tab);
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
				const [history, executions] = await Promise.all([
					listTriggerHistory({ limit: 200 }),
					listTriggerExecutions({ limit: 200 }),
				]);
				const merged = [...history.items, ...executions.items];
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
					e instanceof Error ? e.message : 'Trigger records failed.';
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
				hooksError = e instanceof Error ? e.message : 'Hook registry failed.';
			} finally {
				hooksLoading = false;
			}
		}
	}

	async function reload(): Promise<void> {
		seenRecords = false;
		seenHooks = false;
		await loadTab(tab);
	}

	async function runCleanup(): Promise<void> {
		cleanupBusy = true;
		try {
			const removed = await cleanupTriggerExecutions();
			toasts.success(`Cleaned up ${removed} old trigger records`);
			cleanupArmed = false;
			seenRecords = false;
			await loadTab('records');
		} catch (e) {
			toasts.error(
				'Cleanup failed',
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
			payloadError = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		dispatchBusy = true;
		try {
			const result = await fireHook(hookName, parsed);
			lastDispatch = `Test dispatched to ${hookName}: ${result.status}`;
			toasts.success(lastDispatch);
		} catch (e) {
			lastDispatch = null;
			toasts.error(
				'Dispatch failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			dispatchBusy = false;
		}
	}

	const recordColumns: Column<TriggerRecord>[] = [
		{
			key: 'trigger',
			header: 'Trigger',
			text: (record) => record.triggerName,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'workflow',
			header: 'Workflow',
			text: (record) => record.workflowName,
		},
		{
			key: 'execution',
			header: 'Execution',
			text: (record) => record.executionId,
			cellClass: 'font-mono text-caption text-muted-foreground',
		},
		{ key: 'status', header: 'Status', cell: recordStatusCell },
		{
			key: 'fired',
			header: 'Fired',
			align: 'right',
			text: (record) => formatRelativeTime(record.firedAt),
			cellClass: 'text-caption text-muted-foreground',
		},
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Triggers & hooks"
		description="Fired trigger records and hook test dispatch."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			{#if cleanupArmed}
				<Button
					variant="outline"
					size="sm"
					disabled={cleanupBusy}
					onclick={() => void runCleanup()}
				>
					Confirm cleanup (30d+)
				</Button>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => (cleanupArmed = false)}
				>
					Keep
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					onclick={() => (cleanupArmed = true)}
				>
					<Icon name="trash" size={13} />
					Clean up old records
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
		aria-label="Trigger sections"
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if tab === 'records'}
			<PageState
				loading={recordsLoading}
				error={recordsError}
				errorTitle="Trigger records failed to load"
				empty={triggerRecords.length === 0}
				emptyTitle="No trigger records"
				emptyDescription="Fired triggers appear here once a workflow or hook fires."
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
				<Card title="Hooks">
					<PageState
						loading={hooksLoading}
						error={hooksError}
						errorTitle="Hook registry failed to load"
						empty={hooks.length === 0}
						emptyTitle="No hooks registered"
						emptyDescription="The backend exposes no hook registry endpoint, so there is nothing to pick. Enter a hook name manually on the right to dispatch a test payload."
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
												{hook.deliveries} deliveries
											</span>
										</span>
									</button>
								</li>
							{/each}
						</ul>
					</PageState>
				</Card>

				<Card title="Dispatch test payload">
					<div class="space-y-2">
						<label class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>Hook name{#if hooks.length === 0}
									<span class="ml-1 text-micro">· manual mode, no registry</span
									>{/if}</span
							>
							<Input
								bind:value={hookName}
								placeholder={hooks.length === 0
									? 'Enter hook name manually'
									: 'Pick from the list or type a name'}
							/>
						</label>
						<div class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>Payload</span
							>
							<JsonEditor
								bind:this={payloadEditor}
								bind:value={payload}
								errorLine={payloadErrorLine}
								placeholder={'{\n  "repo": "wf-agent"\n}'}
								label="Hook payload"
								minHeight="10rem"
							/>
						</div>
						{#if payloadError}
							<div
								class="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
							>
								<p class="text-caption text-destructive">
									JSON syntax{#if payloadErrorLine}
										(line {payloadErrorLine}){/if}: {payloadError}
								</p>
								{#if payloadErrorLine}
									<Button
										variant="ghost"
										size="sm"
										onclick={() =>
											payloadEditor?.scrollToLine(payloadErrorLine ?? 1)}
									>
										Go to line
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
								{dispatchBusy ? 'Sending…' : 'Send test'}
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
							Last delivery {formatDateTime(
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
