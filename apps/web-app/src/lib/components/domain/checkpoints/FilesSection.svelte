<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import FilesPanel from '$lib/components/domain/checkpoints/FilesPanel.svelte';
	import { getFileContent, resolveConflicts, type ConflictFile } from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace } from '$lib/stores/fileWorkspace.svelte';
	import { hasConflictMarkers } from '$lib/types/diff';

	// ── dialogs ──
	let resolveOpen = $state(false);
	let resolveTarget = $state<ConflictFile | null>(null);
	let resolveFeature = $state('');
	let resolveContent = $state('');
	let resolveLoading = $state(false);
	let resolveBinary = $state(false);
	let resolveNote = $state<string | null>(null);

	async function runResolveConflict(): Promise<void> {
		const target = resolveTarget;
		if (!target) return;
		const actor = fileWorkspace.actor.trim();
		if (!actor) {
			toasts.error('Enter the resolving actor');
			return;
		}
		if (!resolveContent) {
			toasts.error('Resolve the file content first');
			return;
		}
		if (resolveBinary) {
			toasts.error('Binary conflicts cannot be resolved here', 'Use the terminal');
			return;
		}
		if (hasConflictMarkers(resolveContent)) {
			toasts.warning(
				'Conflict markers remain',
				'The resolution still carries <<<<<<</>>>>>>> markers and will stay conflicted.',
			);
		}
		checkpointStore.beginAction('resolve');
		try {
			const { remaining } = await resolveConflicts(actor, resolveFeature, {
				[target.path]: resolveContent,
			});
			resolveOpen = false;
			resolveTarget = null;
			resolveContent = '';
			toasts.success(
				remaining === 0 ? 'Conflict fully resolved' : 'Resolution recorded',
				remaining === 0 ? target.path : `${remaining} file(s) still conflicted`,
			);
			checkpointStore.bump('files');
		} catch (e) {
			console.error('Conflict resolution failed:', e);
			toasts.error(
				'Resolution failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	function openResolve(conflict: ConflictFile): void {
		resolveTarget = conflict;
		resolveOpen = true;
		// Feature refs look like refs/wf/feat/<name>; main conflicts resolve
		// against an explicit feature name.
		resolveFeature = conflict.partition.startsWith('refs/wf/feat/')
			? conflict.partition.slice('refs/wf/feat/'.length)
			: '';
		resolveContent = '';
		resolveBinary = false;
		resolveNote = null;
		resolveLoading = true;
		// The conflicted bytes (with markers) are read at the conflicted
		// merge commit without touching any workspace; the actor only
		// satisfies path validation.
		const actor = fileWorkspace.actor.trim() || 'manual';
		getFileContent(actor, conflict.path, conflict.snapshotId)
			.then((content) => {
				if (resolveTarget !== conflict) return;
				resolveBinary = content.isBinary;
				if (content.isBinary) {
					resolveNote =
						'Binary file: frontend resolution is unsupported, use the terminal.';
					return;
				}
				resolveContent = content.content ?? '';
				if (!resolveContent) {
					resolveNote = 'Conflicted content is empty at this commit.';
				} else if (content.truncated) {
					resolveNote = 'Content truncated at 1 MiB; verify the tail in the terminal.';
				}
			})
			.catch((e) => {
				if (resolveTarget !== conflict) return;
				console.error('Failed to load conflicted content:', e);
				resolveNote =
					e instanceof Error
						? `Conflicted content unavailable: ${e.message}`
						: 'Conflicted content unavailable.';
			})
			.finally(() => {
				if (resolveTarget === conflict) resolveLoading = false;
			});
	}

	const resolveHasMarkers = $derived(hasConflictMarkers(resolveContent));
</script>

<FilesPanel onresolve={(conflict: ConflictFile) => openResolve(conflict)} />

<Dialog
	bind:open={resolveOpen}
	title="Resolve conflict"
	description="Commit the resolved file content as a child of the conflicted head. Files still carrying markers stay conflicted."
	onclose={() => (resolveTarget = null)}
>
	{#if resolveTarget}
		<p class="mb-2 text-caption">
			<span class="font-mono">{resolveTarget.path}</span> on
			<span class="font-mono">{resolveTarget.partition}</span>
		</p>
	{/if}
	<div class="grid gap-2 sm:grid-cols-2">
		<Input
			label="Actor"
			value={fileWorkspace.actor}
			oninput={(event: Event) =>
				fileWorkspace.setActor((event.target as HTMLInputElement).value)}
			placeholder="Resolving actor"
		/>
		<Input
			label="Feature"
			bind:value={resolveFeature}
			placeholder="Feature name for refs/wf/feat/<name>"
		/>
	</div>
	<div class="mt-2">
		{#if resolveLoading}
			<p class="text-caption text-muted-foreground">
				Loading conflicted content…
			</p>
		{:else if resolveBinary}
			<p class="text-caption text-warning">
				Binary file: resolve it in the terminal, the frontend cannot merge
				binary content.
			</p>
		{:else}
			<Textarea
				label="Resolved content"
				bind:value={resolveContent}
				placeholder="Edit the conflicted content in place"
				rows={10}
			/>
		{/if}
		{#if resolveNote}
			<p class="mt-1 text-micro text-muted-foreground">{resolveNote}</p>
		{/if}
		{#if resolveHasMarkers && !resolveBinary}
			<p class="mt-1 text-micro text-warning">
				Conflict markers remain: saving now keeps the file conflicted.
			</p>
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resolveOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy ||
				resolveLoading ||
				resolveBinary ||
				!fileWorkspace.actor.trim() ||
				!resolveFeature.trim() ||
				!resolveContent}
			onclick={() => void runResolveConflict()}
		>
			Commit resolution
		</Button>
	{/snippet}
</Dialog>
