<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import ContextMenu, { type ContextMenuItem } from '#lib/ui/components/ContextMenu.svelte';
	import { ide } from '#lib/stores/ide.svelte';
	import { ptyStoreFor } from '#lib/stores/pty-session.svelte';
	import {
		createFsEntry,
		downloadFsBlob,
		readFsRaw,
		renameFsEntry,
	} from '#lib/services/fs';
	import {
		deletePaths,
		parentOf,
		pasteClipboard,
		uploadFiles,
	} from '#lib/ide/clipboard';
	import { t } from '#lib/i18n';
	import { buildIdeUrl } from '#lib/utils/ide-url';
	import { invalidateFsTreeIndex } from '#lib/utils/fs-tree-index';
	import { IDE_TREE_ROW_HEIGHT_PX } from '#lib/utils/file-tree-window';
	import type { GitBadgeLevel } from '#lib/utils/git-badges';

	interface Props {
		onFocusSearch?: () => void;
		onViewDiff?: (path: string) => void;
	}

	let { onFocusSearch, onViewDiff }: Props = $props();

	let filter = $state('');
	let expanded = new SvelteSet<string>(['']);
	let creating = $state<{ parent: string; isDir: boolean } | null>(null);
	let newName = $state('');
	let renaming = $state<string | null>(null);
	let renameValue = $state('');
	let actionError = $state<string | null>(null);
	let deleteOpen = $state(false);
	let pendingDeletePaths = $state<string[]>([]);
	let pendingDeleteIsDir = $state(false);
	let menu = $state<{ x: number; y: number; target: string; isDir: boolean } | null>(null);
	let dropTarget = $state<string | null>(null);
	let dragPaths = $state<string[]>([]);
	let uploadDir = $state('');
	let pendingOverwrite = $state<{ destDir: string; files: File[]; names: string[]; conflicts: string[] } | null>(null);
	let fileInput: HTMLInputElement | null = $state(null);

	const MAX_DIR_ENTRIES = 200;
	const treeRowHeightStyle = `--ide-tree-row-height: ${IDE_TREE_ROW_HEIGHT_PX}px`;

	const BADGE_DOT: Record<GitBadgeLevel, string> = {
		added: 'bg-emerald-500',
		modified: 'bg-amber-500',
		deleted: 'bg-red-500',
		renamed: 'bg-sky-500',
		conflicted: 'bg-red-600 ring-2 ring-red-600/40',
		untracked: 'bg-zinc-400',
	};

	function badgeLabel(level: GitBadgeLevel): string {
		switch (level) {
			case 'added': return t('page.ide.badgeAdded');
			case 'modified': return t('page.ide.badgeModified');
			case 'deleted': return t('page.ide.badgeDeleted');
			case 'renamed': return t('page.ide.badgeRenamed');
			case 'conflicted': return t('page.ide.badgeConflicted');
			case 'untracked': return t('page.ide.badgeUntracked');
		}
	}

	onMount(() => {
		void ide.loadDir('');
	});

	$effect(() => {
		const target = ide.revealRequest;
		if (!target) return;
		const parts = target.split('/').filter(Boolean);
		let prefix = '';
		for (let i = 0; i < parts.length - 1; i += 1) {
			prefix = prefix ? `${prefix}/${parts[i]}` : parts[i];
			if (!expanded.has(prefix)) {
				expanded.add(prefix);
				void ide.loadDir(prefix);
			}
		}
		ide.setSelection([target]);
		ide.anchorPath = target;
		ide.consumeReveal();
	});

	let handledCreateNonce = 0;
	$effect(() => {
		const request = ide.rootCreateRequest;
		if (!request || request.nonce === handledCreateNonce) return;
		handledCreateNonce = request.nonce;
		if (!expanded.has('')) expanded.add('');
		startCreate('', request.isDir);
	});

	let handledUploadNonce = 0;
	$effect(() => {
		const request = ide.uploadRequest;
		if (!request || request.nonce === handledUploadNonce) return;
		handledUploadNonce = request.nonce;
		openUploadPicker(request.dir);
	});

	function toggle(path: string): void {
		if (expanded.has(path)) expanded.delete(path);
		else {
			expanded.add(path);
			void ide.loadDir(path);
		}
	}

	function visibleEntries(path: string): { shown: typeof entries; overflow: number } {
		const state = ide.dirState(path);
		const entries = state.entries ?? [];
		const needle = filter.trim().toLowerCase();
		const matched = needle
			? entries.filter((e) => e.path.toLowerCase().includes(needle))
			: entries;
		if (matched.length <= MAX_DIR_ENTRIES) return { shown: matched, overflow: 0 };
		return { shown: matched.slice(0, MAX_DIR_ENTRIES), overflow: matched.length - MAX_DIR_ENTRIES };
	}

	function flattenVisible(): string[] {
		const out: string[] = [];
		const walk = (dir: string): void => {
			for (const entry of visibleEntries(dir).shown) {
				out.push(entry.path);
				if (entry.is_dir && expanded.has(entry.path)) walk(entry.path);
			}
		};
		walk('');
		return out;
	}

	function rowBadge(path: string, isDir: boolean): GitBadgeLevel | null {
		if (!ide.gitBadgesAvailable) return null;
		if (!isDir) return ide.gitBadges.get(path) ?? null;
		return ide.dirBadge(path);
	}

	function isCut(path: string): boolean {
		return ide.clipboard?.mode === 'cut' && (ide.clipboard.paths.includes(path) ?? false);
	}

	function effectiveTargets(target: string): string[] {
		if (ide.selectedPaths.includes(target) && ide.selectedPaths.length > 0) {
			return [...ide.selectedPaths];
		}
		return target ? [target] : [];
	}

	function rangeSelect(anchor: string, to: string): void {
		const flat = flattenVisible();
		const a = flat.indexOf(anchor);
		const b = flat.indexOf(to);
		if (a < 0 || b < 0) {
			const merged = [...ide.selectedPaths];
			for (const p of [anchor, to]) if (!merged.includes(p)) merged.push(p);
			ide.setSelection(merged);
			ide.anchorPath = to;
			return;
		}
		const [lo, hi] = a <= b ? [a, b] : [b, a];
		ide.setSelection(flat.slice(lo, hi + 1));
		ide.anchorPath = to;
	}

	async function openEntry(path: string, isDir: boolean, preview = true): Promise<void> {
		if (isDir) {
			toggle(path);
			return;
		}
		actionError = null;
		try {
			await ide.openFile(path, preview);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function handleRowClick(event: MouseEvent, path: string, isDir: boolean): void {
		if (event.ctrlKey || event.metaKey) {
			const next = new Set(ide.selectedPaths);
			if (next.has(path)) next.delete(path);
			else next.add(path);
			ide.setSelection([...next]);
			ide.anchorPath = path;
			return;
		}
		if (event.shiftKey) {
			rangeSelect(ide.anchorPath ?? path, path);
			return;
		}
		ide.setSelection([path]);
		ide.anchorPath = path;
		void openEntry(path, isDir, true);
	}

	function startCreate(parent: string, isDir: boolean): void {
		creating = { parent, isDir };
		newName = '';
		actionError = null;
	}

	async function commitCreate(): Promise<void> {
		const draft = creating;
		if (!draft || !newName.trim()) {
			creating = null;
			return;
		}
		const target = draft.parent ? `${draft.parent}/${newName.trim()}` : newName.trim();
		creating = null;
		try {
			await createFsEntry(target, draft.isDir);
			ide.invalidateParents(target);
			invalidateFsTreeIndex();
			await ide.loadDir(draft.parent, true);
			void ide.refreshGitStatus();
			if (!draft.isDir) await ide.openFile(target, false);
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function startRename(path: string): void {
		renaming = path;
		renameValue = path.split('/').pop() ?? path;
		actionError = null;
	}

	async function commitRename(path: string): Promise<void> {
		if (renaming !== path) return;
		renaming = null;
		const base = parentOf(path);
		const raw = renameValue.trim().replace(/^\.\/+/, '');
		if (!raw) return;
		const target = base ? `${base}/${raw}` : raw;
		if (target === path) return;
		try {
			await renameFsEntry(path, target);
			ide.invalidateParents(path);
			ide.invalidateParents(target);
			invalidateFsTreeIndex();
			if (ide.dirState(base).loaded) await ide.loadDir(base, true);
			const targetBase = parentOf(target);
			if (targetBase !== base && ide.dirState(targetBase).loaded) {
				await ide.loadDir(targetBase, true);
			}
			void ide.refreshGitStatus();
			ide.bumpGitRefresh();
			ide.retargetPath(path, target);
			ide.setSelection([target]);
			ide.anchorPath = target;
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	function requestRemove(paths: string[], isDir: boolean): void {
		pendingDeletePaths = [...paths];
		pendingDeleteIsDir = isDir && paths.length === 1;
		deleteOpen = true;
		actionError = null;
	}

	function deleteDescription(): string {
		if (pendingDeletePaths.length > 1) {
			return t('page.ide.confirmDeleteMany', { count: pendingDeletePaths.length });
		}
		const target = pendingDeletePaths[0];
		if (!target) return '';
		if (pendingDeleteIsDir) {
			const count = ide.dirState(target).entries?.length;
			if (typeof count === 'number') {
				return t('page.ide.confirmDeleteDir', { path: target, count });
			}
		}
		return t('page.ide.confirmDelete', { path: target });
	}

	function cancelRemove(): void {
		pendingDeletePaths = [];
		deleteOpen = false;
	}

	async function confirmRemove(): Promise<void> {
		const targets = pendingDeletePaths;
		pendingDeletePaths = [];
		deleteOpen = false;
		if (targets.length === 0) return;
		const { ok, failed } = await deletePaths(targets);
		if (failed.length > 0) {
			actionError = t('page.ide.deletePartial', { ok, failed: failed.length });
		} else {
			actionError = null;
		}
	}

	function entryIcon(isDir: boolean): 'folder' | 'file' {
		return isDir ? 'folder' : 'file';
	}

	function openMenu(event: MouseEvent, target: string, isDir: boolean): void {
		event.preventDefault();
		event.stopPropagation();
		menu = { x: event.clientX, y: event.clientY, target, isDir };
	}

	function openMenuKeyboard(event: KeyboardEvent, target: string, isDir: boolean): void {
		if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
			event.preventDefault();
			const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
			menu = { x: rect.left + 24, y: rect.top + rect.height, target, isDir };
		}
	}

	function menuItems(): ContextMenuItem[] {
		if (!menu) return [];
		const hasClipboard = (ide.clipboard?.paths.length ?? 0) > 0;
		if (!menu.target) {
			const items: ContextMenuItem[] = [
				{ id: 'new-file', label: t('page.ide.newFile') },
				{ id: 'new-folder', label: t('page.ide.newFolder') },
				{ id: 'upload', label: t('page.ide.uploadFiles') },
			];
			if (hasClipboard) items.push({ id: 'paste', label: t('page.ide.paste') });
			items.push({ id: 'refresh', label: t('common.refresh') });
			return items;
		}
		const targets = effectiveTargets(menu.target);
		const multi = targets.length > 1;
		const items: ContextMenuItem[] = [];
		if (!multi && !menu.isDir) {
			items.push({ id: 'open', label: t('page.ide.menuOpen') });
			items.push({ id: 'open-pinned', label: t('page.ide.menuOpenPinned') });
			items.push({ id: 'open-new-window', label: t('page.ide.openInNewWindow') });
		}
		if (!multi && menu.isDir) {
			items.push({ id: 'new-file', label: t('page.ide.newFile') });
			items.push({ id: 'new-folder', label: t('page.ide.newFolder') });
		}
		items.push({ id: 'cut', label: t('page.ide.cut') });
		items.push({ id: 'copy', label: t('page.ide.copy') });
		if (!multi && menu.isDir && hasClipboard) {
			items.push({ id: 'paste', label: t('page.ide.paste') });
		}
		if (!multi && !menu.isDir) {
			items.push({ id: 'download', label: t('page.ide.download') });
			items.push({ id: 'view-diff', label: t('page.ide.viewDiff') });
		}
		if (multi) {
			items.push({ id: 'download', label: t('page.ide.download') });
		}
		if (!multi && menu.isDir) {
			items.push({ id: 'upload', label: t('page.ide.uploadFiles') });
			items.push({ id: 'open-terminal', label: t('page.ide.menuOpenTerminal') });
		}
		items.push({ id: 'copy-path', label: t('page.ide.copyPath') });
		items.push({ id: 'copy-relative-path', label: t('page.ide.copyRelativePath') });
		if (!multi && !menu.isDir) {
			items.push({ id: 'terminal-file', label: t('page.ide.menuOpenTerminal') });
		}
		if (!multi) items.push({ id: 'rename', label: t('page.ide.rename') });
		items.push({
			id: 'delete',
			label: multi ? `${t('page.ide.delete')} (${targets.length})` : t('page.ide.delete'),
			danger: true,
		});
		return items;
	}

	function menuTitle(): string {
		if (!menu || !menu.target) return t('page.ide.fileTree');
		const targets = effectiveTargets(menu.target);
		if (targets.length > 1) return t('page.ide.selectedCount', { count: targets.length });
		return menu.target.split('/').pop() ?? menu.target;
	}

	async function copyText(value: string): Promise<void> {
		try {
			await navigator.clipboard.writeText(value);
		} catch {
			actionError = t('page.ide.copyFailed');
		}
	}

	async function openInTerminal(target: string, isDir: boolean): Promise<void> {
		const dir = isDir ? target : parentOf(target);
		const cwd = ide.root ? (dir ? `${ide.root}/${dir}` : ide.root) : undefined;
		try {
			const store = ptyStoreFor(ide.root || 'ide');
			ide.terminalOpen = true;
			await store.spawn({ cwd });
		} catch {
			actionError = t('page.ide.terminalFailed');
		}
	}

	async function doPaste(destDir: string): Promise<void> {
		actionError = null;
		const { ok, failed } = await pasteClipboard(destDir);
		if (failed.length > 0) {
			actionError = t('page.ide.pastePartial', { ok, failed: failed.length });
		}
	}

	async function downloadPaths(paths: string[]): Promise<void> {
		actionError = null;
		let started = 0;
		for (const path of paths) {
			try {
				const raw = await readFsRaw(path);
				downloadFsBlob(raw.filename, raw.mime, raw.content_base64);
				started += 1;
			} catch (e) {
				actionError = e instanceof Error ? e.message : String(e);
				return;
			}
		}
		if (started > 0 && paths.length === 1) {
			actionError = null;
		}
	}

	function openUploadPicker(dir: string): void {
		uploadDir = dir;
		pendingOverwrite = null;
		fileInput?.click();
	}

	async function doUploadFiles(destDir: string, files: File[], names?: string[], overwrite = false): Promise<void> {
		actionError = null;
		const outcome = await uploadFiles(destDir, files, { overwrite, names });
		if (!overwrite && outcome.conflicts.length > 0) {
			pendingOverwrite = { destDir, files, names: names ?? files.map((f) => f.name), conflicts: outcome.conflicts };
			if (outcome.uploaded.length > 0 || outcome.failed.length > 0) {
				actionError = t('page.ide.uploadPartial', {
					ok: outcome.uploaded.length,
					failed: outcome.failed.length,
					conflicts: outcome.conflicts.length,
				});
			}
			return;
		}
		if (outcome.failed.length > 0 || outcome.conflicts.length > 0) {
			actionError = t('page.ide.uploadPartial', {
				ok: outcome.uploaded.length,
				failed: outcome.failed.length,
				conflicts: outcome.conflicts.length,
			});
		}
	}

	function onPickerChange(): void {
		const files = fileInput ? Array.from(fileInput.files ?? []) : [];
		if (fileInput) fileInput.value = '';
		if (files.length === 0) return;
		void doUploadFiles(uploadDir, files);
	}

	async function confirmOverwriteAll(): Promise<void> {
		const pending = pendingOverwrite;
		pendingOverwrite = null;
		if (!pending) return;
		await doUploadFiles(pending.destDir, pending.files, pending.names, true);
	}

	function handleMenuAction(id: string): void {
		const current = menu;
		menu = null;
		if (!current) return;
		const target = current.target;
		if (!target) {
			if (id === 'new-file') startCreate('', false);
			else if (id === 'new-folder') startCreate('', true);
			else if (id === 'upload') openUploadPicker('');
			else if (id === 'paste') void doPaste('');
			else if (id === 'refresh') {
				void ide.loadDir('', true);
				void ide.refreshGitStatus();
			}
			return;
		}
		const targets = effectiveTargets(target);
		if (id === 'open') void openEntry(target, current.isDir, true);
		else if (id === 'open-pinned') void openEntry(target, current.isDir, false);
		else if (id === 'open-new-window') {
			if (ide.root) window.open(buildIdeUrl(target), '_blank', 'noopener');
		} else if (id === 'cut') {
			ide.setClipboard({ mode: 'cut', paths: targets });
		} else if (id === 'copy') {
			ide.setClipboard({ mode: 'copy', paths: targets });
		} else if (id === 'paste') {
			const dest = current.isDir ? target : parentOf(target);
			void doPaste(dest);
		} else if (id === 'download') void downloadPaths(targets);
		else if (id === 'view-diff') onViewDiff?.(target);
		else if (id === 'upload') openUploadPicker(current.isDir ? target : parentOf(target));
		else if (id === 'copy-path') void copyText(target);
		else if (id === 'copy-relative-path') void copyText(target);
		else if (id === 'new-file') {
			if (!expanded.has(target)) {
				expanded.add(target);
				void ide.loadDir(target);
			}
			startCreate(target, false);
		} else if (id === 'new-folder') {
			if (!expanded.has(target)) {
				expanded.add(target);
				void ide.loadDir(target);
			}
			startCreate(target, true);
		} else if (id === 'open-terminal' || id === 'terminal-file') void openInTerminal(target, current.isDir);
		else if (id === 'rename') startRename(target);
		else if (id === 'delete') requestRemove(targets, current.isDir);
	}

	function onRowDragStart(path: string, event: DragEvent): void {
		dragPaths = ide.selectedPaths.includes(path) && ide.selectedPaths.length > 0
			? [...ide.selectedPaths]
			: [path];
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			try {
				event.dataTransfer.setData('text/plain', dragPaths.join('\n'));
			} catch {
				// Drag payload is best-effort.
			}
		}
	}

	function onRowDragOver(path: string, isDir: boolean, event: DragEvent): void {
		const dest = isDir ? path : parentOf(path);
		if (dragPaths.length > 0) {
			if (dragPaths.includes(dest) || dragPaths.some((p) => dest.startsWith(`${p}/`))) return;
		}
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = dragPaths.length > 0 ? 'move' : 'copy';
		dropTarget = dest;
	}

	function onRowDragLeave(): void {
		dropTarget = null;
	}

	async function onRowDrop(path: string, isDir: boolean, event: DragEvent): Promise<void> {
		event.preventDefault();
		const dest = isDir ? path : parentOf(path);
		dropTarget = null;
		const external = event.dataTransfer ? Array.from(event.dataTransfer.files ?? []) : [];
		if (external.length > 0) {
			await doUploadFiles(dest, external);
			return;
		}
		if (dragPaths.length === 0) return;
		const moving = [...dragPaths];
		dragPaths = [];
		ide.setClipboard({ mode: 'cut', paths: moving });
		await doPaste(dest);
	}

	function onRootDragOver(event: DragEvent): void {
		if ((event.target as HTMLElement | null)?.closest?.('[role="treeitem"]')) return;
		if ((event.dataTransfer?.types ?? []).includes('Files')) {
			event.preventDefault();
			if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
			dropTarget = '';
		}
	}

	async function onRootDrop(event: DragEvent): Promise<void> {
		if ((event.currentTarget as HTMLElement | null)?.closest?.('[role="treeitem"]')) return;
		event.preventDefault();
		dropTarget = null;
		const external = event.dataTransfer ? Array.from(event.dataTransfer.files ?? []) : [];
		if (external.length > 0) await doUploadFiles('', external);
		dragPaths = [];
	}
</script>

{#snippet node(dirPath: string, level: number)}
	{#if creating && creating.parent === dirPath}
		<div class="flex items-center gap-1 px-1 py-0.5">
			<Icon name={creating.isDir ? 'folder' : 'file'} size={14} class="text-muted-foreground" />
			<input
				class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-body"
				bind:value={newName}
				placeholder={creating.isDir ? t('page.ide.folderName') : t('page.ide.fileName')}
				onkeydown={(e) => {
					if (e.key === 'Enter') void commitCreate();
					else if (e.key === 'Escape') creating = null;
				}}
			/>
		</div>
	{/if}
	{@const listed = visibleEntries(dirPath)}
	{#each listed.shown as entry (entry.path)}
		{@const isOpen = expanded.has(entry.path)}
		{@const dirty = entry.is_dir ? false : ide.isDirty(entry.path)}
		{@const badge = rowBadge(entry.path, entry.is_dir)}
		{@const selected = ide.isSelected(entry.path)}
		{@const cut = isCut(entry.path)}
		{@const isDrop = dropTarget === entry.path}
		<div
			role="treeitem"
			aria-selected={selected || ide.activePath === entry.path}
			aria-expanded={entry.is_dir ? isOpen : undefined}
			aria-level={level + 1}
			tabindex={0}
			draggable="true"
			ondragstart={(e) => onRowDragStart(entry.path, e)}
			ondragover={(e) => onRowDragOver(entry.path, entry.is_dir, e)}
			ondragleave={onRowDragLeave}
			ondrop={(e) => void onRowDrop(entry.path, entry.is_dir, e)}
			oncontextmenu={(e) => openMenu(e, entry.path, entry.is_dir)}
			onkeydown={(e) => {
				openMenuKeyboard(e, entry.path, entry.is_dir);
				if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement)) {
					void openEntry(entry.path, entry.is_dir, true);
				} else if (e.key === ' ' && !(e.target instanceof HTMLInputElement)) {
					e.preventDefault();
					const next = new Set(ide.selectedPaths);
					if (next.has(entry.path)) next.delete(entry.path);
					else next.add(entry.path);
					ide.setSelection([...next]);
					ide.anchorPath = entry.path;
				} else if (e.key === 'ArrowRight' && entry.is_dir && !isOpen) {
					toggle(entry.path);
				} else if (e.key === 'ArrowLeft' && entry.is_dir && isOpen) {
					toggle(entry.path);
				}
			}}
		>
			<div
				class="group flex w-full items-center gap-1 rounded-md px-1 py-1 text-body hover:bg-accent/60 {ide.activePath ===
				entry.path
					? 'bg-accent text-accent-foreground'
					: selected
						? 'bg-accent/40'
						: ''} {cut ? 'opacity-50' : ''} {isDrop ? 'ring-2 ring-primary' : ''}"
			>
				{#if entry.is_dir}
					<button
						type="button"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
						onclick={(e) => handleRowClick(e, entry.path, true)}
						ondblclick={() => toggle(entry.path)}
					>
						<Icon
							name={isOpen ? 'chevron-down' : 'chevron-right'}
							size={13}
							class="shrink-0 text-muted-foreground"
						/>
						<Icon name="folder" size={14} class="shrink-0 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate">{entry.name || '/'}</span>
						{#if badge}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full {BADGE_DOT[badge]}"
								title={badgeLabel(badge)}
							></span>
						{/if}
					</button>
				{:else}
					<button
						type="button"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left"
						onclick={(e) => handleRowClick(e, entry.path, false)}
						ondblclick={() => void openEntry(entry.path, false, false)}
					>
						<span class="w-4 shrink-0"></span>
						<Icon
							name={entryIcon(false)}
							size={14}
							class="shrink-0 text-muted-foreground"
						/>
						<span class="min-w-0 flex-1 truncate">{entry.name}</span>
						{#if dirty}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
								title={t('page.ide.unsaved')}
							></span>
						{/if}
						{#if badge}
							<span
								class="h-1.5 w-1.5 shrink-0 rounded-full {BADGE_DOT[badge]}"
								title={badgeLabel(badge)}
							></span>
						{/if}
					</button>
				{/if}
				<span class="hidden shrink-0 items-center group-hover:flex">
					{#if entry.is_dir}
						<IconButton
							icon="plus"
							label={t('page.ide.newFile')}
							compact
							variant="ghost"
							onclick={() => {
								if (!isOpen) toggle(entry.path);
								startCreate(entry.path, false);
							}}
						/>
					{/if}
					<IconButton
						icon="pencil"
						label={t('page.ide.rename')}
						compact
						variant="ghost"
						onclick={() => startRename(entry.path)}
					/>
					<IconButton
						icon="trash"
						label={t('page.ide.delete')}
						compact
						variant="ghost"
						onclick={() => requestRemove(effectiveTargets(entry.path), entry.is_dir)}
					/>
				</span>
			</div>

			{#if renaming === entry.path}
				<div class="flex items-center gap-1 py-0.5 pl-8 pr-1">
					<input
						class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-body"
						bind:value={renameValue}
						onkeydown={(e) => {
							if (e.key === 'Enter') void commitRename(entry.path);
							else if (e.key === 'Escape') renaming = null;
						}}
					/>
				</div>
			{/if}

			{#if entry.is_dir && isOpen}
				{@const childState = ide.dirState(entry.path)}
				{#if !childState.loaded && !childState.loading}
					{void ide.loadDir(entry.path)}
				{/if}
				<div class="ml-3 border-l border-border pl-1">
					{#if childState.loading && !childState.entries}
						<p class="px-2 py-1 text-micro text-muted-foreground">{t('common.loading')}</p>
					{:else if childState.error}
						<p class="px-2 py-1 text-micro text-destructive">{childState.error}</p>
					{:else}
						{@render node(entry.path, level + 1)}
					{/if}
				</div>
			{/if}
		</div>
	{/each}
	{#if listed.overflow > 0}
		<p class="px-2 py-1 text-micro text-muted-foreground">
			{t('page.ide.dirOverflow', { count: listed.overflow })}
		</p>
	{/if}
{/snippet}

<div class="flex h-full min-h-0 flex-col">
	<div class="flex items-center gap-1 border-b border-border px-2 py-1.5">
		<div class="min-w-0 flex-1">
			<Input
				bind:value={filter}
				placeholder={t('page.ide.filterFiles')}
				aria-label={t('page.ide.filterFiles')}
			/>
		</div>
		<IconButton
			icon="search"
			label={t('page.ide.sidebarSearch')}
			compact
			variant="ghost"
			onclick={() => onFocusSearch?.()}
		/>
		<IconButton
			icon="plus"
			label={t('page.ide.newFile')}
			compact
			variant="ghost"
			onclick={() => startCreate('', false)}
		/>
		<IconButton
			icon="folder"
			label={t('page.ide.newFolder')}
			compact
			variant="ghost"
			onclick={() => startCreate('', true)}
		/>
		<IconButton
			icon="refresh"
			label={t('common.refresh')}
			compact
			variant="ghost"
			onclick={() => {
				void ide.loadDir('', true);
				void ide.refreshGitStatus();
			}}
		/>
	</div>

	{#if ide.selectedPaths.length > 0 || ide.clipboard}
		<div class="flex items-center gap-1 border-b border-border px-2 py-1 text-micro text-muted-foreground">
			{#if ide.selectedPaths.length > 0}
				<span class="min-w-0 flex-1 truncate">
					{t('page.ide.selectedCount', { count: ide.selectedPaths.length })}
				</span>
				<button
					type="button"
					class="cursor-pointer rounded px-1 underline hover:no-underline"
					onclick={() => ide.clearSelection()}
				>
					{t('page.ide.clearSelection')}
				</button>
			{/if}
			{#if ide.clipboard}
				<span class="shrink-0 truncate" title={ide.clipboard.paths.join(', ')}>
					{ide.clipboard.mode === 'cut'
						? t('page.ide.clipboardCut', { count: ide.clipboard.paths.length })
						: t('page.ide.clipboardCopy', { count: ide.clipboard.paths.length })}
				</span>
			{/if}
		</div>
	{/if}

	{#if actionError}
		<p
			class="border-b border-destructive/30 bg-destructive/10 px-3 py-1 text-micro text-destructive"
		>
			{actionError}
		</p>
	{/if}

	<div
		class="min-h-0 flex-1 overflow-y-auto px-1 py-1"
		role="tree"
		tabindex={0}
		aria-label={t('page.ide.fileTree')}
		style={treeRowHeightStyle}
		ondragover={onRootDragOver}
		ondrop={(e) => void onRootDrop(e)}
		oncontextmenu={(e) => {
			if ((e.target as HTMLElement).closest('[role="treeitem"]')) return;
			openMenu(e, '', true);
		}}
	>
		{@render node('', 0)}
	</div>
</div>

<input type="file" multiple class="hidden" bind:this={fileInput} onchange={onPickerChange} />

{#if menu}
	<ContextMenu
		x={menu.x}
		y={menu.y}
		title={menuTitle()}
		items={menuItems()}
		onaction={handleMenuAction}
		onclose={() => (menu = null)}
	/>
{/if}

<Dialog
	bind:open={deleteOpen}
	title={t('page.ide.delete')}
	description={pendingDeletePaths.length > 0 ? deleteDescription() : undefined}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={cancelRemove}>
			{t('common.cancel')}
		</Button>
		<Button size="sm" variant="default" onclick={() => void confirmRemove()}>
			{t('common.delete')}
		</Button>
	</div>
</Dialog>

<Dialog
	open={pendingOverwrite !== null}
	title={t('page.ide.overwriteTitle')}
	description={pendingOverwrite ? t('page.ide.overwriteDesc', { names: pendingOverwrite.conflicts.slice(0, 3).join(', ') }) : undefined}
	onclose={() => (pendingOverwrite = null)}
>
	<div class="flex justify-end gap-2 p-4">
		<Button size="sm" variant="ghost" onclick={() => (pendingOverwrite = null)}>
			{t('page.ide.skipConflicts')}
		</Button>
		<Button size="sm" variant="default" onclick={() => void confirmOverwriteAll()}>
			{t('page.ide.overwriteAll')}
		</Button>
	</div>
</Dialog>
