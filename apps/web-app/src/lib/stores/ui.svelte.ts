const RECENT_LIMIT = 12;

export interface RecentEntry {
	href: string;
	label: string;
	visitedAt: number;
}

class UiStore {
	mobileNavOpen = $state(false);
	commandOpen = $state(false);
	recent = $state<RecentEntry[]>([]);

	toggleMobileNav(): void {
		this.mobileNavOpen = !this.mobileNavOpen;
	}

	closeMobileNav(): void {
		this.mobileNavOpen = false;
	}

	toggleCommand(): void {
		this.commandOpen = !this.commandOpen;
	}

	setCommandOpen(open: boolean): void {
		this.commandOpen = open;
	}

	recordVisit(href: string, label: string): void {
		const next = this.recent.filter((entry) => entry.href !== href);
		next.unshift({ href, label, visitedAt: Date.now() });
		this.recent = next.slice(0, RECENT_LIMIT);
	}
}

export const ui = new UiStore();
