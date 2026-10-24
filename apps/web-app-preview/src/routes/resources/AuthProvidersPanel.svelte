<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { t } from '#lib/i18n';
	import type { AuthProviderStatus } from '#lib/services/auth';

	interface Props {
		providers: AuthProviderStatus[];
		busyId?: string | null;
		onconnect?: (provider: AuthProviderStatus) => void;
		onlogout?: (provider: AuthProviderStatus) => void;
	}

	let { providers, busyId = null, onconnect, onlogout }: Props = $props();

	function methodLabel(method: string): string {
		return method === 'oauth' ? 'OAuth' : 'API key';
	}
</script>

<Card
	title={t('page.resources.auth.title')}
	description={t('page.resources.auth.description')}
>
	{#if providers.length === 0}
		<EmptyState
			icon="key"
			title={t('page.resources.auth.empty')}
			description={t('page.resources.auth.emptyDesc')}
			class="py-6"
		/>
	{:else}
		<ul class="divide-y divide-border">
			{#each providers as provider (provider.id)}
				<li
					class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0 last:pb-0"
				>
					<div class="min-w-0">
						<p class="flex flex-wrap items-center gap-1.5 truncate text-body">
							<span class="truncate font-medium"
								>{provider.display_name || provider.id}</span
							>
							<Badge
								variant={provider.has_credential ? 'success' : 'secondary'}
							>
								{provider.has_credential
									? t('page.resources.auth.stored')
									: t('page.resources.auth.notLoggedIn')}
							</Badge>
							{#if provider.is_subscription}
								<Badge variant="info"
									>{t('page.resources.auth.subscription')}</Badge
								>
							{/if}
						</p>
						<p
							class="mt-0.5 truncate font-mono text-micro text-muted-foreground"
						>
							{provider.id} · {methodLabel(provider.method)}
							{#if provider.login_label}
								<span class="ml-1">· {provider.login_label}</span>
							{/if}
						</p>
					</div>
					<div class="flex shrink-0 items-center gap-2">
						<StatusBadge
							status={provider.has_credential ? 'active' : 'disabled'}
							size="sm"
						/>
						{#if provider.has_credential}
							<Button
								variant="outline"
								size="sm"
								disabled={busyId === provider.id}
								onclick={() => onlogout?.(provider)}
							>
								{t('page.resources.auth.logout')}
							</Button>
						{:else}
							<Button
								size="sm"
								disabled={busyId === provider.id}
								onclick={() => onconnect?.(provider)}
							>
								{busyId === provider.id
									? t('page.resources.auth.connecting')
									: t('page.resources.auth.connect')}
							</Button>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</Card>
