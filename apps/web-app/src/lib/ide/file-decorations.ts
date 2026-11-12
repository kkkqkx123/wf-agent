import type { GitBadgeLevel } from '#lib/utils/git-badges';

export interface ProblemSummary {
	errors: number;
	warnings: number;
}

export interface DecorationRequest {
	git: GitBadgeLevel | null;
	errors: number;
	warnings: number;
}

export interface FileDecoration {
	git: GitBadgeLevel | null;
	letter: string | null;
	letterClass: string;
	textClass: string;
	errors: number;
	warnings: number;
	hasErrors: boolean;
	hasWarnings: boolean;
}

const GIT_LETTER: Record<GitBadgeLevel, string> = {
	added: 'A',
	modified: 'M',
	deleted: 'D',
	renamed: 'R',
	conflicted: 'C',
	untracked: 'U',
};

/** Single-letter badge for a git level. Letters stay untranslated. */
export function letterForGitLevel(level: GitBadgeLevel): string {
	return GIT_LETTER[level];
}

function normalizeCount(value: number): number {
	if (!Number.isFinite(value)) return 0;
	return Math.max(0, Math.floor(value));
}

/** Badge letter color. Problems outrank version status. */
export function letterClassFor(request: DecorationRequest): string {
	if (normalizeCount(request.errors) > 0) return 'text-destructive';
	switch (request.git) {
		case 'conflicted':
		case 'deleted':
			return 'text-destructive';
		case 'added':
			return 'text-success';
		case 'renamed':
			return 'text-info';
		case 'modified':
			return 'text-warning';
		case 'untracked':
			return 'text-muted-foreground';
		default:
			break;
	}
	if (normalizeCount(request.warnings) > 0) return 'text-warning';
	return 'text-muted-foreground';
}

/** Filename color. Clean files keep the inherited tone. */
export function textClassFor(request: DecorationRequest): string {
	if (normalizeCount(request.errors) > 0) return 'text-destructive';
	switch (request.git) {
		case 'conflicted':
			return 'text-destructive font-semibold';
		case 'deleted':
			return 'text-destructive';
		case 'added':
			return 'text-success';
		case 'renamed':
			return 'text-info';
		case 'untracked':
			return 'text-muted-foreground';
		case 'modified':
			return 'text-warning';
		default:
			break;
	}
	if (normalizeCount(request.warnings) > 0) return 'text-warning';
	return '';
}

/** Merge git status with diagnostic counts into one tree/tab decoration. */
export function resolveFileDecoration(request: DecorationRequest): FileDecoration | null {
	const errors = normalizeCount(request.errors);
	const warnings = normalizeCount(request.warnings);
	if (!request.git && errors === 0 && warnings === 0) return null;
	return {
		git: request.git,
		letter: request.git ? letterForGitLevel(request.git) : null,
		letterClass: letterClassFor({ git: request.git, errors, warnings }),
		textClass: textClassFor({ git: request.git, errors, warnings }),
		errors,
		warnings,
		hasErrors: errors > 0,
		hasWarnings: warnings > 0,
	};
}

/** Directory variant of the file merge. Callers pass aggregated inputs. */
export function resolveDirDecoration(request: DecorationRequest): FileDecoration | null {
	return resolveFileDecoration(request);
}
