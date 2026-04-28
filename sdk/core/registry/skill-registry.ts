/**
 * Skill Registry Service
 *
 * Responsible for the discovery, parsing, and management of skills
 * Complies with the Claude Code Skill specifications
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { Skill, SkillMetadata, SkillConfig, SkillMatchResult } from "@wf-agent/types";
import {
  SkillParseError as SkillParseErrorClass,
  SkillValidationError as SkillValidationErrorClass,
} from "@wf-agent/types";
import { sdkLogger as logger } from "../../utils/logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";

/**
 * Skill Registry Class
 */
export class SkillRegistry {
  private skills: Map<string, Skill> = new Map();
  private config: SkillConfig;
  private contentCache: Map<string, { content: string; timestamp: number }> = new Map();
  private resourceCache: Map<string, { content: string | Buffer; timestamp: number }> = new Map();

  constructor(config: SkillConfig) {
    this.config = {
      autoScan: true,
      cacheEnabled: true,
      cacheTTL: 300000, // 5 minutes
      ...config,
    };
  }

  /**
   * Initialize the Skill registry
   * Scan all configured Skill directories
   */
  async initialize(): Promise<void> {
    if (!this.config.autoScan) {
      return;
    }

    for (const skillPath of this.config.paths) {
      await this.scanSkills(skillPath);
    }
  }

  /**
   * Scan the Skill directory
   * @param skillsPath Path to the Skill directory
   */
  async scanSkills(skillsPath: string): Promise<void> {
    const absolutePath = path.resolve(skillsPath);

    try {
      const entries = await fs.readdir(absolutePath, { withFileTypes: true });

      for (const entry of entries) {
        if (!entry.isDirectory()) {
          continue;
        }

        const skillDir = path.join(absolutePath, entry.name);
        const skillMdPath = path.join(skillDir, "SKILL.md");

        try {
          await fs.access(skillMdPath);
          await this.loadSkill(skillDir);
        } catch {
          // The SKILL.md file is not in the directory; skip it.
          continue;
        }
      }
    } catch (error) {
      // The directory does not exist or is inaccessible; ignore it.
      logger.warn(`Failed to scan skills directory: ${absolutePath}`, {
        path: absolutePath,
        error: getErrorOrNew(error),
      });
    }
  }

  /**
   * Load Skill
   * @param skillDir Path to the Skill directory
   * @throws SkillParseError If parsing fails
   */
  private async loadSkill(skillDir: string): Promise<void> {
    const skillMdPath = path.join(skillDir, "SKILL.md");

    try {
      const content = await fs.readFile(skillMdPath, "utf-8");
      const metadata = this.parseSkillMd(content, skillDir);

      // Verify that the directory name matches the name field.
      const dirName = path.basename(skillDir);
      if (metadata.name !== dirName) {
        throw new SkillValidationErrorClass(
          metadata.name,
          `Skill directory name '${dirName}' does not match skill name '${metadata.name}'`,
        );
      }

      const skill: Skill = {
        metadata,
        path: skillDir,
      };

      this.skills.set(metadata.name, skill);
    } catch (error) {
      if (error instanceof SkillValidationErrorClass) {
        throw error;
      }

      throw new SkillParseErrorClass(
        skillDir,
        "Failed to parse SKILL.md",
        error instanceof Error ? error : new Error(String(error)),
      );
    }
  }

  /**
   * Parsing the SKILL.md file
   * @param content SKILL.md file content
   * @param skillDir Skill directory path
   * @returns Skill metadata
   * @throws SkillParseError If parsing fails.
   */
  private parseSkillMd(content: string, skillDir: string): SkillMetadata {
    // Extract the YAML frontmatter.
    const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---/);

    if (!frontmatterMatch || !frontmatterMatch[1]) {
      throw new SkillParseErrorClass(skillDir, "Missing YAML frontmatter");
    }

    const frontmatter = frontmatterMatch[1];

    try {
      // Simple YAML parsing (without relying on external libraries)
      const metadata = this.parseYamlFrontmatter(frontmatter);

      // Verify required fields
      if (!metadata["name"]) {
        throw new SkillParseErrorClass(skillDir, "Missing required field: name");
      }

      if (!metadata["description"]) {
        throw new SkillParseErrorClass(skillDir, "Missing required field: description");
      }

      // Verify the name format.
      const name = metadata["name"] as string;
      if (!/^[a-z0-9-]+$/.test(name)) {
        throw new SkillParseErrorClass(
          skillDir,
          `Invalid skill name '${name}': must be lowercase alphanumeric with hyphens only`,
        );
      }

      return {
        name: metadata["name"] as string,
        description: metadata["description"] as string,
        version: metadata["version"] as string | undefined,
        license: metadata["license"] as string | undefined,
        allowedTools: metadata["allowedTools"] as string[] | undefined,
        metadata: metadata["metadata"] as Record<string, string> | undefined,
      };
    } catch (error) {
      if (error instanceof SkillParseErrorClass) {
        throw error;
      }

      throw new SkillParseErrorClass(
        skillDir,
        "Failed to parse YAML frontmatter",
        error instanceof Error ? error : new Error(String(error)),
      );
    }
  }

  /**
   * Simple YAML frontmatter parsing
   * @param yaml: A YAML string
   * @returns: The parsed object
   */
  private parseYamlFrontmatter(yaml: string): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    const lines = yaml.split("\n");
    let inArray = false;
    let arrayKey = "";

    for (const line of lines) {
      const trimmed = line.trim();

      if (!trimmed || trimmed.startsWith("#")) {
        continue;
      }

      // Array items
      if (trimmed.startsWith("- ")) {
        if (inArray && arrayKey) {
          if (!result[arrayKey]) {
            result[arrayKey] = [];
          }
          (result[arrayKey] as string[]).push(trimmed.substring(2).trim());
        }
        continue;
      }

      // key-value pair
      const colonIndex = trimmed.indexOf(":");
      if (colonIndex > 0) {
        const key = trimmed.substring(0, colonIndex).trim();
        const value = trimmed.substring(colonIndex + 1).trim();

        // Check if it is the start of an array.
        if (value === "" || value === "[]") {
          arrayKey = key;
          inArray = true;
          if (value === "[]") {
            result[key] = [];
          }
        } else {
          // Parse the value
          result[key] = this.parseYamlValue(value);
          inArray = false;
        }
      }
    }

    return result;
  }

  /**
   * Parse a YAML value
   * @param value A string representing the YAML value
   * @returns The parsed value
   */
  private parseYamlValue(value: string): unknown {
    // Remove the quotes.
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      return value.slice(1, -1);
    }

    // Boolean values
    if (value === "true") return true;
    if (value === "false") return false;

    // Numbers
    const num = Number(value);
    if (!isNaN(num)) return num;

    // null
    if (value === "null" || value === "~") return null;

    // The default is a string.
    return value;
  }

  /**
   * Get all Skill metadata
   * @returns Array of Skill metadata
   */
  getAllSkills(): SkillMetadata[] {
    return Array.from(this.skills.values()).map(skill => skill.metadata);
  }

  /**
   * Get the Skill based on the name
   * @param name: The name of the Skill
   * @returns: The Skill or undefined
   */
  getSkill(name: string): Skill | undefined {
    return this.skills.get(name);
  }

  /**
   * Match Skills based on the description
   * @param query The query string
   * @returns An array of matching results
   */
  matchSkills(query: string): SkillMatchResult[] {
    const results: SkillMatchResult[] = [];
    const queryLower = query.toLowerCase();

    for (const skill of this.skills.values()) {
      const score = this.calculateMatchScore(queryLower, skill.metadata);

      if (score > 0) {
        results.push({
          skill: skill.metadata,
          score,
          reason: `Description contains relevant keywords`,
        });
      }
    }

    // Sort in descending order of score.
    return results.sort((a, b) => b.score - a.score);
  }

  /**
   * Calculate the matching score
   * @param query The query string (in lowercase)
   * @param metadata Skill metadata
   * @returns The matching score (ranging from 0 to 1)
   */
  private calculateMatchScore(query: string, metadata: SkillMetadata): number {
    const description = metadata.description.toLowerCase();
    const name = metadata.name.toLowerCase();

    // Exactly match the name
    if (query.includes(name)) {
      return 1.0;
    }

    // Partial match of names
    if (name.includes(query)) {
      return 0.8;
    }

    // Extract keywords
    const keywords = this.extractKeywords(description);
    const queryWords = query.split(/\s+/);

    let matchCount = 0;
    for (const word of queryWords) {
      if (keywords.some(keyword => keyword.includes(word) || word.includes(keyword))) {
        matchCount++;
      }
    }

    if (matchCount === 0) {
      return 0;
    }

    // Calculate the score based on the number of matching keywords.
    return Math.min(0.7, (matchCount / queryWords.length) * 0.7);
  }

  /**
   * Extract keywords
   * @param text: The text
   * @returns: An array of keywords
   */
  private extractKeywords(text: string): string[] {
    // Remove common stopwords.
    const stopWords = new Set([
      "a",
      "an",
      "the",
      "is",
      "are",
      "was",
      "were",
      "be",
      "been",
      "being",
      "have",
      "has",
      "had",
      "do",
      "does",
      "did",
      "will",
      "would",
      "should",
      "could",
      "may",
      "might",
      "must",
      "shall",
      "can",
      "need",
      "dare",
      "ought",
      "used",
      "to",
      "of",
      "in",
      "for",
      "on",
      "with",
      "at",
      "by",
      "from",
      "as",
      "into",
      "through",
      "during",
      "before",
      "after",
      "above",
      "below",
      "and",
      "but",
      "or",
      "nor",
      "so",
      "yet",
      "both",
      "either",
      "neither",
      "not",
      "only",
      "own",
      "same",
      "than",
      "too",
      "very",
      "just",
      "when",
      "where",
      "why",
      "how",
      "all",
      "each",
      "every",
      "any",
      "some",
      "no",
      "this",
      "that",
      "these",
      "those",
      "i",
      "you",
      "he",
      "she",
      "it",
      "we",
      "they",
    ]);

    return text
      .toLowerCase()
      .split(/\s+/)
      .filter(word => word.length > 2 && !stopWords.has(word))
      .map(word => word.replace(/[^a-z0-9-]/g, ""))
      .filter(word => word.length > 0);
  }

  /**
   * Load Skill full content
   * @param name Skill name
   * Markdown body of @returns SKILL.md
   */
  async loadSkillContent(name: string): Promise<string> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    // Check the cache.
    if (this.config.cacheEnabled && skill.content) {
      const cached = this.contentCache.get(name);
      if (cached && Date.now() - cached.timestamp < (this.config.cacheTTL || 300000)) {
        return cached.content;
      }
    }

    // Read the file
    const skillMdPath = path.join(skill.path, "SKILL.md");
    const content = await fs.readFile(skillMdPath, "utf-8");

    // Remove the YAML frontmatter.
    const bodyMatch = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)/);
    const body = bodyMatch && bodyMatch[1] ? bodyMatch[1].trim() : content;

    // Update the cache
    if (this.config.cacheEnabled) {
      skill.content = body;
      this.contentCache.set(name, { content: body, timestamp: Date.now() });
    }

    return body;
  }

  /**
   * Load Skill resource
   * @param name Skill name
   * @param resourceType Resource type
   * @param resourcePath Resource path (relative to the resource directory)
   * @returns Resource content
   */
  async loadSkillResource(
    name: string,
    resourceType: "references" | "examples" | "scripts" | "assets",
    resourcePath: string,
  ): Promise<string | Buffer> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    const cacheKey = `${name}:${resourceType}:${resourcePath}`;

    // Check the cache
    if (this.config.cacheEnabled) {
      const cached = this.resourceCache.get(cacheKey);
      if (cached && Date.now() - cached.timestamp < (this.config.cacheTTL || 300000)) {
        return cached.content;
      }
    }

    // Read the file
    const fullPath = path.join(skill.path, resourceType, resourcePath);
    const content =
      resourceType === "assets"
        ? await fs.readFile(fullPath)
        : await fs.readFile(fullPath, "utf-8");

    // Update the cache
    if (this.config.cacheEnabled) {
      this.resourceCache.set(cacheKey, {
        content: content as string | Buffer,
        timestamp: Date.now(),
      });
    }

    return content;
  }

  /**
   * List all resources for the Skill
   * @param name Skill name
   * @param resourceType Resource type
   * @returns Array of resource paths
   */
  async listSkillResources(
    name: string,
    resourceType: "references" | "examples" | "scripts" | "assets",
  ): Promise<string[]> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill '${name}' not found`);
    }

    const resourceDir = path.join(skill.path, resourceType);

    try {
      const entries = await fs.readdir(resourceDir, { withFileTypes: true });
      return entries.filter(entry => entry.isFile()).map(entry => entry.name);
    } catch {
      return [];
    }
  }

  /**
   * Clear the cache
   */
  clearCache(): void {
    this.contentCache.clear();
    this.resourceCache.clear();

    for (const skill of this.skills.values()) {
      skill.content = undefined;
      skill.references = undefined;
      skill.examples = undefined;
      skill.scripts = undefined;
      skill.assets = undefined;
    }
  }

  /**
   * Reload all Skills
   */
  async reload(): Promise<void> {
    this.skills.clear();
    this.clearCache();
    await this.initialize();
  }
}
