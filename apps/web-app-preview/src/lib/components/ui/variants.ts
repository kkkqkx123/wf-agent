import { cn } from '$lib/utils/cn';

export type Size = 'sm' | 'md' | 'lg';

const BASE_FOCUS =
	'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium transition-colors duration-150 disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[hsl(var(--ring))]';

const BUTTON_VARIANT = {
	default: 'bg-primary text-primary-foreground hover:bg-primary/90',
	secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
	outline:
		'border border-border bg-card text-foreground hover:bg-accent hover:text-accent-foreground',
	ghost: 'text-foreground hover:bg-accent hover:text-accent-foreground',
	destructive:
		'bg-destructive text-destructive-foreground hover:bg-destructive/90',
	link: 'text-info underline-offset-4 hover:underline',
} as const;

const BUTTON_SIZE = {
	sm: 'h-7 px-2.5 text-small',
	md: 'h-8.5 px-3 text-body',
	lg: 'h-10 px-4 text-title',
	icon: 'h-8 w-8',
	'icon-sm': 'h-7 w-7',
} as const;

export type ButtonVariant = keyof typeof BUTTON_VARIANT;
export type ButtonSize = keyof typeof BUTTON_SIZE;

export function buttonClass(
	variant: ButtonVariant = 'default',
	size: ButtonSize = 'md',
	extra?: string,
): string {
	return cn(BASE_FOCUS, BUTTON_VARIANT[variant], BUTTON_SIZE[size], extra);
}

const BADGE_VARIANT = {
	default: 'bg-primary text-primary-foreground',
	secondary: 'bg-secondary text-secondary-foreground',
	outline: 'border border-border text-foreground',
	success: 'bg-success/12 text-success border border-success/25',
	danger: 'bg-destructive/12 text-destructive border border-destructive/25',
	warning: 'bg-warning/12 text-warning border border-warning/25',
	info: 'bg-info/12 text-info border border-info/25',
	running: 'bg-running/12 text-running border border-running/25',
	neutral: 'bg-muted text-muted-foreground border border-border',
} as const;

export type BadgeVariant = keyof typeof BADGE_VARIANT;

export function badgeClass(
	variant: BadgeVariant = 'neutral',
	extra?: string,
): string {
	return cn(
		'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-micro font-medium leading-5',
		BADGE_VARIANT[variant],
		extra,
	);
}

export const INPUT_BASE =
	'w-full rounded-md border border-input bg-card px-2.5 text-body text-foreground placeholder:text-muted-foreground transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))] disabled:cursor-not-allowed disabled:opacity-60';

export const SURFACE_PANEL =
	'rounded-lg border border-border bg-card text-card-foreground shadow-sm';

export const MUTED_TEXT = 'text-muted-foreground';
