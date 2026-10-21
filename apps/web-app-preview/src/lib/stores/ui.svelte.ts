import { browser } from '$app/env';

export type ViewportKind = 'mobile' | 'tablet' | 'compact' | 'wide';

function kindFor(width: number): ViewportKind {
	if (width < 768) return 'mobile';
	if (width < 1024) return 'tablet';
	if (width < 1280) return 'compact';
	return 'wide';
}

class UiStore {
	viewportWidth = $state(browser ? window.innerWidth : 1440);
	mobileNavOpen = $state(false);
	inspectorOpen = $state(false);
	inspectorTitle = $state('');
	commandOpen = $state(false);
	helpOpen = $state(false);

	get viewport(): ViewportKind {
		return kindFor(this.viewportWidth);
	}

	get isMobile(): boolean {
		return this.viewport === 'mobile';
	}

	/** Wide viewports can keep the inspector docked; smaller ones overlay it. */
	get inspectorDocked(): boolean {
		return this.viewport === 'wide';
	}

	syncViewport(width: number): void {
		this.viewportWidth = width;
		if (width >= 768) this.mobileNavOpen = false;
	}

	toggleMobileNav(): void {
		this.mobileNavOpen = !this.mobileNavOpen;
	}

	closeMobileNav(): void {
		this.mobileNavOpen = false;
	}

	openInspector(title = ''): void {
		this.inspectorTitle = title;
		this.inspectorOpen = true;
	}

	closeInspector(): void {
		this.inspectorOpen = false;
	}

	toggleCommand(): void {
		this.commandOpen = !this.commandOpen;
	}

	setCommandOpen(open: boolean): void {
		this.commandOpen = open;
	}

	toggleHelp(): void {
		this.helpOpen = !this.helpOpen;
	}

	setHelpOpen(open: boolean): void {
		this.helpOpen = open;
	}
}

export const ui = new UiStore();
