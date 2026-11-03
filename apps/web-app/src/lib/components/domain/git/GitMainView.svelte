<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import TuiPanel from '#lib/ui/components/TuiPanel.svelte';
	import DiffView from '#lib/ui/components/DiffView.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import { git as gitStore } from '#lib/stores/git.svelte';
	import type { GitWorkbench, MainViewMode } from '#lib/git/workbench.svelte';
	import { selectableIndices } from '#lib/git/workbench.svelte';
	import type { DiffFile, DiffLine } from '#lib/types/diff';
	import { t } from '#lib/i18n';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let modeItems = $derived.by(() => {
		if (workbench.panel === 'files' || workbench.panel === 'conflicts') {
			return [
				{ id: 'diff', label: t('domain.git.viewDiff') },
				{ id: 'staging', label: t('domain.git.viewStaging') },
			];
		}
		if (workbench.panel === 'commits') {
			return [
				{ id: 'diff', label: t('domain.git.viewDiff') },
				{ id: 'commitFiles', label: t('domain.git.viewCommitFiles') },
			];
		}
		return [{ id: 'diff', label: t('domain.git.viewDiff') }];
	});

	let selectedPath = $derived(gitStore.selectedFile);
	let selectedCommit = $derived(gitStore.selectedCommit);

	function hunkCount(file: DiffFile): number {
		return file.lines.filter((l) => l.type === 'meta').length;
	}

	function splitHunks(file: DiffFile): { header: string; rows: DiffLine[] }[] {
		const out: { header: string; rows: DiffLine[] }[] = [];
		let current: { header: string; rows: DiffLine[] } | null = null;
		for (const line of file.lines) {
			if (line.type === 'meta') {
				if (current) out.push(current);
				current = { header: line.text, rows: [] };
			} else if (current) {
				current.rows.push(line);
			}
		}
		if (current) out.push(current);
		return out;
	}

	function isConflictMarker(text: string): boolean {
		return (
			text.startsWith('<<<<<<<') || text.startsWith('=======') || text.startsWith('>>>>>>>')
		);
	}

	let lastAnchor = $state<{ path: string; hunk: number; index: number } | null>(null);

	function onLineClick(
		path: string,
		hunkIndex: number,
		lineIndex: number,
		selectable: number[],
		event: MouseEvent,
	): void {
		if (event.shiftKey && lastAnchor && lastAnchor.path === path && lastAnchor.hunk === hunkIndex) {
			workbench.selectRange(path, hunkIndex, lastAnchor.index, lineIndex, selectable);
		} else {
			workbench.toggleLine(path, hunkIndex, lineIndex);
			lastAnchor = { path, hunk: hunkIndex, index: lineIndex };
		}
	}
</script>

<TuiPanel
	title="Main"
	description={workbench.selection
		? t('domain.git.selected', { selection: workbench.selection })
		: t('domain.git.mainViewDescription')}
	focused={workbench.panel === 'commits' || workbench.panel === 'files'}
>
	{#if modeItems.length > 1}
		<div class="mb-2">
			<Segmented
				items={modeItems}
				value={workbench.viewMode}
				onchange={(id) => workbench.setViewMode(id as MainViewMode)}
			/>
		</div>
	{/if}

	{#if workbench.isSelectedConflicted && selectedPath}
		{@const conflictPath = selectedPath}
		<div class="mb-2 rounded-md border border-destructive/40 bg-destructive/5 px-2 py-1.5">
			<p class="font-mono text-caption font-semibold text-destructive">{conflictPath}</p>
			<p class="mt-0.5 text-caption text-muted-foreground">
				{t('domain.git.conflictDescription')}
			</p>
			{#if workbench.meta?.mergeHead || workbench.meta?.rebaseHead || workbench.meta?.cherryPickHead}
				<p class="mt-0.5 font-mono text-micro text-warning">
					{t('page.git.midOperation')} · {t('domain.git.conflictPending')}
				</p>
			{:else}
				<p class="mt-0.5 font-mono text-micro text-muted-foreground">
					{t('domain.git.conflictPending')}
				</p>
			{/if}
			{#each workbench.diff as file (file.path)}
				{#if file.path === conflictPath}
					{#each file.lines as line (line.text + line.type)}
						{#if isConflictMarker(line.text)}
							<div
								class="mt-0.5 bg-warning/15 px-1 font-mono text-micro text-warning"
							>
								{line.text}
							</div>
						{/if}
					{/each}
				{/if}
			{/each}
			<div class="mt-1.5 flex flex-wrap gap-1">
				<Button
					size="sm"
					variant="outline"
					disabled={gitStore.busy}
					onclick={() => void workbench.acceptOurs(conflictPath)}
				>
					{t('domain.git.acceptOurs')}
				</Button>
				<Button
					size="sm"
					variant="outline"
					disabled={gitStore.busy}
					onclick={() => void workbench.acceptTheirs(conflictPath)}
				>
					{t('domain.git.acceptTheirs')}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					disabled={gitStore.busy}
					onclick={() => void workbench.toggleTerminal()}
				>
					{t('domain.git.openTerminal')}
				</Button>
			</div>
		</div>
	{:else if workbench.viewMode === 'staging'}
		{#if workbench.isMidOperation}
			<p class="mb-1 font-mono text-micro text-warning">
				{t('domain.git.midOpStagingReadonly')}
			</p>
		{/if}
		{#if !selectedPath}
			<p class="text-caption text-muted-foreground">{t('domain.git.noSelectionStaging')}</p>
		{:else}
			{@const stagePath = selectedPath}
			<div class="mb-2 flex flex-wrap gap-1">
				<Button
					size="sm"
					variant="outline"
					disabled={gitStore.busy || workbench.selectionSize === 0}
					onclick={() => void workbench.stageSelected(stagePath)}
				>
					{t('domain.git.stageSelected')}
					{#if workbench.selectionSize > 0}({workbench.selectionSize}){/if}
				</Button>
				<Button
					size="sm"
					variant="outline"
					disabled={gitStore.busy || workbench.selectionSize === 0}
					onclick={() => void workbench.unstageSelected(stagePath)}
				>
					{t('domain.git.unstageSelected')}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					disabled={gitStore.busy}
					onclick={() => workbench.selectAllForFile(stagePath)}
				>
					{t('domain.git.selectAllLines')}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					disabled={gitStore.busy || workbench.selectionSize === 0}
					onclick={() => workbench.clearSelection()}
				>
					{t('domain.git.clearSelection')}
				</Button>
			</div>
			<p class="mb-1 text-micro font-semibold text-muted-foreground">
				{t('domain.git.unstaged')} · {stagePath}
			</p>
			{#if workbench.diff.length === 0}
				<p class="mb-2 text-caption text-muted-foreground">{t('domain.git.pickFile')}</p>
			{:else}
				{#each workbench.diff as file (file.path)}
					{#if file.path === stagePath}
						{#if file.binary}
							<p class="mb-2 text-caption text-muted-foreground">
								{t('domain.git.binaryOnlyFile')}
							</p>
						{:else if file.lines.length === 0}
							<p class="mb-2 text-caption text-muted-foreground">
								{t('domain.git.pickFile')}
							</p>
						{:else}
							{@const hunks = splitHunks(file)}
							{#each hunks as hunk, hunkIndex (hunkIndex)}
								{@const selectable = selectableIndices(hunk.rows)}
								{@const selected = workbench.selectedForHunk(file.path, hunkIndex)}
								<div class="mb-2 rounded-md border border-border">
									<div class="flex flex-wrap items-center gap-1 border-b border-border px-1.5 py-1">
										<input
											type="checkbox"
											aria-label={t('domain.git.hunkLabel', { number: hunkIndex + 1 })}
											checked={selectable.length > 0 &&
												selectable.every((v) => selected.includes(v))}
											disabled={gitStore.busy ||
												file.truncated ||
												selectable.length === 0}
											onchange={() =>
												workbench.toggleHunk(file.path, hunkIndex, selectable)}
											class="h-3.5 w-3.5 accent-primary"
										/>
										<span class="font-mono text-micro text-muted-foreground"
											>{hunk.header}</span
										>
										<span class="ml-auto flex gap-1">
											<Button
												size="sm"
												variant="ghost"
												disabled={gitStore.busy}
												onclick={() => void workbench.stageHunk(file.path, hunkIndex)}
											>
												{t('domain.git.stageHunk', { number: hunkIndex + 1 })}
											</Button>
											<Button
												size="sm"
												variant="ghost"
												disabled={gitStore.busy}
												onclick={() => void workbench.discardHunk(file.path, hunkIndex)}
											>
												{t('domain.git.discardHunk')}
											</Button>
										</span>
									</div>
									{#if file.truncated}
										<p class="px-2 py-1 font-mono text-micro text-warning">
											{t('domain.git.truncatedNoLines')}
										</p>
									{/if}
									<ul>
										{#each hunk.rows as row, rowIndex (rowIndex)}
											{@const globalIndex = rowIndex}
											{@const isSelectable =
												row.type === 'add' || row.type === 'del'}
											<li
												class="flex items-start gap-1.5 px-1.5 py-px font-mono text-micro {row.type ===
												'add'
													? 'bg-success/10 text-success'
													: row.type === 'del'
														? 'bg-destructive/10 text-destructive'
														: 'text-foreground'}"
											>
												{#if isSelectable}
													<input
														type="checkbox"
														checked={workbench.isLineSelected(
															file.path,
															hunkIndex,
															globalIndex,
														)}
														disabled={gitStore.busy || file.truncated}
														onclick={(e) =>
															onLineClick(
																file.path,
																hunkIndex,
																globalIndex,
																selectable,
																e,
															)}
														class="mt-0.5 h-3.5 w-3.5 shrink-0 accent-primary"
													/>
												{:else}
													<span class="w-3.5 shrink-0"></span>
												{/if}
												<span class="w-2 shrink-0 opacity-70"
													>{row.type === 'add'
														? '+'
														: row.type === 'del'
															? '-'
															: ' '}</span
												>
												<span class="min-w-0 flex-1 whitespace-pre-wrap break-all"
													>{row.text}</span
												>
											</li>
										{/each}
									</ul>
									{#if selected.length > 0}
										<div class="flex gap-1 border-t border-border px-1.5 py-1">
											<Button
												size="sm"
												variant="outline"
												disabled={gitStore.busy}
												onclick={() =>
													void workbench.stageLines(file.path, hunkIndex, selected)}
											>
												{t('domain.git.stageLines')} ({selected.length})
											</Button>
											<Button
												size="sm"
												variant="ghost"
												disabled={gitStore.busy}
												onclick={() =>
													void workbench.discardLines(file.path, hunkIndex, selected)}
											>
												{t('domain.git.discardLines')}
											</Button>
										</div>
									{/if}
								</div>
							{/each}
						{/if}
					{/if}
				{/each}
			{/if}
			<p class="mb-1 mt-2 text-micro font-semibold text-muted-foreground">
				{t('domain.git.stagedDiff')} · {stagePath}
			</p>
			{#if workbench.stagedDiff.length === 0}
				<p class="mb-2 text-caption text-muted-foreground">{t('domain.git.pickFile')}</p>
			{:else}
				{#each workbench.stagedDiff as file (file.path)}
					{#if file.path === stagePath}
						{#if file.binary}
							<p class="mb-2 text-caption text-muted-foreground">
								{t('domain.git.binaryOnlyFile')}
							</p>
						{:else if file.lines.length === 0}
							<p class="mb-2 text-caption text-muted-foreground">
								{t('domain.git.pickFile')}
							</p>
						{:else}
							{@const hunks = splitHunks(file)}
							{#each hunks as hunk, hunkIndex (hunkIndex)}
								{@const selectable = selectableIndices(hunk.rows)}
								{@const selected = workbench.selectedForHunk(file.path, hunkIndex)}
								<div class="mb-2 rounded-md border border-border">
									<div class="flex flex-wrap items-center gap-1 border-b border-border px-1.5 py-1">
										<input
											type="checkbox"
											aria-label={t('domain.git.hunkLabel', { number: hunkIndex + 1 })}
											checked={selectable.length > 0 &&
												selectable.every((v) => selected.includes(v))}
											disabled={gitStore.busy ||
												file.truncated ||
												selectable.length === 0}
											onchange={() =>
												workbench.toggleHunk(file.path, hunkIndex, selectable)}
											class="h-3.5 w-3.5 accent-primary"
										/>
										<span class="font-mono text-micro text-muted-foreground"
											>{hunk.header}</span
										>
										<span class="ml-auto flex gap-1">
											<Button
												size="sm"
												variant="ghost"
												disabled={gitStore.busy}
												onclick={() => void workbench.unstageHunk(file.path, hunkIndex)}
											>
												{t('domain.git.unstageHunk')}
											</Button>
										</span>
									</div>
									{#if file.truncated}
										<p class="px-2 py-1 font-mono text-micro text-warning">
											{t('domain.git.truncatedNoLines')}
										</p>
									{/if}
									<ul>
										{#each hunk.rows as row, rowIndex (rowIndex)}
											{@const globalIndex = rowIndex}
											{@const isSelectable =
												row.type === 'add' || row.type === 'del'}
											<li
												class="flex items-start gap-1.5 px-1.5 py-px font-mono text-micro {row.type ===
												'add'
													? 'bg-success/10 text-success'
													: row.type === 'del'
														? 'bg-destructive/10 text-destructive'
														: 'text-foreground'}"
											>
												{#if isSelectable}
													<input
														type="checkbox"
														checked={workbench.isLineSelected(
															file.path,
															hunkIndex,
															globalIndex,
														)}
														disabled={gitStore.busy || file.truncated}
														onclick={(e) =>
															onLineClick(
																file.path,
																hunkIndex,
																globalIndex,
																selectable,
																e,
															)}
														class="mt-0.5 h-3.5 w-3.5 shrink-0 accent-primary"
													/>
												{:else}
													<span class="w-3.5 shrink-0"></span>
												{/if}
												<span class="w-2 shrink-0 opacity-70"
													>{row.type === 'add'
														? '+'
														: row.type === 'del'
															? '-'
															: ' '}</span
												>
												<span class="min-w-0 flex-1 whitespace-pre-wrap break-all"
													>{row.text}</span
												>
											</li>
										{/each}
									</ul>
									{#if selected.length > 0}
										<div class="flex gap-1 border-t border-border px-1.5 py-1">
											<Button
												size="sm"
												variant="outline"
												disabled={gitStore.busy}
												onclick={() =>
													void workbench.unstageLines(file.path, hunkIndex, selected)}
											>
												{t('domain.git.unstageLines')} ({selected.length})
											</Button>
										</div>
									{/if}
								</div>
							{/each}
						{/if}
					{/if}
				{/each}
			{/if}
		{/if}
	{:else if workbench.viewMode === 'commitFiles'}
		{#if !selectedCommit}
			<p class="text-caption text-muted-foreground">{t('domain.git.pickCommit')}</p>
		{:else}
			<p class="mb-1 text-micro font-semibold text-muted-foreground">
				{t('domain.git.commitFilesTitle')}
			</p>
			{#if workbench.commitFiles.length === 0}
				<p class="mb-2 text-caption text-muted-foreground">
					{t('domain.git.pickCommitFile')}
				</p>
			{:else}
				<ul class="mb-2 divide-y divide-border">
					{#each workbench.commitFiles as file (file.path)}
						<li>
							<button
								type="button"
								onclick={() =>
									selectedCommit &&
									void workbench.showCommitFileDiff(selectedCommit, file.path)}
								class="flex w-full items-center justify-between gap-2 px-1 py-1 text-left hover:bg-accent/50"
							>
								<span class="truncate font-mono text-caption">{file.path}</span>
								<span class="font-mono text-micro text-muted-foreground"
									>{file.status}</span
								>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
			{#if workbench.diff.length === 0}
				<p class="text-caption text-muted-foreground">{t('domain.git.pickCommitFile')}</p>
			{:else}
				{#each workbench.diff as file (file.path)}
					<div class="mb-2">
						<DiffView
							lines={file.lines}
							title={file.truncated
								? `${file.path} (+${file.additions}/-${file.deletions}) · truncated`
								: `${file.path} (+${file.additions}/-${file.deletions})`}
						/>
					</div>
				{/each}
			{/if}
		{/if}
	{:else if workbench.panel === 'commits'}
		{#if !selectedCommit}
			<p class="text-caption text-muted-foreground">{t('domain.git.pickCommit')}</p>
		{:else if workbench.diff.length === 0}
			<p class="text-caption text-muted-foreground">{t('domain.git.pickCommitFile')}</p>
		{:else}
			<p class="mb-1 text-micro text-muted-foreground">{t('domain.git.commitDiffAll')}</p>
			{#each workbench.diff as file (file.path)}
				<div class="mb-2">
					<DiffView
						lines={file.lines}
						title={file.truncated
							? `${file.path} (+${file.additions}/-${file.deletions}) · truncated`
							: `${file.path} (+${file.additions}/-${file.deletions})`}
					/>
				</div>
			{/each}
		{/if}
	{:else if workbench.panel === 'conflicts' && workbench.visibleConflicts.length === 0}
		<p class="text-caption text-muted-foreground">{t('domain.git.conflictEmpty')}</p>
	{:else if workbench.diff.length === 0}
		{@const isUntracked =
			selectedPath && workbench.files.find((f) => f.path === selectedPath)?.kind === 'untracked'}
		{#if isUntracked}
			<p class="text-caption text-muted-foreground">
				{t('domain.git.untrackedOnlyFile')}
			</p>
		{:else}
			<p class="text-caption text-muted-foreground">{t('domain.git.pickFile')}</p>
		{/if}
	{:else}
		<p class="mb-1 text-micro text-muted-foreground">{t('domain.git.unstagedDiff')}</p>
		{#each workbench.diff as file (file.path)}
			<div class="mb-2">
				{#if file.binary}
					<p class="mb-1 text-caption text-muted-foreground">
						{t('domain.git.binaryOnlyFile')}
					</p>
				{:else}
					{@const count = hunkCount(file)}
					{#if count > 0}
						<div class="mb-1 flex flex-wrap gap-1">
							{#each Array.from({ length: count }, (_, i) => i) as i (i)}
								<Button
									size="sm"
									variant="outline"
									disabled={gitStore.busy}
									onclick={() => void workbench.stageHunk(file.path, i)}
								>
									{t('domain.git.stageHunk', { number: i + 1 })}
								</Button>
								<Button
									size="sm"
									variant="ghost"
									disabled={gitStore.busy}
									onclick={() => void workbench.discardHunk(file.path, i)}
								>
									{t('domain.git.discardHunk')}
								</Button>
							{/each}
						</div>
					{/if}
				{/if}
				<DiffView
					lines={file.lines}
					title={file.truncated
						? `${file.path} (+${file.additions}/-${file.deletions}) · truncated`
						: `${file.path} (+${file.additions}/-${file.deletions})`}
				/>
			</div>
		{/each}
	{/if}
	{#snippet footer()}
		{#if workbench.controls.length > 0}
			<div class="flex gap-x-3 gap-y-1 overflow-x-auto md:flex-wrap md:overflow-visible">
				{#each workbench.controls as control (control.id)}
					{@const spec = workbench.specFor(control.id)}
					<Button
						size="sm"
						variant="ghost"
						disabled={gitStore.busy || (spec ? !spec.available : false)}
						title={spec?.reason ??
							(control.legacyKey
								? `${control.name} (${control.hint} · ${control.legacyKey})`
								: `${control.name} (${control.hint})`)}
						onclick={() => void workbench.runActionId(control.id)}
					>
						<span class="font-mono"
							>[{control.legacyKey
								? `${control.hint} · ${control.legacyKey}`
								: control.hint}]</span
						>
						{control.name}
					</Button>
				{/each}
			</div>
		{/if}
	{/snippet}
</TuiPanel>
