<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import {
		listTriggerHistory,
		listHooks,
		fireHook,
	} from '$lib/services/triggers';
	import type { TriggerRecord } from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime, formatRelativeTime } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'records', label: 'Trigger records' },
		{ id: 'hooks', label: 'Hook test dispatch' },
	];

	const initial = parseListParams(page.url);
	let tab = $state(initial.tab ?? 'records');
	let hookName = $state('');
	let payload = $state('{\n  "repo": "wf-agent",\n  "branch": "main"\n}');
	let dispatching = $state(false);

	const list = createCollection((params) => listTriggerHistory(params));
	// No bulk hook-list endpoint exists yet, so this stays empty until one lands.
	const hooks = createResource(() => listHooks());

	async function sendTest(): Promise<void> {
		if (!hookName.trim()) {
			toasts.warning('Enter a hook name first');
			return;
		}
		let parsed: unknown;
		try {
			parsed = JSON.parse(payload);
		} catch {
			toasts.error('Payload is not valid JSON');
			return;
		}
		dispatching = true;
		try {
			const result = await fireHook(hookName.trim(), parsed);
			toasts.success(`Dispatch to ${hookName.trim()} · ${result.status}`);
			await list.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Dispatch failed');
		} finally {
			dispatching = false;
		}
	}

	$effect(() => {
		gotoWithParams(page.url, {
			tab,
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void list.loadPages(Number(initial.page) || 1);
		void hooks.reload();
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Triggers & hooks"
		description="Fired trigger records and hook test dispatch with idempotency keys."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => {
					list.reload();
					hooks.reload();
				}}
			/>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'records'}
			{#if list.loading && list.loaded === 0}
				<Card bodyClass="p-3">
					<div class="space-y-2">
						{#each Array.from({ length: 6 }, (_, position) => position) as index (index)}
							<Skeleton shape="block" height="34px" class="rounded-md" />
						{/each}
					</div>
				</Card>
			{:else if list.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load trigger records"
					description={list.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => list.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else if list.loaded === 0}
				<EmptyState
					icon="zap"
					title="No trigger records"
					description="Records appear once a trigger fires."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				{#snippet recordTrigger(record: TriggerRecord)}
					<span class="font-mono text-caption">{record.triggerName}</span>
				{/snippet}
				{#snippet recordExecution(record: TriggerRecord)}
					<span class="font-mono text-caption text-muted-foreground">
						{record.executionId}
					</span>
				{/snippet}
				{#snippet recordStatus(record: TriggerRecord)}
					<StatusBadge status={record.status} size="sm" />
				{/snippet}
				{#snippet recordFired(record: TriggerRecord)}
					<span class="text-caption text-muted-foreground">
						{formatRelativeTime(record.firedAt)}
					</span>
				{/snippet}
				<Card bodyClass="p-0">
					<DataTable
						rows={list.items}
						rowKey={(row) => row.id}
						columns={[
							{ key: 'trigger', header: 'Trigger', cell: recordTrigger },
							{
								key: 'workflow',
								header: 'Workflow',
								text: (row) => row.workflowName,
							},
							{ key: 'execution', header: 'Execution', cell: recordExecution },
							{ key: 'status', header: 'Status', cell: recordStatus },
							{
								key: 'fired',
								header: 'Fired',
								align: 'right',
								cell: recordFired,
							},
						]}
					/>
					<LoadMorePager
						shown={list.loaded}
						hasMore={list.hasMore}
						loading={list.loading}
						pageSize={list.pageSize}
						onloadmore={() => list.loadMore()}
					/>
				</Card>
			{/if}
		{:else}
			<div class="grid gap-3 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
				<Card title="Hooks">
					{#if hooks.loading && !hooks.data}
						<div class="space-y-2">
							{#each Array.from({ length: 3 }, (_, position) => position) as index (index)}
								<Skeleton shape="block" height="56px" class="rounded-md" />
							{/each}
						</div>
					{:else if hooks.error}
						<p class="text-caption text-destructive">{hooks.error}</p>
					{:else if !hooks.data || hooks.data.length === 0}
						<EmptyState
							icon="link"
							title="No hooks to list"
							description="The backend exposes no bulk hook listing yet; dispatch by name below."
							class="py-6"
						/>
					{:else}
						<ul class="space-y-1">
							{#each hooks.data as hook (hook.name)}
								<li>
									<button
										type="button"
										onclick={() => (hookName = hook.name)}
										class="w-full rounded-md border px-2.5 py-2 text-left transition-colors {hookName ===
										hook.name
											? 'border-ring bg-accent'
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
					{/if}
				</Card>

				<Card title="Dispatch test payload">
					<div class="space-y-2">
						<label class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>Hook name</span
							>
							<Input bind:value={hookName} />
						</label>
						<label class="block">
							<span class="mb-1 block text-caption text-muted-foreground"
								>Payload</span
							>
							<Textarea
								bind:value={payload}
								class="min-h-40 font-mono text-caption"
							/>
						</label>
						<div class="flex items-center gap-2">
							<Button size="sm" disabled={dispatching} onclick={sendTest}>
								<Icon name="zap" size={13} />
								{dispatching ? 'Dispatching…' : 'Send test'}
							</Button>
						</div>
						<p class="text-micro text-muted-foreground">
							Last delivery {formatDateTime(
								hooks.data?.find((hook) => hook.name === hookName)
									?.lastDeliveredAt ?? null,
							)}
						</p>
					</div>
				</Card>
			</div>
		{/if}
	</div>
</div>
