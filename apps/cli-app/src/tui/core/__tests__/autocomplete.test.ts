import { describe, it, expect, beforeEach } from "vitest";
import { CombinedAutocompleteProvider, type AutocompleteItem, type AutocompleteSuggestions } from "../autocomplete.js";

class MockAutocompleteProvider {
  private items: AutocompleteItem[];
  
  constructor(items: AutocompleteItem[]) {
    this.items = items;
  }
  
  async getSuggestions(text: string, cursorPosition: number): Promise<AutocompleteSuggestions> {
    const filtered = this.items.filter(item => 
      item.label.toLowerCase().includes(text.toLowerCase())
    );
    return {
      items: filtered,
      cursorPosition,
    };
  }
}

describe("CombinedAutocompleteProvider", () => {
  let provider1: MockAutocompleteProvider;
  let provider2: MockAutocompleteProvider;
  let combined: CombinedAutocompleteProvider;

  beforeEach(() => {
    provider1 = new MockAutocompleteProvider([
      { label: "apple", value: "a", description: "A fruit" },
      { label: "apricot", value: "ap", description: "Another fruit" },
    ]);
    
    provider2 = new MockAutocompleteProvider([
      { label: "banana", value: "b", description: "A yellow fruit" },
      { label: "blueberry", value: "bl", description: "A blue fruit" },
    ]);
    
    combined = new CombinedAutocompleteProvider(provider1 as any, provider2 as any);
  });

  describe("Initialization", () => {
    it("should initialize with multiple providers", () => {
      expect(combined).toBeDefined();
    });

    it("should accept empty providers list", () => {
      const empty = new CombinedAutocompleteProvider();
      expect(empty).toBeDefined();
    });
  });

  describe("addProvider", () => {
    it("should add a new provider", async () => {
      const newProvider = new MockAutocompleteProvider([
        { label: "cherry", value: "c" },
      ]);
      
      combined.addProvider(newProvider as any);
      const result = await combined.getSuggestions("cherry", 0);
      
      expect(result.items.some(item => item.label === "cherry")).toBe(true);
    });

    it("should accumulate suggestions from all providers", async () => {
      const result = await combined.getSuggestions("", 0);
      
      expect(result.items.length).toBe(4);
      expect(result.items.map(i => i.label)).toEqual(
        expect.arrayContaining(["apple", "apricot", "banana", "blueberry"])
      );
    });
  });

  describe("getSuggestions", () => {
    it("should merge suggestions from all providers", async () => {
      const result = await combined.getSuggestions("a", 0);
      
      expect(result.items.length).toBeGreaterThanOrEqual(2);
      expect(result.items.map(i => i.label)).toEqual(
        expect.arrayContaining(["apple", "apricot"])
      );
    });

    it("should filter based on text input", async () => {
      const result = await combined.getSuggestions("ban", 0);
      
      expect(result.items.length).toBe(1);
      expect(result.items[0]?.label).toBe("banana");
    });

    it("should preserve cursor position", async () => {
      const result = await combined.getSuggestions("test", 5);
      
      expect(result.cursorPosition).toBe(5);
    });

    it("should handle provider errors gracefully", async () => {
      const errorProvider = {
        getSuggestions: async () => {
          throw new Error("Provider error");
        },
      };
      
      const combinedWithError = new CombinedAutocompleteProvider(errorProvider as any, provider1 as any);
      const result = await combinedWithError.getSuggestions("apple", 0);
      
      // Should still get results from working provider
      expect(result.items.length).toBeGreaterThan(0);
    });

    it("should return empty array when no matches", async () => {
      const result = await combined.getSuggestions("xyz123", 0);
      
      expect(result.items.length).toBe(0);
    });

    it("should handle empty text", async () => {
      const result = await combined.getSuggestions("", 0);
      
      expect(result.items.length).toBe(4);
    });
  });

  describe("Error Handling", () => {
    it("should continue if one provider fails", async () => {
      const failingProvider = {
        getSuggestions: async () => {
          throw new Error("Fail");
        },
      };
      
      const combinedWithFailure = new CombinedAutocompleteProvider(
        failingProvider as any,
        provider1 as any
      );
      
      const result = await combinedWithFailure.getSuggestions("apple", 0);
      expect(result.items.length).toBeGreaterThan(0);
    });

    it("should handle all providers failing", async () => {
      const failingProvider1 = {
        getSuggestions: async () => { throw new Error("Fail 1"); },
      };
      const failingProvider2 = {
        getSuggestions: async () => { throw new Error("Fail 2"); },
      };
      
      const allFailing = new CombinedAutocompleteProvider(
        failingProvider1 as any,
        failingProvider2 as any
      );
      
      const result = await allFailing.getSuggestions("test", 0);
      expect(result.items.length).toBe(0);
    });
  });
});
