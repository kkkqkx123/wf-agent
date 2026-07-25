import { wrapTextWithAnsi } from "../core/utils.js";

const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";
const DIM = "\x1b[2m";
const UNDERLINE = "\x1b[4m";
const CYAN = "\x1b[36m";
const GREEN = "\x1b[32m";
const BLUE = "\x1b[34m";
const GRAY = "\x1b[90m";

type InlineToken =
  | { type: "text"; text: string }
  | { type: "bold"; text: string }
  | { type: "italic"; text: string }
  | { type: "code"; text: string }
  | { type: "link"; text: string; url: string };

function parseInline(input: string): InlineToken[] {
  const tokens: InlineToken[] = [];
  let i = 0;

  while (i < input.length) {
    if (input[i] === "\\" && i + 1 < input.length) {
      tokens.push({ type: "text", text: input[i + 1]! });
      i += 2;
      continue;
    }

    if (input[i] === "`") {
      const end = input.indexOf("`", i + 1);
      if (end !== -1) {
        tokens.push({ type: "code", text: input.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }

    if (input[i] === "*" && i + 1 < input.length && input[i + 1] === "*") {
      const end = input.indexOf("**", i + 2);
      if (end !== -1) {
        tokens.push({ type: "bold", text: input.slice(i + 2, end) });
        i = end + 2;
        continue;
      }
    }

    if (input[i] === "_" && i + 1 < input.length && input[i + 1] === "_") {
      const end = input.indexOf("__", i + 2);
      if (end !== -1) {
        tokens.push({ type: "bold", text: input.slice(i + 2, end) });
        i = end + 2;
        continue;
      }
    }

    if (input[i] === "*") {
      const end = input.indexOf("*", i + 1);
      if (end !== -1 && input[end + 1] !== "*") {
        tokens.push({ type: "italic", text: input.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }

    if (input[i] === "_" && i + 1 < input.length && input[i + 1] !== "_") {
      const end = input.indexOf("_", i + 1);
      if (end !== -1 && input[end + 1] !== "_") {
        tokens.push({ type: "italic", text: input.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }

    const linkMatch = input.slice(i).match(/^\[([^\]]+)\]\(([^)]+)\)/);
    if (linkMatch) {
      tokens.push({ type: "link", text: linkMatch[1]!, url: linkMatch[2]! });
      i += linkMatch[0].length;
      continue;
    }

    tokens.push({ type: "text", text: input[i]! });
    i++;
  }

  return tokens;
}

function renderInline(tokens: InlineToken[]): string {
  let result = "";
  for (const t of tokens) {
    switch (t.type) {
      case "bold":
        result += `${BOLD}${t.text}${RESET}`;
        break;
      case "italic":
        result += `${UNDERLINE}${t.text}${RESET}`;
        break;
      case "code":
        result += `${GRAY}${t.text}${RESET}`;
        break;
      case "link":
        result += `${BLUE}${UNDERLINE}${t.text}${RESET}${DIM} (${t.url})${RESET}`;
        break;
      case "text":
        result += t.text;
        break;
    }
  }
  return result;
}

function countLeading(s: string, ch: string): number {
  let count = 0;
  for (const c of s) {
    if (c === ch) count++;
    else break;
  }
  return count;
}

interface Block {
  type: "heading" | "code" | "blockquote" | "ulist" | "olist" | "paragraph" | "thematicbreak";
  level?: number;
  content?: string;
  items?: string[];
}

function parseBlocks(input: string): Block[] {
  const lines = input.split("\n");
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i]!;

    if (/^#{1,6}\s/.test(line)) {
      const level = countLeading(line, "#");
      blocks.push({ type: "heading", level, content: line.slice(level).trim() });
      i++;
      continue;
    }

    if (/^```/.test(line)) {
      const fence = line.match(/^```(\w*)/)?.[1] ?? "";
      i++;
      const codeLines: string[] = [];
      while (i < lines.length && !/^```/.test(lines[i]!)) {
        codeLines.push(lines[i]!);
        i++;
      }
      i++;
      blocks.push({ type: "code", content: codeLines.join("\n"), level: fence.length > 0 ? 1 : 0 });
      continue;
    }

    if (/^>\s?/.test(line)) {
      const quoteLines: string[] = [];
      while (i < lines.length && /^>/.test(lines[i]!)) {
        quoteLines.push(lines[i]!.replace(/^>\s?/, ""));
        i++;
      }
      blocks.push({ type: "blockquote", content: quoteLines.join("\n") });
      continue;
    }

    if (/^[-*+]\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*+]\s/.test(lines[i]!)) {
        items.push(lines[i]!.replace(/^[-*+]\s/, ""));
        i++;
      }
      blocks.push({ type: "ulist", items });
      continue;
    }

    if (/^\d+[.)]\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+[.)]\s/.test(lines[i]!)) {
        items.push(lines[i]!.replace(/^\d+[.)]\s/, ""));
        i++;
      }
      blocks.push({ type: "olist", items });
      continue;
    }

    if (/^---+$/.test(line) || /^\*\*\*+$/.test(line)) {
      blocks.push({ type: "thematicbreak", content: "" });
      i++;
      continue;
    }

    const paraLines: string[] = [];
    while (i < lines.length && lines[i]!.trim() !== "" && !/^[#>*-]/.test(lines[i]!) && !/^\d+[.)]\s/.test(lines[i]!) && !/^```/.test(lines[i]!)) {
      paraLines.push(lines[i]!);
      i++;
    }
    if (paraLines.length > 0) {
      blocks.push({ type: "paragraph", content: paraLines.join(" ") });
    }

    while (i < lines.length && lines[i]!.trim() === "") {
      i++;
    }
  }

  return blocks;
}

export class MarkdownRenderer {
  render(markdown: string, width: number): string[] {
    const result: string[] = [];
    const blocks = parseBlocks(markdown);

    for (const block of blocks) {
      switch (block.type) {
        case "heading": {
          const prefix = "#".repeat(block.level ?? 1);
          const rendered = renderInline(parseInline(block.content!));
          result.push(`${BOLD}${prefix} ${rendered}${RESET}`);
          result.push("");
          break;
        }

        case "code": {
          const lines = block.content!.split("\n");
          for (const line of lines) {
            result.push(`  ${DIM}${line}${RESET}`);
          }
          result.push("");
          break;
        }

        case "blockquote": {
          const wrapped = wrapTextWithAnsi(block.content!, Math.max(1, width - 4));
          for (const line of wrapped) {
            result.push(`${DIM}>${RESET} ${line}`);
          }
          result.push("");
          break;
        }

        case "ulist": {
          for (const item of block.items ?? []) {
            const rendered = renderInline(parseInline(item));
            const wrapped = wrapTextWithAnsi(rendered, Math.max(1, width - 4));
            if (wrapped.length > 0) {
              result.push(`  ${GREEN}•${RESET} ${wrapped[0]}`);
              for (let j = 1; j < wrapped.length; j++) {
                result.push(`    ${wrapped[j]}`);
              }
            }
          }
          result.push("");
          break;
        }

        case "olist": {
          for (let idx = 0; idx < (block.items ?? []).length; idx++) {
            const item = block.items![idx]!;
            const rendered = renderInline(parseInline(item));
            const wrapped = wrapTextWithAnsi(rendered, Math.max(1, width - 6));
            if (wrapped.length > 0) {
              result.push(`  ${CYAN}${idx + 1}.${RESET} ${wrapped[0]}`);
              for (let j = 1; j < wrapped.length; j++) {
                result.push(`     ${wrapped[j]}`);
              }
            }
          }
          result.push("");
          break;
        }

        case "thematicbreak": {
          const hr = "─".repeat(Math.min(width, 40));
          result.push(`${DIM}${hr}${RESET}`);
          result.push("");
          break;
        }

        case "paragraph": {
          const rendered = renderInline(parseInline(block.content!));
          const wrapped = wrapTextWithAnsi(rendered, width);
          for (const line of wrapped) {
            result.push(line);
          }
          result.push("");
          break;
        }
      }
    }

    return result;
  }
}
