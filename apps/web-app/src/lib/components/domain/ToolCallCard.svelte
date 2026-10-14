<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ToolCallEntry } from '$lib/types/models';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';	import type { IconName } from '@wf-agent/ui/icons/paths';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import type { StatusTone } from '@wf-agent/ui/status';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import { approveApproval, rejectApproval } from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';
	import { resolveToolOutputBudgets } from '$lib/config/tool-output';
	import {
		countSummary,
		diffLineTone,
		hidesBody,
		inferDisplayKind,
		limitBodyLines,
		previewFor,
	} from '$lib/utils/tool-output';
	import type { DisplayPayload } from '$lib/utils/tool-output';
	import { preferences } from '$lib/stores/preferences.svelte';

	interface Props {
		entry: ToolCallEntry;
		class?: string;
	}

	let { entry, class: className = '' }: Props = $props();

	let open = $state(false);
	let resultOpen = $state(false);
	let decisionPending = $state(false);
	let showFullOutput = $state(false);

	const PREVIEW_PRE =
		'max-h-48 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro break-words whitespace-pre-wrap text-foreground';

	const failed = $derived(entry.status === 'failed');
	const previews = $derived(preferences.toolOutputPreviews);
	const budgets = $derived(
		resolveToolOutputBudgets(preferences.toolOutputDetail),
	);
	const displayKind = $derived(
		entry.displayKind ?? inferDisplayKind(entry.name),
	);
	const display = $derived<DisplayPayload | null>(
		entry.displayKind
			? {
					kind: entry.displayKind,
					preview: entry.displayPreview ?? '',
					truncated: entry.displayTruncated ?? false,
					hiddenLines: 0,
				}
			: null,
	);
	const previewInput = $derived({
		output: entry.output,
		error: entry.error,
		success: !failed,
		display,
	});
	const hiddenBody = $derived(hidesBody(entry.name, previewInput));
	const preview = $derived(previewFor(entry.name, previewInput, budgets));
	const previewLines = $derived(preview.text.split('\n'));
	const countText = $derived(countSummary(entry.output, entry.displayPreview));
	const fullBody = $derived(limitBodyLines(entry.output, budgets.bodyMaxLines));
	const expandable = $derived(hiddenBody || preview.overflow);
	const headerHint = $derived(
		!previews
			? null
			: hiddenBody
				? `· ${countText}`
				: preview.overflow
					? preview.hiddenLines > 0
						? `· +${preview.hiddenLines} lines`
						: '· +more'
					: null,
	);

	function diffToneClass(line: string): string {
		if (displayKind !== 'diff') return '';
		const tone = diffLineTone(line);
		if (tone === 'add') return 'text-success';
		if (tone === 'remove') return 'text-destructive';
		if (tone === 'hunk') return 'text-info';
		return '';
	}

	// A new tool call starts collapsed; live updates of the same call keep
	// the user's expansion choice.
	$effect(() => {
		void entry.id;
		showFullOutput = false;
	});

	/** Outputs beyond this length open in a dialog instead of inline. */
	const INLINE_OUTPUT_LIMIT = 2000;

	const isApproval = $derived(entry.kind === 'approval');
	const isGateway = $derived(entry.kind === 'network');
	const isScript = $derived(entry.kind === 'bash' || entry.kind === 'script');
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

	// Kinds absent from the map (mcp, network, utility) fall back to muted.
	const KIND_TONE: Record<string, StatusTone> = {
		bash: 'success',
		script: 'success',
		file: 'info',
		search: 'warning',
		approval: 'running',
		memory: 'info',
		knowledge: 'info',
		agent: 'warning',
		interaction: 'running',
		workflow: 'info',
		risk: 'warning',
	};

	const icon = $derived(KIND_ICON[entry.kind] ?? 'blocks');
	const tone = $derived(toneText(KIND_TONE[entry.kind]));
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
		{#if headerHint}
			<span
				class="shrink-0 font-mono text-micro tabular-nums text-muted-foreground"
			>
				{headerHint}
			</span>
		{/if}
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
					{#if previews && !hiddenBody && preview.overflow}
						<span class="ml-1 font-mono normal-case tabular-nums">
							{#if preview.hiddenLines > 0}
								+{preview.hiddenLines} lines
							{:else}
								+more
							{/if}
						</span>
					{/if}
				</p>
				{#if !previews}
					<JsonViewer value={entry.output} />
				{:else if hiddenBody}
					<p class="font-mono text-micro text-muted-foreground">{countText}</p>
					<button
						type="button"
						onclick={() => (showFullOutput = !showFullOutput)}
						aria-expanded={showFullOutput}
						class="mb-1 flex items-center gap-1 text-micro text-muted-foreground transition-colors hover:text-foreground"
					>
						<Icon
							name="chevron-down"
							size={12}
							class={cn(
								'transition-transform duration-150',
								showFullOutput && 'rotate-180',
							)}
						/>
						{showFullOutput ? 'Hide output' : 'Show output'}
					</button>
					{#if showFullOutput}
						<pre
							data-scrollable="true"
							class={PREVIEW_PRE}>{fullBody.text}</pre>
						{#if fullBody.capped}
							<p class="mt-1 font-mono text-micro text-muted-foreground">
								… {fullBody.hiddenLines} more lines — open Full result for everything
							</p>
						{/if}
					{/if}
				{:else if preview.text}
					{#if displayKind === 'diff'}
						<pre
							data-scrollable="true"
							class={PREVIEW_PRE}>{#each previewLines as line, index (index)}<span
									class={diffToneClass(line)}>{line}</span
								>{#if index < previewLines.length - 1}<br />{/if}{/each}</pre>
					{:else}
						<pre data-scrollable="true" class={PREVIEW_PRE}>{preview.text}</pre>
					{/if}
					{#if expandable}
						<button
							type="button"
							onclick={() => (showFullOutput = !showFullOutput)}
							aria-expanded={showFullOutput}
							class="mt-1 flex items-center gap-1 text-micro text-muted-foreground transition-colors hover:text-foreground"
						>
							<Icon
								name="chevron-down"
								size={12}
								class={cn(
									'transition-transform duration-150',
									showFullOutput && 'rotate-180',
								)}
							/>
							{showFullOutput ? 'Hide full output' : 'Show full output'}
						</button>
					{/if}
					{#if showFullOutput}
						<div class="mt-1">
							<JsonViewer value={entry.output} />
						</div>
					{/if}
				{:else}
					<p class="font-mono text-micro text-muted-foreground">
						No content recorded
					</p>
				{/if}
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
