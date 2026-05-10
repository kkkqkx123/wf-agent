/**
 * Text Normalization Utilities Unit Tests
 */

import { describe, it, expect } from "vitest";
import { unescapeHtmlEntities, escapeHtmlEntities } from "../text-normalization.js";

describe("Text Normalization", () => {
  describe("unescapeHtmlEntities", () => {
    it("should unescape basic HTML entities", () => {
      const input = "&lt;div&gt;Hello&lt;/div&gt;";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("<div>Hello</div>");
    });

    it("should unescape ampersand entity", () => {
      const input = "Tom &amp; Jerry";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("Tom & Jerry");
    });

    it("should unescape quote entities", () => {
      const input = "&quot;quoted text&quot;";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe('"quoted text"');
    });

    it("should unescape numeric character references", () => {
      const input = "It&#39;s a test &#x2F; path &#x60;code&#x60;";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("It's a test / path `code`");
    });

    it("should handle mixed entities", () => {
      const input = "&lt;script&gt;alert(&#39;XSS&#39;)&lt;/script&gt;";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("<script>alert('XSS')</script>");
    });

    it("should return empty string for empty input", () => {
      expect(unescapeHtmlEntities("")).toBe("");
    });

    it("should return undefined for undefined input", () => {
      expect(unescapeHtmlEntities(undefined as any)).toBeUndefined();
    });

    it("should return null for null input", () => {
      expect(unescapeHtmlEntities(null as any)).toBeNull();
    });

    it("should return non-string input unchanged", () => {
      expect(unescapeHtmlEntities(123 as any)).toBe(123);
    });

    it("should not modify text without entities", () => {
      const input = "Plain text with no entities";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("Plain text with no entities");
    });

    it("should handle multiple occurrences of same entity", () => {
      const input = "&lt;a&gt;&lt;b&gt;&lt;c&gt;";
      const result = unescapeHtmlEntities(input);
      expect(result).toBe("<a><b><c>");
    });
  });

  describe("escapeHtmlEntities", () => {
    it("should escape less than and greater than", () => {
      const input = "<div>";
      const result = escapeHtmlEntities(input);
      expect(result).toBe("&lt;div&gt;");
    });

    it("should escape ampersand", () => {
      const input = "Tom & Jerry";
      const result = escapeHtmlEntities(input);
      expect(result).toBe("Tom &amp; Jerry");
    });

    it("should escape double quotes", () => {
      const input = 'Say "hello"';
      const result = escapeHtmlEntities(input);
      expect(result).toBe("Say &quot;hello&quot;");
    });

    it("should escape single quotes", () => {
      const input = "It's a test";
      const result = escapeHtmlEntities(input);
      expect(result).toBe("It&#39;s a test");
    });

    it("should escape all special characters", () => {
      const input = '<div class="test">&nbsp;</div>';
      const result = escapeHtmlEntities(input);
      expect(result).toBe("&lt;div class=&quot;test&quot;&gt;&amp;nbsp;&lt;/div&gt;");
    });

    it("should return empty string for empty input", () => {
      expect(escapeHtmlEntities("")).toBe("");
    });

    it("should return undefined for undefined input", () => {
      expect(escapeHtmlEntities(undefined as any)).toBeUndefined();
    });

    it("should return null for null input", () => {
      expect(escapeHtmlEntities(null as any)).toBeNull();
    });

    it("should return non-string input unchanged", () => {
      expect(escapeHtmlEntities(123 as any)).toBe(123);
    });

    it("should not modify text without special characters", () => {
      const input = "Plain text 123";
      const result = escapeHtmlEntities(input);
      expect(result).toBe("Plain text 123");
    });

    it("should handle multiple occurrences of same character", () => {
      const input = "a < b < c";
      const result = escapeHtmlEntities(input);
      expect(result).toBe("a &lt; b &lt; c");
    });

    it("should escape in correct order to avoid double escaping", () => {
      const input = "&<>";
      const result = escapeHtmlEntities(input);
      // Ampersand should be escaped first, then < and >
      expect(result).toBe("&amp;&lt;&gt;");
    });
  });

  describe("round-trip conversion", () => {
    it("should preserve text through escape and unescape", () => {
      const original = '<div class="test">Hello & welcome</div>';
      const escaped = escapeHtmlEntities(original);
      const unescaped = unescapeHtmlEntities(escaped);
      expect(unescaped).toBe(original);
    });

    it("should handle complex HTML", () => {
      const original = '<p>It\'s a "test" & <b>bold</b></p>';
      const escaped = escapeHtmlEntities(original);
      const unescaped = unescapeHtmlEntities(escaped);
      expect(unescaped).toBe(original);
    });
  });
});
