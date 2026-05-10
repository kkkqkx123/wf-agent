import { eastAsianWidth } from "get-east-asian-width";

// Grapheme segmenter (shared instance)
const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/**
 * Get the shared grapheme segmenter instance.
 */
export function getSegmenter(): Intl.Segmenter {
  return segmenter;
}

/**
 * Check if a grapheme cluster (after segmentation) could possibly be an RGI emoji.
 * This is a fast heuristic to avoid the expensive rgiEmojiRegex test.
 */
function couldBeEmoji(segment: string): boolean {
  const cp = segment.codePointAt(0)!;
  return (
    (cp >= 0x1f000 && cp <= 0x1fbff) || // Emoji and Pictograph
    (cp >= 0x2300 && cp <= 0x23ff) || // Misc technical
    (cp >= 0x2600 && cp <= 0x27bf) || // Misc symbols, dingbats
    (cp >= 0x2b50 && cp <= 0x2b55) || // Specific stars/circles
    segment.includes("\uFE0F") || // Contains VS16 (emoji presentation selector)
    segment.length > 2 // Multi-codepoint sequences (ZWJ, skin tones, etc.)
  );
}

// Regexes for character classification (simplified versions without Unicode property escapes)
const NUL = '\u0000';
const US = '\u001f';
const DEL = '\u007f';
const zeroWidthRegex = new RegExp(`^[${NUL}-${US}${DEL}]+$`);
const leadingNonPrintingRegex = new RegExp(`^[${NUL}-${US}${DEL}]+`);

// Cache for non-ASCII strings
const WIDTH_CACHE_SIZE = 512;
const widthCache = new Map<string, number>();

function isPrintableAscii(str: string): boolean {
  for (let i = 0; i < str.length; i++) {
    const code = str.charCodeAt(i);
    if (code < 0x20 || code > 0x7e) {
      return false;
    }
  }
  return true;
}

/**
 * Calculate the terminal width of a single grapheme cluster.
 */
function graphemeWidth(segment: string): number {
  // Zero-width clusters (control characters)
  if (zeroWidthRegex.test(segment)) {
    return 0;
  }

  // Emoji detection using codepoint checks (no regex needed)
  if (couldBeEmoji(segment)) {
    // Check if it's likely an emoji by examining codepoints
    const cp = segment.codePointAt(0);
    if (cp !== undefined) {
      // Common emoji ranges
      if (
        (cp >= 0x1f600 && cp <= 0x1f64f) || // Emoticons
        (cp >= 0x1f300 && cp <= 0x1f5ff) || // Misc Symbols and Pictographs
        (cp >= 0x1f680 && cp <= 0x1f6ff) || // Transport and Map
        (cp >= 0x2600 && cp <= 0x26ff) || // Misc symbols
        (cp >= 0x2700 && cp <= 0x27bf) || // Dingbats
        (cp >= 0xfe00 && cp <= 0xfe0f) || // Variation Selectors
        (cp >= 0x1f900 && cp <= 0x1f9ff) || // Supplemental Symbols
        (cp >= 0x1fa00 && cp <= 0x1fa6f) || // Chess Symbols
        (cp >= 0x1fa70 && cp <= 0x1faff) // Symbols and Pictographs Extended-A
      ) {
        return 2;
      }
      // Multi-codepoint sequences are likely emojis
      if (segment.length > 2 || segment.includes("\u200d")) {
        return 2;
      }
    }
  }

  // Get base visible codepoint
  const base = segment.replace(leadingNonPrintingRegex, "");
  const cp = base.codePointAt(0);
  if (cp === undefined) {
    return 0;
  }

  // Regional indicator symbols are often rendered as full-width emoji
  if (cp >= 0x1f1e6 && cp <= 0x1f1ff) {
    return 2;
  }

  let width = eastAsianWidth(cp);

  // Trailing halfwidth/fullwidth forms
  if (segment.length > 1) {
    for (const char of segment.slice(1)) {
      const c = char.codePointAt(0)!;
      if (c >= 0xff00 && c <= 0xffef) {
        width += eastAsianWidth(c);
      }
    }
  }

  return width;
}

/**
 * Calculate the visible width of a string in terminal columns.
 */
export function visibleWidth(str: string): number {
  if (str.length === 0) {
    return 0;
  }

  // Fast path for ASCII-only strings
  if (isPrintableAscii(str)) {
    return str.length;
  }

  // Use cache for non-ASCII strings
  if (widthCache.has(str)) {
    return widthCache.get(str)!;
  }

  let width = 0;
  let hasAnsi = false;
  let i = 0;

  while (i < str.length) {
    // Skip ANSI escape sequences
    if (str[i] === "\x1b") {
      hasAnsi = true;
      const ESC = '\u001b';
      const BEL = '\u0007';
      const ansiMatch = str.slice(i).match(new RegExp('^' + ESC + '\\[[0-9;]*[A-Za-z]'));
      if (ansiMatch) {
        i += ansiMatch[0].length;
        continue;
      }
      // Handle other escape sequences
      const otherMatch = str.slice(i).match(new RegExp('^' + ESC + '[^' + ESC + ']*(?:' + ESC + '\\\\|[' + BEL + ESC + '])'));
      if (otherMatch) {
        i += otherMatch[0].length;
        continue;
      }
      i++;
      continue;
    }

    // Process grapheme clusters
    const segments = Array.from(segmenter.segment(str.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    width += graphemeWidth(segment);
    i += segment.length;
  }

  // Cache result if not too large
  if (!hasAnsi && str.length < 256) {
    if (widthCache.size >= WIDTH_CACHE_SIZE) {
      // Clear oldest entries (simple strategy)
      const firstKey = widthCache.keys().next().value;
      if (firstKey !== undefined) {
        widthCache.delete(firstKey);
      }
    }
    widthCache.set(str, width);
  }

  return width;
}

/**
 * Extract ANSI escape codes from text at a given position.
 */
function extractAnsiCode(text: string, pos: number): { code: string; length: number } | null {
  if (text[pos] !== "\x1b") {
    return null;
  }

  const remaining = text.slice(pos);
  const ESC = '\u001b';
  const BEL = '\u0007';

  // CSI sequence: ESC [ ... final byte
  const csiMatch = remaining.match(new RegExp('^' + ESC + '\\[[0-9;]*[A-Za-z]'));
  if (csiMatch) {
    return { code: csiMatch[0], length: csiMatch[0].length };
  }

  // OSC sequence: ESC ] ... ST (ESC \ or BEL)
  const oscMatch = remaining.match(new RegExp('^' + ESC + '\\][^' + BEL + ']*(?:' + BEL + '|' + ESC + '\\\\)'));
  if (oscMatch) {
    return { code: oscMatch[0], length: oscMatch[0].length };
  }

  // APC sequence: ESC _ ... ST (ESC \)
  const apcMatch = remaining.match(new RegExp('^' + ESC + '_[^' + ESC + ']*' + ESC + '\\\\'));
  if (apcMatch) {
    return { code: apcMatch[0], length: apcMatch[0].length };
  }

  // DCS sequence: ESC P ... ST (ESC \)
  const dcsMatch = remaining.match(new RegExp('^' + ESC + 'P[^' + ESC + ']*' + ESC + '\\\\'));
  if (dcsMatch) {
    return { code: dcsMatch[0], length: dcsMatch[0].length };
  }

  // Simple escape + single char
  if (remaining.length >= 2) {
    return { code: remaining.slice(0, 2), length: 2 };
  }

  return null;
}

/**
 * Truncate text to a maximum visible width, preserving ANSI codes.
 */
export function truncateToWidth(
  text: string,
  maxWidth: number,
  ellipsis: string = "",
  pad: boolean = false,
): string {
  if (maxWidth <= 0) {
    return "";
  }

  const ellipsisWidth = visibleWidth(ellipsis);
  if (ellipsisWidth > maxWidth) {
    return "";
  }

  const effectiveMaxWidth = maxWidth - ellipsisWidth;

  if (visibleWidth(text) <= effectiveMaxWidth) {
    return pad ? text + " ".repeat(maxWidth - visibleWidth(text) - ellipsisWidth) : text;
  }

  // Need to truncate
  let result = "";
  let currentWidth = 0;
  let i = 0;

  while (i < text.length) {
    // Skip ANSI codes
    const ansi = extractAnsiCode(text, i);
    if (ansi) {
      result += ansi.code;
      i += ansi.length;
      continue;
    }

    // Process grapheme cluster
    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = graphemeWidth(segment);

    if (currentWidth + segWidth > effectiveMaxWidth) {
      break;
    }

    result += segment;
    currentWidth += segWidth;
    i += segment.length;
  }

  return result + ellipsis;
}

/**
 * Wrap text to fit within a given width, preserving ANSI codes.
 */
export function wrapTextWithAnsi(text: string, width: number): string[] {
  if (width <= 0) {
    return [];
  }

  const lines: string[] = [];
  let currentLine = "";
  let currentWidth = 0;
  let i = 0;

  while (i < text.length) {
    // Handle newlines
    if (text[i] === "\n") {
      lines.push(currentLine);
      currentLine = "";
      currentWidth = 0;
      i++;
      continue;
    }

    // Skip ANSI codes (they don't add width)
    const ansi = extractAnsiCode(text, i);
    if (ansi) {
      currentLine += ansi.code;
      i += ansi.length;
      continue;
    }

    // Process grapheme cluster
    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = graphemeWidth(segment);

    // Word wrapping: if adding this segment would exceed width, start new line
    if (currentWidth + segWidth > width && currentWidth > 0) {
      lines.push(currentLine);
      currentLine = "";
      currentWidth = 0;
    }

    currentLine += segment;
    currentWidth += segWidth;
    i += segment.length;
  }

  // Add final line
  if (currentLine.length > 0 || lines.length === 0) {
    lines.push(currentLine);
  }

  return lines;
}

/**
 * Extract text segments before, at, and after a column range.
 * Used for overlay compositing.
 */
export function extractSegments(
  line: string,
  startCol: number,
  endCol: number,
  maxAfterWidth: number,
  strict: boolean = false,
): { before: string; beforeWidth: number; after: string; afterWidth: number } {
  let before = "";
  let after = "";
  let beforeWidth = 0;
  let afterWidth = 0;
  let currentWidth = 0;
  let i = 0;
  let inRange = false;

  while (i < line.length) {
    // Skip ANSI codes
    const ansi = extractAnsiCode(line, i);
    if (ansi) {
      if (inRange) {
        after += ansi.code;
      } else {
        before += ansi.code;
      }
      i += ansi.length;
      continue;
    }

    // Process grapheme cluster
    const segments = Array.from(segmenter.segment(line.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = graphemeWidth(segment);

    if (!inRange && currentWidth + segWidth > startCol) {
      // Entering the range
      inRange = true;
    }

    if (inRange) {
      if (currentWidth >= endCol) {
        // Past the range
        if (afterWidth < maxAfterWidth) {
          after += segment;
          afterWidth += segWidth;
        }
        if (strict && afterWidth >= maxAfterWidth) {
          break;
        }
      } else {
        // In the range - skip (this is the overlay area)
      }
    } else {
      before += segment;
      beforeWidth += segWidth;
    }

    currentWidth += segWidth;
    i += segment.length;
  }

  return { before, beforeWidth, after, afterWidth };
}

/**
 * Slice text by column positions, preserving ANSI codes.
 */
export function sliceByColumn(text: string, startCol: number, endCol: number, strict: boolean = false): string {
  let result = "";
  let currentWidth = 0;
  let i = 0;

  while (i < text.length) {
    // Skip ANSI codes
    const ansi = extractAnsiCode(text, i);
    if (ansi) {
      if (currentWidth >= startCol && currentWidth < endCol) {
        result += ansi.code;
      }
      i += ansi.length;
      continue;
    }

    // Process grapheme cluster
    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = graphemeWidth(segment);

    if (currentWidth >= startCol && currentWidth < endCol) {
      if (strict && currentWidth + segWidth > endCol) {
        break;
      }
      result += segment;
    }

    currentWidth += segWidth;
    i += segment.length;
  }

  return result;
}

/**
 * Slice text with width tracking.
 */
export function sliceWithWidth(text: string, startCol: number, maxWidth: number, strict: boolean = false): {
  text: string;
  width: number;
} {
  let result = "";
  let currentWidth = 0;
  let extractedWidth = 0;
  let i = 0;

  // Move to start position
  while (i < text.length && currentWidth < startCol) {
    const ansi = extractAnsiCode(text, i);
    if (ansi) {
      i += ansi.length;
      continue;
    }

    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    currentWidth += graphemeWidth(segment);
    i += segment.length;
  }

  // Extract up to maxWidth
  while (i < text.length && extractedWidth < maxWidth) {
    const ansi = extractAnsiCode(text, i);
    if (ansi) {
      result += ansi.code;
      i += ansi.length;
      continue;
    }

    const segments = Array.from(segmenter.segment(text.slice(i)));
    if (segments.length === 0) break;

    const { segment } = segments[0]!;
    const segWidth = graphemeWidth(segment);

    if (strict && extractedWidth + segWidth > maxWidth) {
      break;
    }

    result += segment;
    extractedWidth += segWidth;
    i += segment.length;
  }

  return { text: result, width: extractedWidth };
}

/**
 * Apply background color to a line, padding it to the specified width.
 *
 * @param line - The line content (may contain ANSI codes)
 * @param width - Target width for padding
 * @param bgFn - Function to apply background color
 * @returns Line with background applied and padded to width
 */
export function applyBackgroundToLine(
  line: string,
  width: number,
  bgFn: (text: string) => string
): string {
  // Calculate padding needed
  const visibleLen = visibleWidth(line);
  const paddingNeeded = Math.max(0, width - visibleLen);
  const padding = " ".repeat(paddingNeeded);

  // Apply background to content + padding
  const withPadding = line + padding;
  return bgFn(withPadding);
}

/**
 * Check if a character is whitespace.
 */
export function isWhitespaceChar(char: string): boolean {
  return /\s/.test(char);
}

/**
 * Check if a character is punctuation.
 */
export function isPunctuationChar(char: string): boolean {
  // Common punctuation ranges and characters
  const cp = char.codePointAt(0);
  if (cp === undefined) return false;
  
  return (
    (cp >= 0x21 && cp <= 0x2f) || // !"#$%&'()*+,-./
    (cp >= 0x3a && cp <= 0x40) || // :;<=>?@
    (cp >= 0x5b && cp <= 0x60) || // [\]^_`
    (cp >= 0x7b && cp <= 0x7e) || // {|}~
    (cp >= 0x3000 && cp <= 0x303f) || // CJK punctuation
    (cp >= 0xff01 && cp <= 0xff0f) || // Fullwidth punctuation
    (cp >= 0xff1a && cp <= 0xff20) ||
    (cp >= 0xff3b && cp <= 0xff40) ||
    (cp >= 0xff5b && cp <= 0xff65)
  );
}

/**
 * Represents a chunk of text for word-wrap layout.
 * Tracks both the text content and its position in the original line.
 */
export interface TextChunk {
  text: string;
  startIndex: number;
  endIndex: number;
}

/**
 * Split a line into word-wrapped chunks.
 * Wraps at word boundaries when possible, falling back to character-level
 * wrapping for words longer than the available width.
 *
 * @param line - The text line to wrap
 * @param maxWidth - Maximum visible width per chunk
 * @param preSegmented - Optional pre-segmented graphemes
 * @returns Array of chunks with text and position information
 */
export function wordWrapLine(
  line: string,
  maxWidth: number,
  preSegmented?: Intl.SegmentData[]
): TextChunk[] {
  if (!line || maxWidth <= 0) {
    return [{ text: "", startIndex: 0, endIndex: 0 }];
  }

  const lineWidth = visibleWidth(line);
  if (lineWidth <= maxWidth) {
    return [{ text: line, startIndex: 0, endIndex: line.length }];
  }

  const chunks: TextChunk[] = [];
  const segments = preSegmented ?? [...segmenter.segment(line)];

  let currentWidth = 0;
  let chunkStart = 0;

  // Wrap opportunity: the position after the last whitespace before a non-whitespace
  let wrapOppIndex = -1;
  let wrapOppWidth = 0;

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]!;
    const grapheme = seg.segment;
    const gWidth = visibleWidth(grapheme);
    const charIndex = seg.index;
    const isWs = isWhitespaceChar(grapheme);

    // Overflow check before advancing
    if (currentWidth + gWidth > maxWidth) {
      if (wrapOppIndex >= 0 && currentWidth - wrapOppWidth + gWidth <= maxWidth) {
        // Backtrack to last wrap opportunity
        chunks.push({
          text: line.slice(chunkStart, wrapOppIndex),
          startIndex: chunkStart,
          endIndex: wrapOppIndex,
        });
        chunkStart = wrapOppIndex;
        currentWidth -= wrapOppWidth;
      } else if (chunkStart < charIndex) {
        // No viable wrap opportunity: force-break at current position
        chunks.push({
          text: line.slice(chunkStart, charIndex),
          startIndex: chunkStart,
          endIndex: charIndex,
        });
        chunkStart = charIndex;
        currentWidth = 0;
      }
      wrapOppIndex = -1;
    }

    if (gWidth > maxWidth) {
      // Single grapheme wider than maxWidth - re-wrap it
      const subChunks = wordWrapLine(grapheme, maxWidth);
      for (let j = 0; j < subChunks.length - 1; j++) {
        const sc = subChunks[j]!;
        chunks.push({
          text: sc.text,
          startIndex: charIndex + sc.startIndex,
          endIndex: charIndex + sc.endIndex,
        });
      }
      const last = subChunks[subChunks.length - 1]!;
      chunkStart = charIndex + last.startIndex;
      currentWidth = visibleWidth(last.text);
      wrapOppIndex = -1;
      continue;
    }

    // Advance
    currentWidth += gWidth;

    // Record wrap opportunity: whitespace followed by non-whitespace
    const next = segments[i + 1];
    if (isWs && next && !isWhitespaceChar(next.segment)) {
      wrapOppIndex = next.index;
      wrapOppWidth = currentWidth;
    }
  }

  // Push final chunk
  chunks.push({
    text: line.slice(chunkStart),
    startIndex: chunkStart,
    endIndex: line.length,
  });

  return chunks;
}
