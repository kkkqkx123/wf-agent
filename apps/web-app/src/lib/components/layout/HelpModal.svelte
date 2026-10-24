<script lang="ts">
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import { ui } from '#lib/stores/ui.svelte';
	import { CANVAS_SHORTCUT_HELP } from '#lib/graph/canvas-shortcuts';
	import { goto } from '$app/navigation';
	import { t, type MessageKey } from '#lib/i18n';

	interface Shortcut {
		keys: string;
		/** Translation key for the action the shortcut triggers. */
		actionKey?: MessageKey;
		/** Pre-rendered action label, used for entries defined outside i18n. */
		action?: string;
	}

	interface Link {
		href: string;
		labelKey: MessageKey;
	}

	// Actions are stored as keys so the labels follow the active locale.
	const globalShortcuts: Shortcut[] = [
		{ keys: 'Ctrl+K', actionKey: 'layout.help.globalSearch' },
		{ keys: 'F1 or ?', actionKey: 'layout.help.toggleHelp' },
		{ keys: 'Esc', actionKey: 'layout.help.closeDialogs' },
	];

	// Canvas entries live in the shared shortcut catalog, so their labels
	// stay as-is instead of being duplicated as layout.* keys.
	const canvasShortcuts: Shortcut[] = CANVAS_SHORTCUT_HELP.map((entry) => ({
		keys: entry.keys,
		action: entry.action,
	}));

	const links: Link[] = [
		{ href: `/docs/user-guide.md`, labelKey: 'layout.help.userGuide' },
		{
			href: 'https://github.com/atomgit-com/wf-agent#readme',
			labelKey: 'layout.help.readme',
		},
	];

	function navigateTo(href: string): void {
		if (href.startsWith('http://') || href.startsWith('https://')) {
			window.open(href, '_blank', 'noopener,noreferrer');
		} else {
			// The installed Kit version does not export resolve(); href values
			// here are already root-relative, so goto() is sufficient.

			goto(href);
		}
	}
</script>

<Dialog
	bind:open={ui.helpOpen}
	title={t('layout.help.title')}
	description={t('layout.help.description')}
	width="30rem"
>
	<section>
		<p class="text-micro uppercase tracking-wide text-muted-foreground">
			{t('layout.help.globalShortcuts')}
		</p>
		<ul class="mt-1 divide-y divide-border rounded-md border border-border">
			{#each globalShortcuts as shortcut (shortcut.keys)}
				<li class="flex items-center justify-between px-3 py-2">
					<span class="text-body"
						>{shortcut.action ?? t(shortcut.actionKey!)}</span
					>
					<kbd
						class="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-micro text-muted-foreground"
					>
						{shortcut.keys}
					</kbd>
				</li>
			{/each}
		</ul>
	</section>

	<section class="mt-4">
		<p class="text-micro uppercase tracking-wide text-muted-foreground">
			{t('layout.help.canvasShortcuts')}
		</p>
		<ul class="mt-1 divide-y divide-border rounded-md border border-border">
			{#each canvasShortcuts as shortcut (shortcut.keys)}
				<li class="flex items-center justify-between px-3 py-2">
					<span class="text-body">{shortcut.action}</span>
					<kbd
						class="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-micro text-muted-foreground"
					>
						{shortcut.keys}
					</kbd>
				</li>
			{/each}
		</ul>
	</section>

	<section class="mt-4">
		<p class="text-micro uppercase tracking-wide text-muted-foreground">
			{t('layout.help.documentation')}
		</p>
		<ul class="mt-1 space-y-1">
			{#each links as link (link.href)}
				<li>
					<button
						type="button"
						onclick={(e) => {
							e.preventDefault();
							navigateTo(link.href);
						}}
						class="text-body text-info hover:underline"
					>
						{t(link.labelKey)}
					</button>
				</li>
			{/each}
		</ul>
	</section>
</Dialog>
