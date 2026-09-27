<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ToolCallEntry } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import Button from '$lib/components/ui/Button.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import JsonViewer from './JsonViewer.svelte';
	import { approveApproval, rejectApproval } from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		entry: ToolCallEntry;
		class?: string;
	}

	let { entry, class: className = '' }: Props = $props();

	let open = $state(false);
	let resultOpen = $state(false);
	let decisionPending = $state(false);

	/** Outputs beyond this length open in a dialog instead of inline. */
	const INLINE_OUTPUT_LIMIT = 2000;

	const isApproval = $derived(entry.kind === 'approval');
	const isGateway = $derived(entry.kind === 'network');
	const isScript = $derived(
		entry.kind === 'bash' || entry.kind === 'script',
	);
	const hasLargeOutput = $derived(entry.output.length > INLINE_OUTPUT_LIMIT);

	function reviewApprovals(): void {
		void goto(resolve('/checkpoints?tab=approvals'));
	}

	async function decide(approved: boolean): Promise<void> {
		const id = entry.approvalId;
		if (!id) {
			reviewApprovals();
			return;
		}
		decisionPending = true;
		try {
			if (approved) {
				await approveApproval(id);
				toasts.success('Approval granted');
			} else {
				await rejectApproval(id);
				toasts.warning('Approval rejected');
			}
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error('Approval decision failed');
		} finally {
			decisionPending = false;
		}
	}

	const KIND_ICON: Record<string, IconName> = {
		bash: 'terminal',
		script: 'terminal',
		file: 'file',
		search: 'search',
		approval: 'shield',
		mcp: 'blocks',
		network: 'link',
		memory: 'database',
		knowledge: 'layers',
		agent: 'cpu',
		interaction: 'zap',
		workflow: 'workflow',
		utility: 'command',
		risk: 'alert-triangle',
	};

	const KIND_TONE: Record<string, string> = {
		bash: 'text-success',
		script: 'text-success',
		file: 'text-info',
		search: 'text-warning',
		approval: 'text-running',
		mcp: 'text-muted-foreground',
		network: 'text-muted-foreground',
		memory: 'text-info',
		knowledge: 'text-info',
		agent: 'text-warning',
		interaction: 'text-running',
		workflow: 'text-info',
		utility: 'text-muted-foreground',
		risk: 'text-warning',
	};

	const icon = $derived(KIND_ICON[entry.kind] ?? 'blocks');
	const tone = $derived(KIND_TONE[entry.kind] ?? 'text-muted-foreground');
</script>

<article
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-card',
		className,
	)}
>
	<button
		type="button"
		onclick={() => (open = !open)}
		aria-expanded={open}
		class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
	>
		<Icon name={icon} size={14} class={cn('shrink-0', tone)} />
		<span class="min-w-0 flex-1 truncate font-mono text-caption text-foreground"
			>{entry.name}</span
		>
		<span class="shrink-0 text-micro tabular-nums text-muted-foreground">
			{formatDuration(entry.durationMs)}
		</span>
		<StatusBadge status={entry.status} size="sm" />
		<Icon
			name="chevron-down"
			size={14}
			class={cn(
				'shrink-0 text-muted-foreground transition-transform duration-150',
				open && 'rotate-180',
			)}
		/>
	</button>

	{#if open}
		<div class="animate-panel-in space-y-2 border-t border-border px-3 py-2.5">
			{#if isGateway && entry.endpoint}
				<p class="truncate font-mono text-micro text-muted-foreground">
					{entry.endpoint}
				</p>
			{/if}
			{#if isScript && entry.exitCode !== null && entry.exitCode !== undefined}
				<p class="text-micro tabular-nums text-muted-foreground">
					exit code {entry.exitCode}
				</p>
			{/if}
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					Input
				</p>
				<JsonViewer value={entry.input} collapsed />
			</div>
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					Output
				</p>
				<JsonViewer value={entry.output} />
			</div>
			{#if isApproval}
				<div
					class="flex items-center justify-between gap-2 rounded-md bg-muted px-2 py-1.5"
				>
					<p class="text-micro text-muted-foreground">
						Resolution happens in the approvals queue
					</p>
					<div class="flex shrink-0 items-center gap-1.5">
						{#if entry.approvalId}
							<Button
								variant="outline"
								size="sm"
								disabled={decisionPending}
								onclick={() => void decide(true)}
							>
								<Icon name="check" size={13} />
								Approve
							</Button>
							<Button
								variant="ghost"
								size="sm"
								disabled={decisionPending}
								onclick={() => void decide(false)}
							>
								<Icon name="x" size={13} />
								Reject
							</Button>
						{:else}
							<Button variant="outline" size="sm" onclick={reviewApprovals}>
								Review
							</Button>
						{/if}
					</div>
				</div>
			{/if}
			{#if hasLargeOutput}
				<div class="flex justify-end">
					<Button variant="ghost" size="sm" onclick={() => (resultOpen = true)}>
						<Icon name="maximize" size={13} />
						Full result
					</Button>
				</div>
			{/if}
		</div>
	{/if}

	<Dialog
		bind:open={resultOpen}
		title={entry.name}
		description="Complete tool result"
	>
		<JsonViewer value={entry.output} maxLength={32000} />
	</Dialog>
</article>
