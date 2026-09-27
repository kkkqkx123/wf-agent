<script lang="ts">
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import { ui } from '$lib/stores/ui.svelte';

	interface Shortcut {
		keys: string;
		action: string;
	}

	interface Link {
		href: string;
		label: string;
	}

	const shortcuts: Shortcut[] = [
		{ keys: 'Ctrl+K', action: 'Global search' },
		{ keys: 'F1', action: 'Open this help overlay' },
		{ keys: 'Esc', action: 'Close dialogs and overlays' },
	];

	const links: Link[] = [
		{ href: '/docs/user-guide.md', label: 'User guide' },
		{ href: 'https://github.com/atomgit-com/wf-agent#readme', label: 'README' },
	];
</script>

<Dialog
	bind:open={ui.helpOpen}
	title="Help"
	description="Keyboard shortcuts and documentation"
	width="30rem"
>
	<section>
		<p class="text-micro uppercase tracking-wide text-muted-foreground">
			Shortcuts
		</p>
		<ul class="mt-1 divide-y divide-border rounded-md border border-border">
			{#each shortcuts as shortcut (shortcut.keys)}
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
			Documentation
		</p>
		<ul class="mt-1 space-y-1">
			{#each links as link (link.href)}
				<li>
					<a
						href={link.href}
						target="_blank"
						rel="noreferrer"
						class="text-body text-info hover:underline"
					>
						{link.label}
					</a>
				</li>
			{/each}
		</ul>
	</section>
</Dialog>
