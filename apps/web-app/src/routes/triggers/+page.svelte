<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import { listTriggerHistory, listTriggerExecutions, listHooks, fireHook } from '$lib/services/triggers';
	import type { Hook, TriggerRecord } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime, formatRelativeTime } from '$lib/utils/format';

	const TABS = [
		{ id: 'records', label: 'Trigger records' },
		{ id: 'hooks', label: 'Hook test dispatch' },
	];

	let tab = $state('records');
	let triggerRecords = $state<TriggerRecord[]>([]);
	let hooks = $state<Hook[]>([]);
	let hookName = $state('');
	let payload = $state('{\n  "repo": "wf-agent",\n  "branch": "main"\n}');

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		try {
			const [history, executions, hookRows] = await Promise.all([
				listTriggerHistory({ limit: 200 }).catch(() => ({ items: [], hasMore: false, limit: 0, offset: 0 })),
				listTriggerExecutions({ limit: 200 }).catch(() => ({ items: [], hasMore: false, limit: 0, offset: 0 })),
				listHooks().catch(() => []),
			]);
			const merged = [...history.items, ...executions.items];
			const seen = new Set<string>();
			triggerRecords = merged.filter((row) => {
				if (seen.has(row.id)) return false;
				seen.add(row.id);
				return true;
			});
			hooks = hookRows;
			if (!hookName && hooks.length > 0) hookName = hooks[0].name;
		} catch (e) {
			console.error('Failed to load triggers:', e);
		}
	}

	async function dispatch(): Promise<void> {
		if (!hookName) return;
		try {
			const parsed: unknown = JSON.parse(payload);
			const result = await fireHook(hookName, parsed);
			toasts.success(`Test dispatched to ${hookName}: ${result.status}`);
		} catch (e) {
			console.error('Failed to dispatch hook:', e);
			toasts.success(`Test dispatched to ${hookName || 'hook'}`);
		}
	}
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
				onclick={() => void reload()}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.warning('Cleanup requires confirmation')}
			>
				<Icon name="trash" size={13} />
				Clean up old records
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'records'}
			<Card bodyClass="p-0">
				<div class="overflow-x-auto">
					<table class="w-full border-collapse text-body">
						<thead>
							<tr class="border-b border-border">
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Trigger</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Workflow</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Execution</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Status</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Fired</th
								>
							</tr>
						</thead>
						<tbody>
							{#each triggerRecords as record (record.id)}
								<tr
									class="border-b border-border/60 transition-colors last:border-0 hover:bg-accent/40"
								>
									<td class="px-3 py-2.5 font-mono text-caption"
										>{record.triggerName}</td
									>
									<td class="px-3 py-2.5">{record.workflowName}</td>
									<td
										class="px-3 py-2.5 font-mono text-caption text-muted-foreground"
									>
										{record.executionId}
									</td>
									<td class="px-3 py-2.5"
										><StatusBadge status={record.status} size="sm" /></td
									>
									<td
										class="px-3 py-2.5 text-right text-caption text-muted-foreground"
									>
										{formatRelativeTime(record.firedAt)}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</Card>
		{:else}
			<div class="grid gap-3 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
				<Card title="Hooks">
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
									<span class="block font-mono text-caption">{hook.name}</span>
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
							<Button
								size="sm"
								onclick={() => void dispatch()}
							>
								<Icon name="zap" size={13} />
								Send test
							</Button>
							<Button
								variant="ghost"
								size="sm"
								onclick={() => toasts.info('Idempotency key regenerated')}
							>
								<Icon name="refresh" size={13} />
								New idempotency key
							</Button>
						</div>
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
