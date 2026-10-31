<script lang="ts">
	import Card from '#lib/ui/components/Card.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import {
		appendWorkflowNestedAgentLoopInput,
		convertWorkflowNestedAgentLoopPending,
		listWorkflowNestedAgentLoops,
		setWorkflowNestedAgentLoopAppendMode,
		type WorkflowNestedAgentLoop,
		type WorkflowNestedAppendMode,
	} from '#lib/services/agent-loops';
	import { watchExecutionActivity } from '#lib/services/inspector-activity';
	import { toasts } from '#lib/stores/toast.svelte';
	import type { StreamState } from '#lib/api/sse';
	import { SvelteMap } from 'svelte/reactivity';

	interface Props {
		executionId: string;
		live: boolean;
	}

	let { executionId, live }: Props = $props();

	/** Live refresh coalescing window; mirrors the inspector beat. */
	const REFRESH_DEBOUNCE_MS = 2000;
	/** Fallback poll while the run is live, in case the event stream stalls. */
	const POLL_MS = 15000;

	let loops = $state<WorkflowNestedAgentLoop[]>([]);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let seen = $state('');
	let streamState = $state<StreamState>('closed');
	let lastRefresh = 0;

	// Per-node composer state. Drafts are local-only: they live here, never
	// on the server, and a failed send keeps them untouched.
	let drafts = new SvelteMap<string, string>();
	let modes = new SvelteMap<string, WorkflowNestedAppendMode>();
	let busy = new SvelteMap<string, boolean>();
	let rowErrors = new SvelteMap<string, string | null>();

	function modeOf(loop: WorkflowNestedAgentLoop): WorkflowNestedAppendMode {
		const saved = modes.get(loop.nodeId);
		if (saved) return saved;
		return loop.appendMode === 'steering' ? 'steering' : 'followup';
	}

	async function load(id: string): Promise<void> {
		loadError = null;
		loading = true;
		try {
			loops = await listWorkflowNestedAgentLoops(id);
		} catch (e) {
			seen = '';
			loops = [];
			loadError = e instanceof Error ? e.message : 'Nested agent loops failed to load.';
		} finally {
			loading = false;
		}
	}

	/**
	 * Background refresh of counts and modes. It never clears the last good
	 * snapshot or the local drafts: a failed poll simply keeps what is on
	 * screen, and the error surfaces on the next manual reload.
	 */
	async function refresh(id: string): Promise<void> {
		try {
			loops = await listWorkflowNestedAgentLoops(id);
			if (seen === id) loadError = null;
		} catch {
			// Keep the rendered snapshot and drafts; manual retry reports.
		}
	}

	function scheduleRefresh(id: string): void {
		const now = Date.now();
		if (now - lastRefresh < REFRESH_DEBOUNCE_MS) return;
		lastRefresh = now;
		void refresh(id);
	}

	async function runAppend(loop: WorkflowNestedAgentLoop): Promise<void> {
		const nodeId = loop.nodeId;
		const text = (drafts.get(nodeId) ?? '').trim();
		if (!text || busy.get(nodeId)) return;
		busy.set(nodeId, true);
		rowErrors.set(nodeId, null);
		try {
			await appendWorkflowNestedAgentLoopInput(executionId, nodeId, text, modeOf(loop), 'web');
			drafts.set(nodeId, '');
			toasts.success('Input appended', nodeId);
			await refresh(executionId);
		} catch (e) {
			const reason = e instanceof Error ? e.message : 'Append failed.';
			// Local-only mark: the draft stays, and nothing claims the
			// server buffered it.
			rowErrors.set(nodeId, `Send failed, local draft kept: ${reason}`);
			toasts.error('Append failed', reason);
		} finally {
			busy.set(nodeId, false);
		}
	}

	async function runSetMode(loop: WorkflowNestedAgentLoop): Promise<void> {
		const nodeId = loop.nodeId;
		if (busy.get(nodeId)) return;
		busy.set(nodeId, true);
		rowErrors.set(nodeId, null);
		try {
			await setWorkflowNestedAgentLoopAppendMode(executionId, nodeId, modeOf(loop));
			toasts.success('Append mode updated', `${nodeId} defaults to ${modeOf(loop)}`);
			await refresh(executionId);
		} catch (e) {
			const reason = e instanceof Error ? e.message : 'Update failed.';
			rowErrors.set(nodeId, `Mode update failed, local selection kept: ${reason}`);
			toasts.error('Append mode update failed', reason);
		} finally {
			busy.set(nodeId, false);
		}
	}

	async function runConvert(loop: WorkflowNestedAgentLoop): Promise<void> {
		const nodeId = loop.nodeId;
		if (busy.get(nodeId)) return;
		busy.set(nodeId, true);
		rowErrors.set(nodeId, null);
		try {
			await convertWorkflowNestedAgentLoopPending(executionId, nodeId);
			toasts.success('Pending input converted', nodeId);
			await refresh(executionId);
		} catch (e) {
			const reason = e instanceof Error ? e.message : 'Convert failed.';
			rowErrors.set(nodeId, `Convert failed: ${reason}`);
			toasts.error('Convert failed', reason);
		} finally {
			busy.set(nodeId, false);
		}
	}

	$effect(() => {
		const id = executionId;
		if (!id || seen === id) return;
		seen = id;
		void load(id);
	});

	// Live runs follow the shared execution event stream (debounced) plus a
	// slow poll fallback; settled runs load once and stay static.
	$effect(() => {
		const id = executionId;
		if (!id || !live) return;
		const stop = watchExecutionActivity(id, {
			onEvent: () => scheduleRefresh(id),
			onTerminal: () => void refresh(id),
			onState: (state) => (streamState = state),
		});
		const timer = window.setInterval(() => void refresh(id), POLL_MS);
		return () => {
			stop();
			window.clearInterval(timer);
		};
	});
</script>

<Card
	title="Nested agent loops"
	description="Target one agent loop inside this workflow. Pending counts are server-buffered; drafts below stay local until sent."
>
	{#snippet actions()}
		{#if live}
			<span class="text-micro text-muted-foreground">
				{streamState === 'open' ? 'live' : streamState}
			</span>
		{/if}
		<IconButton
			icon="refresh"
			label="Reload nested agent loops"
			disabled={loading}
			onclick={() => void load(executionId)}
		/>
	{/snippet}

	{#if loading && loops.length === 0}
		<p class="text-caption text-muted-foreground">Loading nested agent loops…</p>
	{:else if loadError && loops.length === 0}
		<div class="flex flex-wrap items-center gap-2">
			<p class="text-caption text-destructive">{loadError}</p>
			<Button size="sm" variant="outline" onclick={() => void load(executionId)}>Retry</Button>
		</div>
	{:else if loops.length === 0}
		<p class="text-caption text-muted-foreground">No nested agent loops on this run.</p>
	{:else}
		<ul class="space-y-3">
			{#each loops as loop (loop.nodeId)}
				{@const nodeId = loop.nodeId}
				{@const rowBusy = busy.get(nodeId) === true}
				{@const draft = drafts.get(nodeId) ?? ''}
				{@const rowError = rowErrors.get(nodeId) ?? null}
				<li class="border-b border-border/60 pb-3 last:border-0 last:pb-0">
					<div class="flex flex-wrap items-center gap-2">
						<span class="font-mono text-caption text-foreground">{nodeId}</span>
						<StatusBadge status={loop.status} size="sm" />
						{#if loop.live}
							<Badge variant="running">live</Badge>
						{:else}
							<Badge variant="neutral">idle</Badge>
						{/if}
						<Badge variant="outline">{modeOf(loop)}</Badge>
						<span class="ml-auto text-micro tabular-nums text-muted-foreground">
							server buffered · steering {loop.steeringPending} · followup {loop.followupPending}
						</span>
					</div>
					<div class="mt-2 flex flex-wrap items-center gap-2">
						<Select
							value={modeOf(loop)}
							size="sm"
							class="w-32"
							aria-label={`Append mode for ${nodeId}`}
							options={[
								{ value: 'followup', label: 'followup' },
								{ value: 'steering', label: 'steering' },
							]}
							onchange={(next) => modes.set(nodeId, next as WorkflowNestedAppendMode)}
						/>
						<Button
							size="sm"
							variant="outline"
							disabled={rowBusy}
							onclick={() => void runSetMode(loop)}
						>
							Set default
						</Button>
						<Button
							size="sm"
							variant="outline"
							disabled={rowBusy || loop.followupPending === 0}
							title={loop.followupPending === 0
								? 'No followup input pending'
								: 'Convert followup input to steering'}
							onclick={() => void runConvert(loop)}
						>
							Convert pending
						</Button>
					</div>
					<div class="mt-2 flex items-center gap-2">
						<Input
							size="sm"
							class="min-w-0 flex-1"
							placeholder={`Append input for ${nodeId}`}
							aria-label={`Append input for ${nodeId}`}
							value={draft}
							oninput={(event) => drafts.set(nodeId, event.currentTarget.value)}
							onkeydown={(event) => {
								if (event.key === 'Enter' && !event.shiftKey) {
									event.preventDefault();
									void runAppend(loop);
								}
							}}
						/>
						<Button
							size="sm"
							disabled={rowBusy || !draft.trim()}
							onclick={() => void runAppend(loop)}
						>
							{rowBusy ? 'Sending…' : 'Append'}
						</Button>
					</div>
					{#if draft.trim()}
						<p class="mt-1 text-micro text-muted-foreground">
							Local draft, kept in this browser until sent.
						</p>
					{/if}
					{#if rowError}
						<p class="mt-1 text-micro text-destructive">{rowError}</p>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</Card>
