/**
 * Autocomplete support for TUI components.
 */

import { createContextualLogger } from "@wf-agent/sdk/utils";

const logger = createContextualLogger({ component: "Autocomplete" });

export interface AutocompleteItem {
  label: string;
  value?: string;
  description?: string;
}

export interface AutocompleteSuggestions {
  items: AutocompleteItem[];
  cursorPosition?: number;
}

export interface AutocompleteProvider {
  getSuggestions(text: string, cursorPosition: number): Promise<AutocompleteSuggestions>;
}

export interface SlashCommand {
  name: string;
  description: string;
  handler: (args: string) => void | Promise<void>;
}

/**
 * Combined autocomplete provider that merges multiple providers.
 */
export class CombinedAutocompleteProvider implements AutocompleteProvider {
  private providers: AutocompleteProvider[] = [];

  constructor(...providers: AutocompleteProvider[]) {
    this.providers = providers;
  }

  addProvider(provider: AutocompleteProvider): void {
    this.providers.push(provider);
  }

  async getSuggestions(text: string, cursorPosition: number): Promise<AutocompleteSuggestions> {
    const allSuggestions: AutocompleteItem[] = [];

    for (const provider of this.providers) {
      try {
        const suggestions = await provider.getSuggestions(text, cursorPosition);
        allSuggestions.push(...suggestions.items);
      } catch (error) {
        // Ignore errors from individual providers
        logger.warn("Autocomplete provider error", {}, { 
          error: error instanceof Error ? error.message : String(error) 
        });
      }
    }

    return {
      items: allSuggestions,
      cursorPosition,
    };
  }
}
