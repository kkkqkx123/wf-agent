<script lang="ts">
	/**
	 * Inline interaction prompt: the tool approvals and follow-up questions the
	 * running agent loop is blocked on. Shown in the transcript tail so the
	 * user answers in place instead of switching to the checkpoints queue.
	 *
	 * Semantics follow the CLI approval and question overlays (approve / reject
	 * with an optional reason; single-choice questions with free-form input and
	 * a cancel path); the shape is browser-native buttons and inputs.
	 */
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import type { BadgeVariant } from '#lib/ui/components/variants';
	import type {
		InteractionAnswer,
		PendingInteraction,
	} from '#lib/services/interactions';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const pending = $derived(controller.pendingInteractions);

	// Per-interaction local input, keyed by interaction id then question index.
	let answers = $state<Record<string, Record<number, string>>>({});
	let custom = $state<Record<string, Record<number, string>>>({});
	let reasons = $state<Record<string, string>>({});
	let submitting = $state<string | null>(null);

	function riskVariant(risk: string | null): BadgeVariant {
		switch (risk) {
			case 'high':
			case 'critical':
				return 'danger';
			case 'medium':
				return 'warning';
			case 'low':
				return 'info';
			default:
				return 'neutral';
		}
	}

	function prettyParameters(parameters: unknown): string {
		if (parameters === null || parameters === undefined) return '';
		if (typeof parameters === 'string') return parameters;
		try {
			return JSON.stringify(parameters, null, 2);
		} catch {
			return '';
		}
	}

	function selected(id: string, index: number): string {
		return answers[id]?.[index] ?? '';
	}

	function pick(id: string, index: number, value: string): void {
		answers = { ...answers, [id]: { ...(answers[id] ?? {}), [index]: value } };
	}

	function customText(id: string, index: number): string {
		return custom[id]?.[index] ?? '';
	}

	function setCustom(id: string, index: number, value: string): void {
		custom = { ...custom, [id]: { ...(custom[id] ?? {}), [index]: value } };
	}

	async function approve(interaction: PendingInteraction): Promise<void> {
		submitting = interaction.id;
		try {
			await controller.respondApproval(interaction, true);
		} finally {
			submitting = null;
		}
	}

	async function reject(interaction: PendingInteraction): Promise<void> {
		submitting = interaction.id;
		try {
			await controller.respondApproval(
				interaction,
				false,
				reasons[interaction.id]?.trim() || undefined,
			);
		} finally {
			submitting = null;
		}
	}

	async function submitQuestions(
		interaction: PendingInteraction,
	): Promise<void> {
		const payload: InteractionAnswer[] = interaction.questions.map(
			(question) => {
				const typed = customText(interaction.id, question.index).trim();
				const picked = selected(interaction.id, question.index);
				return {
					questionIndex: question.index,
					selectedValue: typed ? null : picked || null,
					customInput: typed ? typed : null,
				};
			},
		);
		submitting = interaction.id;
		try {
			await controller.respondQuestions(interaction, payload);
		} finally {
			submitting = null;
		}
	}

	async function cancel(interaction: PendingInteraction): Promise<void> {
		submitting = interaction.id;
		try {
			await controller.cancelInteraction(interaction);
		} finally {
			submitting = null;
		}
	}
</script>

{#each pending as interaction (interaction.id)}
	{#if interaction.approval}
		{@const approval = interaction.approval}
		<div
			role="group"
			aria-label={t('chat.interactionPrompt.approvalFor', {
				tool: approval.toolName,
			})}
			class="rounded-lg border border-warning/40 bg-warning/5 px-3 py-2.5"
		>
			<div class="flex items-center gap-2">
				<Icon name="shield" size={14} class="shrink-0 text-warning" />
				<span class="text-caption font-medium text-foreground">
					{t('chat.interactionPrompt.approvalRequired')}
				</span>
				{#if approval.riskLevel}
					<Badge variant={riskVariant(approval.riskLevel)}>
						{approval.riskLevel}
					</Badge>
				{/if}
				{#if approval.totalTools && approval.toolIndex !== null}
					<span class="ml-auto font-mono text-micro text-muted-foreground">
						{approval.toolIndex + 1} / {approval.totalTools}
					</span>
				{/if}
			</div>

			<p class="mt-1.5 font-mono text-caption text-foreground">
				{approval.toolName}
			</p>
			{#if approval.toolDescription}
				<p class="mt-0.5 text-caption text-muted-foreground">
					{approval.toolDescription}
				</p>
			{/if}

			{#if prettyParameters(approval.parameters)}
				<pre
					data-scrollable="true"
					class="mt-2 max-h-40 overflow-auto rounded-md border border-border bg-card px-2 py-1.5 font-mono text-micro text-foreground">{prettyParameters(
						approval.parameters,
					)}</pre>
			{/if}

			{#if approval.pendingQueue.length > 0}
				<p class="mt-1.5 text-micro text-muted-foreground">
					{t('chat.interactionPrompt.moreQueued', {
						count: approval.pendingQueue.length,
					})}
					{#each approval.pendingQueue as queued (queued.id || queued.name)}
						<span class="ml-1 font-mono">{queued.name}</span>
					{/each}
				</p>
			{/if}

			<input
				value={reasons[interaction.id] ?? ''}
				oninput={(event) =>
					(reasons = {
						...reasons,
						[interaction.id]: (event.currentTarget as HTMLInputElement).value,
					})}
				placeholder={t('chat.interactionPrompt.rejectionReasonPlaceholder')}
				aria-label={t('chat.interactionPrompt.rejectionReason')}
				class="mt-2 w-full rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>

			<div class="mt-2 flex items-center gap-2">
				<Button
					size="sm"
					disabled={submitting === interaction.id}
					onclick={() => void approve(interaction)}
				>
					<Icon name="check" size={13} />
					{t('chat.interactionPrompt.approve')}
				</Button>
				<Button
					size="sm"
					variant="outline"
					disabled={submitting === interaction.id}
					onclick={() => void reject(interaction)}
				>
					<Icon name="x" size={13} />
					{t('chat.interactionPrompt.reject')}
				</Button>
			</div>
		</div>
	{:else if interaction.questions.length > 0}
		<div
			role="group"
			aria-label={t('chat.interactionPrompt.questionGroup')}
			class="rounded-lg border border-info/40 bg-info/5 px-3 py-2.5"
		>
			<div class="flex items-center gap-2">
				<Icon name="info" size={14} class="shrink-0 text-info" />
				<span class="text-caption font-medium text-foreground">
					{t('chat.interactionPrompt.needsInfo')}
				</span>
			</div>

			{#each interaction.questions as question (question.index)}
				<p class="mt-2 text-caption text-foreground">{question.text}</p>
				{#if question.options.length > 0}
					<div class="mt-1 flex flex-col gap-1" role="radiogroup">
						{#each question.options as option, index (option.value)}
							<button
								type="button"
								role="radio"
								aria-checked={selected(interaction.id, question.index) ===
									option.value}
								onclick={() =>
									pick(interaction.id, question.index, option.value)}
								class="flex items-center gap-2 rounded-md border px-2 py-1 text-left text-caption transition-colors {selected(
									interaction.id,
									question.index,
								) === option.value
									? 'border-info bg-info/10 text-foreground'
									: 'border-border bg-card text-muted-foreground hover:bg-accent'}"
							>
								<span class="font-mono text-micro">{index + 1}</span>
								<span class="min-w-0 flex-1 truncate">{option.label}</span>
							</button>
						{/each}
					</div>
				{/if}
				<input
					value={customText(interaction.id, question.index)}
					oninput={(event) =>
						setCustom(
							interaction.id,
							question.index,
							(event.currentTarget as HTMLInputElement).value,
						)}
					placeholder={t('chat.interactionPrompt.customAnswerPlaceholder')}
					aria-label={t('chat.interactionPrompt.customAnswer')}
					class="mt-1 w-full rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
			{/each}

			<div class="mt-2 flex items-center gap-2">
				<Button
					size="sm"
					disabled={submitting === interaction.id}
					onclick={() => void submitQuestions(interaction)}
				>
					<Icon name="arrow-up" size={13} />
					{t('chat.interactionPrompt.submit')}
				</Button>
				<Button
					size="sm"
					variant="outline"
					disabled={submitting === interaction.id}
					onclick={() => void cancel(interaction)}
				>
					{t('common.cancel')}
				</Button>
			</div>
		</div>
	{/if}
{/each}
