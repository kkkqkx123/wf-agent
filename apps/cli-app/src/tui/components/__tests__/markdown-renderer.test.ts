import { describe, it, expect } from "vitest";
import { MarkdownRenderer } from "../markdown-renderer.js";

describe("MarkdownRenderer", () => {
  const renderer = new MarkdownRenderer();

  it("should render plain text as paragraphs", () => {
    const lines = renderer.render("Hello world", 80);
    expect(lines.length).toBeGreaterThan(0);
    expect(lines[0]).toBe("Hello world");
  });

  it("should render headings with bold prefix", () => {
    const lines = renderer.render("# Title", 80);
    expect(lines[0]).toContain("Title");
    expect(lines[0]).toContain("\x1b[1m");
  });

  it("should render multiple heading levels", () => {
    const h1 = renderer.render("# H1", 80);
    expect(h1[0]).toContain("# H1");
    const h2 = renderer.render("## H2", 80);
    expect(h2[0]).toContain("## H2");
    const h3 = renderer.render("### H3", 80);
    expect(h3[0]).toContain("### H3");
  });

  it("should render code blocks with dim text", () => {
    const lines = renderer.render("```\nconst x = 1;\n```", 80);
    expect(lines.some((l) => l.includes("const x = 1;"))).toBe(true);
    expect(lines.some((l) => l.includes("\x1b[2m"))).toBe(true);
  });

  it("should render blockquotes with > prefix", () => {
    const lines = renderer.render("> A quote", 80);
    expect(lines.some((l) => l.includes("A quote"))).toBe(true);
    expect(lines.some((l) => l.includes("\x1b[2m>"))).toBe(true);
  });

  it("should render unordered lists", () => {
    const lines = renderer.render("- item1\n- item2", 80);
    expect(lines.some((l) => l.includes("item1"))).toBe(true);
    expect(lines.some((l) => l.includes("item2"))).toBe(true);
  });

  it("should render ordered lists", () => {
    const lines = renderer.render("1. first\n2. second", 80);
    expect(lines.some((l) => l.includes("first"))).toBe(true);
    expect(lines.some((l) => l.includes("second"))).toBe(true);
  });

  it("should render thematic break", () => {
    const lines = renderer.render("---", 80);
    expect(lines.some((l) => l.includes("\u2500"))).toBe(true);
  });

  it("should render inline bold **text**", () => {
    const lines = renderer.render("This is **bold** text", 80);
    expect(lines[0]).toContain("\x1b[1mbold\x1b[0m");
  });

  it("should render inline italic *text*", () => {
    const lines = renderer.render("This is *italic* text", 80);
    expect(lines[0]).toContain("\x1b[4mitalic\x1b[0m");
  });

  it("should render inline code with backticks", () => {
    const lines = renderer.render("Use `code` here", 80);
    expect(lines[0]).toContain("\x1b[90mcode\x1b[0m");
  });

  it("should render links with URL", () => {
    const lines = renderer.render("Click [here](https://example.com)", 80);
    expect(lines[0]).toContain("here");
    expect(lines[0]).toContain("example.com");
  });

  it("should handle empty input", () => {
    const lines = renderer.render("", 80);
    expect(lines.length).toBe(0);
  });

  it("should wrap text within width", () => {
    const longText = "A very long line that should be wrapped when it exceeds the available width. ".repeat(5);
    const lines = renderer.render(longText, 40);
    expect(lines.length).toBeGreaterThan(1);
  });

  it("should ignore HTML tags", () => {
    const lines = renderer.render("<div>plain text</div>", 80);
    expect(lines.some((l) => l.includes("<div>"))).toBe(true);
  });

  it("should render a complete markdown document", () => {
    const md = `# Title

Some paragraph text here.

## Section

- list item 1
- list item 2

> A blockquote

\`\`\`
code block
\`\`\`
`;
    const lines = renderer.render(md, 80);
    expect(lines.length).toBeGreaterThan(10);
  });
});
