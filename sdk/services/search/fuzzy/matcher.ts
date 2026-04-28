/**
 * Fuzzy Matcher
 *
 * Provides fuzzy string matching algorithm for file search.
 */

/**
 * Fuzzy match result
 */
export interface FuzzyMatchResult {
  /** Match score */
  score: number;
  /** Match positions in the text */
  positions: number[];
}

/**
 * Perform fuzzy matching between text and query
 */
export function fuzzyMatch(text: string, query: string): FuzzyMatchResult | null {
  const textLower = text.toLowerCase();
  const queryLower = query.toLowerCase();

  let queryIndex = 0;
  let score = 0;
  const positions: number[] = [];

  for (let i = 0; i < text.length && queryIndex < query.length; i++) {
    if (textLower[i] === queryLower[queryIndex]) {
      positions.push(i);
      queryIndex++;

      // Bonus for matching at word boundaries
      if (i === 0 || text[i - 1] === "/" || text[i - 1] === "_" || text[i - 1] === "-") {
        score += 2;
      } else {
        score += 1;
      }
    }
  }

  if (queryIndex === query.length) {
    return { score, positions };
  }

  return null;
}

/**
 * Sort items by fuzzy match score
 */
export function sortByFuzzyMatch<T>(
  items: T[],
  query: string,
  getSearchText: (item: T) => string,
): Array<{ item: T; score: number }> {
  const matched = items
    .map((item) => {
      const searchText = getSearchText(item);
      const match = fuzzyMatch(searchText, query);
      return match ? { item, score: match.score } : null;
    })
    .filter((result): result is { item: T; score: number } => result !== null);

  // Sort by score (descending) and path length (ascending for tie-breaking)
  matched.sort((a, b) => {
    if (b.score !== a.score) {
      return b.score - a.score;
    }
    const aText = getSearchText(a.item);
    const bText = getSearchText(b.item);
    return aText.length - bText.length;
  });

  return matched;
}
