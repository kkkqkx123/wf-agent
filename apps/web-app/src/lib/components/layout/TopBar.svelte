<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import type { ResolvedPathname } from '$app/types';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import { navItemFor } from '#lib/config/navigation';
	import { preferences } from '#lib/stores/preferences.svelte';
	import { ui } from '#lib/stores/ui.svelte';
	import LanguageSwitcher from './LanguageSwitcher.svelte';
	import ThemeSwitcher from './ThemeSwitcher.svelte';
	import { buildListQuery } from '#lib/utils/route';
	import {
		onDirectoryChange,
		onWorkspaceLabelChange,
		resolveDirectory,
		resolveTabLabel,
	} from '#lib/api/workspace';
	import { t } from '#lib/i18n';

	const activeItem = $derived(navItemFor(page.url.pathname));

	// Active workspace badge: short project name with the full directory
	// as tooltip. Refreshes on directory and label switches without reload.
	let workspaceLabel = $state('');
	let workspaceDir = $state<string | undefined>(undefined);

	function refreshWorkspace(): void {
		workspaceDir = resolveDirectory();
		workspaceLabel = resolveTabLabel();
	}

	onMount(() => {
		refreshWorkspace();
		const offDirectory = onDirectoryChange(refreshWorkspace);
		const offLabel = onWorkspaceLabelChange(refreshWorkspace);
		return () => {
			offDirectory();
			offLabel();
		};
	});

	// Plain anchors drop the query string, so carry the workspace
	// parameters over explicitly. Modified clicks keep native behavior.
	function openWorkspaceSettings(event: MouseEvent): void {
		if (
			event.defaultPrevented ||
			event.button !== 0 ||
			event.metaKey ||
			event.ctrlKey ||
			event.shiftKey ||
			event.altKey
		) {
			return;
		}
		event.preventDefault();
		const query = buildListQuery(page.url as unknown as URL, { tab: 'workspace' });
		const target = `${resolve('/settings')}${query ? `?${query}` : ''}` as ResolvedPathname;
		void goto(target);
	}
</script>

<header class="frost flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
	<IconButton
		icon="menu"
		label={t('layout.topbar.toggleNavigation')}
		class="lg:hidden"
		onclick={() => ui.toggleMobileNav()}
	/>

	<nav
		aria-label={t('layout.topbar.breadcrumb')}
		class="flex min-w-0 items-center gap-1.5 text-body"
	>
		<a href={resolve('/')} class="truncate text-muted-foreground hover:text-foreground">wf-agent</a>
		{#if activeItem}
			<Icon name="chevron-right" size={13} class="shrink-0 text-muted-foreground/60" />
			<span class="truncate font-medium text-foreground">{activeItem.label()}</span>
		{/if}
	</nav>

	<div class="ml-auto flex items-center gap-1.5">
		<a
			href={resolve('/settings')}
			onclick={openWorkspaceSettings}
			title={workspaceDir ?? workspaceLabel}
			aria-label={t('layout.topbar.workspace')}
			class="hidden max-w-44 items-center gap-1.5 truncate rounded-md border border-border bg-muted/40 px-2 py-1 text-caption text-muted-foreground transition-colors hover:text-foreground sm:inline-flex"
		>
			<Icon name="folder" size={13} class="shrink-0" />
			<span class="truncate">{workspaceLabel || t('layout.topbar.noWorkspace')}</span>
		</a>
		<Button
			variant="outline"
			size="sm"
			class="hidden gap-2 md:inline-flex"
			onclick={() => ui.setCommandOpen(true)}
		>
			<Icon name="search" size={13} />
			<span>{t('common.search')}</span>
			<kbd
				class="ml-1 rounded border border-border bg-muted px-1 font-mono text-micro text-muted-foreground"
			>
				⌘K
			</kbd>
		</Button>

		<IconButton
			icon="command"
			label={t('layout.topbar.commandPalette')}
			onclick={() => ui.toggleCommand()}
		/>
		<IconButton
			icon="panel-right"
			label={t('layout.topbar.toggleInspector')}
			active={preferences.inspectorPinned}
			onclick={() => preferences.toggleInspectorPinned()}
		/>
		<ThemeSwitcher />
		<LanguageSwitcher />
	</div>
</header>
