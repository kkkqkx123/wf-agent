class UiStore {
	mobileNavOpen = $state(false);
	commandOpen = $state(false);

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
}

export const ui = new UiStore();
