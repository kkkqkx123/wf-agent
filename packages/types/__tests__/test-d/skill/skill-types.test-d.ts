/**
 * Skill Types Test
 * 
 * Tests for Skill type definitions following Claude Code Skill specification.
 * These types define the structure for skill metadata, resources, and configuration.
 * 
 * Priority: 🟢 LOW (Stage 3)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  SkillMetadata,
  SkillResourceType,
  Skill,
  SkillConfig,
  SkillMatchResult,
  SkillLoadContext,
  SkillLoadResult,
} from "../../../src/index.js";
import { SkillParseError, SkillValidationError } from "../../../src/index.js";

// ============================================================================
// Test 1: Skill Metadata Type
// ============================================================================

/**
 * Test SkillMetadata required fields
 */
const basicMetadata: SkillMetadata = {
  name: "my-skill",
  description: "A test skill for demonstration",
};

expectType<SkillMetadata>(basicMetadata);
expectType<string>(basicMetadata.name);
expectType<string>(basicMetadata.description);

/**
 * Test SkillMetadata with all optional fields
 */
const fullMetadata: SkillMetadata = {
  name: "advanced-skill",
  description: "An advanced skill with all metadata",
  version: "1.0.0",
  license: "MIT",
  allowedTools: ["readFile", "writeFile"],
  metadata: {
    author: "John Doe",
    category: "development",
  },
};

expectType<SkillMetadata>(fullMetadata);
expectType<string | undefined>(fullMetadata.version);
expectType<string | undefined>(fullMetadata.license);
expectType<string[] | undefined>(fullMetadata.allowedTools);
expectType<Record<string, string> | undefined>(fullMetadata.metadata);

/**
 * Test SkillMetadata name format (hyphen-case)
 */
const validNames: SkillMetadata[] = [
  { name: "simple", description: "Simple skill" },
  { name: "multi-word-skill", description: "Multi-word skill" },
  { name: "skill-with-numbers-123", description: "Skill with numbers" },
];

for (const meta of validNames) {
  expectType<SkillMetadata>(meta);
  expectType<string>(meta.name);
}

/**
 * Test SkillMetadata custom metadata field
 */
const customMetadata: SkillMetadata = {
  name: "custom-skill",
  description: "Skill with custom metadata",
  metadata: {
    "x-custom-field": "value",
    "x-another-field": "another-value",
  },
};

expectType<SkillMetadata>(customMetadata);
if (customMetadata.metadata) {
  expectType<Record<string, string>>(customMetadata.metadata);
}

// ============================================================================
// Test 2: Skill Resource Type
// ============================================================================

/**
 * Test SkillResourceType union
 */
declare const resourceType: SkillResourceType;
expectType<"references" | "examples" | "scripts" | "assets">(resourceType);

/**
 * Test assignable resource types
 */
expectAssignable<SkillResourceType>("references");
expectAssignable<SkillResourceType>("examples");
expectAssignable<SkillResourceType>("scripts");
expectAssignable<SkillResourceType>("assets");

// ============================================================================
// Test 3: Skill Definition Type
// ============================================================================

/**
 * Test minimal Skill definition
 */
const minimalSkill: Skill = {
  metadata: {
    name: "minimal",
    description: "Minimal skill",
  },
  path: "/path/to/skill",
};

expectType<Skill>(minimalSkill);
expectType<SkillMetadata>(minimalSkill.metadata);
expectType<string>(minimalSkill.path);
expectType<string | undefined>(minimalSkill.content);

/**
 * Test complete Skill definition with all resources
 */
const completeSkill: Skill = {
  metadata: {
    name: "complete-skill",
    description: "Complete skill with all resources",
    version: "1.0.0",
  },
  path: "/skills/complete-skill",
  content: "# Complete Skill\n\nThis is the main content.",
  references: {
    "guide.md": "# Guide\n\nReference content...",
    "api.md": "# API Reference\n\nAPI details...",
  },
  examples: {
    "example1.ts": "console.log('Example 1');",
    "example2.py": "print('Example 2')",
  },
  scripts: {
    "setup.sh": "#!/bin/bash\necho 'Setup'",
    "cleanup.sh": "#!/bin/bash\necho 'Cleanup'",
  },
  assets: {
    "logo.png": Buffer.from("image data"),
    "config.json": '{"key": "value"}',
  },
};

expectType<Skill>(completeSkill);
expectType<string | undefined>(completeSkill.content);
expectType<Record<string, string> | undefined>(completeSkill.references);
expectType<Record<string, string> | undefined>(completeSkill.examples);
expectType<Record<string, string> | undefined>(completeSkill.scripts);
expectType<Record<string, string | Buffer> | undefined>(completeSkill.assets);

/**
 * Test Skill with only some resources
 */
const partialSkill: Skill = {
  metadata: {
    name: "partial-skill",
    description: "Skill with some resources",
  },
  path: "/skills/partial",
  content: "# Partial Skill",
  references: {
    "README.md": "# README",
  },
  // examples, scripts, assets are omitted
};

expectType<Skill>(partialSkill);

/**
 * Test Skill lazy loading pattern
 */
const lazySkill: Skill = {
  metadata: {
    name: "lazy-skill",
    description: "Skill loaded on demand",
  },
  path: "/skills/lazy",
  // content and resources not loaded yet
};

expectType<Skill>(lazySkill);
expectType<string | undefined>(lazySkill.content);

// ============================================================================
// Test 4: Skill Configuration Type
// ============================================================================

/**
 * Test SkillConfig with required fields
 */
const basicSkillConfig: SkillConfig = {
  paths: ["/skills", "./local-skills"],
};

expectType<SkillConfig>(basicSkillConfig);
expectType<string[]>(basicSkillConfig.paths);

/**
 * Test SkillConfig with all optional fields
 */
const fullSkillConfig: SkillConfig = {
  paths: ["/skills/global", "/skills/local"],
  autoScan: true,
  cacheEnabled: true,
  cacheTTL: 300000, // 5 minutes
};

expectType<SkillConfig>(fullSkillConfig);
expectType<boolean | undefined>(fullSkillConfig.autoScan);
expectType<boolean | undefined>(fullSkillConfig.cacheEnabled);
expectType<number | undefined>(fullSkillConfig.cacheTTL);

/**
 * Test SkillConfig with single path
 */
const singlePathConfig: SkillConfig = {
  paths: ["/skills"],
  autoScan: false,
};

expectType<SkillConfig>(singlePathConfig);

/**
 * Test SkillConfig with relative paths
 */
const relativePathConfig: SkillConfig = {
  paths: ["./skills", "../shared-skills"],
};

expectType<SkillConfig>(relativePathConfig);

// ============================================================================
// Test 5: Skill Match Result Type
// ============================================================================

/**
 * Test SkillMatchResult interface
 */
const matchResult: SkillMatchResult = {
  skill: {
    name: "matched-skill",
    description: "A matched skill",
  },
  score: 0.95,
  reason: "High relevance to query",
};

expectType<SkillMatchResult>(matchResult);
expectType<SkillMetadata>(matchResult.skill);
expectType<number>(matchResult.score);
expectType<string>(matchResult.reason);

/**
 * Test SkillMatchResult with different scores
 */
const lowScoreMatch: SkillMatchResult = {
  skill: {
    name: "low-match",
    description: "Low matching skill",
  },
  score: 0.3,
  reason: "Partial keyword match",
};

const perfectMatch: SkillMatchResult = {
  skill: {
    name: "perfect-match",
    description: "Perfect matching skill",
  },
  score: 1.0,
  reason: "Exact name match",
};

expectType<SkillMatchResult>(lowScoreMatch);
expectType<SkillMatchResult>(perfectMatch);

/**
 * Test score range validation (documentation)
 * Score should be between 0 and 1
 */
const validScores: number[] = [0.0, 0.25, 0.5, 0.75, 1.0];
for (const score of validScores) {
  const result: SkillMatchResult = {
    skill: { name: "test", description: "test" },
    score,
    reason: "Test",
  };
  expectType<SkillMatchResult>(result);
}

// ============================================================================
// Test 6: Skill Load Context Type
// ============================================================================

/**
 * Test SkillLoadContext with minimal fields
 */
const minimalLoadContext: SkillLoadContext = {
  skill: {
    metadata: {
      name: "load-test",
      description: "Skill for load testing",
    },
    path: "/skills/load-test",
  },
};

expectType<SkillLoadContext>(minimalLoadContext);
expectType<Skill>(minimalLoadContext.skill);
expectType<unknown | undefined>(minimalLoadContext.agentContext);
expectType<Record<string, unknown> | undefined>(minimalLoadContext.variables);
expectType<string[] | undefined>(minimalLoadContext.tools);

/**
 * Test SkillLoadContext with all fields
 */
const fullLoadContext: SkillLoadContext = {
  skill: {
    metadata: {
      name: "context-skill",
      description: "Skill with full context",
    },
    path: "/skills/context",
    content: "# Content",
  },
  agentContext: {
    state: "active",
    userId: "user123",
  },
  variables: {
    userName: "John",
    projectId: "proj-456",
  },
  tools: ["readFile", "writeFile", "bash"],
};

expectType<SkillLoadContext>(fullLoadContext);
expectType<unknown | undefined>(fullLoadContext.agentContext);
expectType<Record<string, unknown> | undefined>(fullLoadContext.variables);
expectType<string[] | undefined>(fullLoadContext.tools);

// ============================================================================
// Test 7: Skill Load Result Type
// ============================================================================

/**
 * Test successful SkillLoadResult
 */
const successLoadResult: SkillLoadResult = {
  success: true,
  content: "# Loaded Skill Content",
  data: { processed: true },
  loadTime: 150,
  cached: false,
};

expectType<SkillLoadResult>(successLoadResult);
expectType<boolean>(successLoadResult.success);
expectType<string | undefined>(successLoadResult.content);
expectType<unknown | undefined>(successLoadResult.data);
expectType<Error | undefined>(successLoadResult.error);
expectType<number | undefined>(successLoadResult.loadTime);
expectType<boolean | undefined>(successLoadResult.cached);

/**
 * Test failed SkillLoadResult
 */
const failedLoadResult: SkillLoadResult = {
  success: false,
  error: new Error("Failed to load skill"),
  loadTime: 50,
};

expectType<SkillLoadResult>(failedLoadResult);
expectType<boolean>(failedLoadResult.success);
expectType<Error | undefined>(failedLoadResult.error);

/**
 * Test cached SkillLoadResult
 */
const cachedLoadResult: SkillLoadResult = {
  success: true,
  content: "# Cached Content",
  cached: true,
  loadTime: 5, // Fast due to cache
};

expectType<SkillLoadResult>(cachedLoadResult);
expectType<boolean | undefined>(cachedLoadResult.cached);

// ============================================================================
// Test 8: Skill Error Classes
// ============================================================================

/**
 * Test SkillParseError construction
 */
const parseError = new SkillParseError(
  "/skills/broken",
  "Invalid YAML frontmatter",
  new SyntaxError("Unexpected token"),
);

expectType<SkillParseError>(parseError);
expectType<string>(parseError.skillPath);
expectType<string>(parseError.reason);
expectType<Error | undefined>(parseError.originalError);
expectType<string>(parseError.message);
expectType<string>(parseError.name);

/**
 * Test SkillParseError without original error
 */
const simpleParseError = new SkillParseError(
  "/skills/invalid",
  "Missing required field: name",
);

expectType<SkillParseError>(simpleParseError);
expectType<string>(simpleParseError.skillPath);
expectType<string>(simpleParseError.reason);
expectType<Error | undefined>(simpleParseError.originalError);

/**
 * Test SkillValidationError construction
 */
const validationError = new SkillValidationError(
  "test-skill",
  "Name must use hyphen-case format",
);

expectType<SkillValidationError>(validationError);
expectType<string>(validationError.skillName);
expectType<string>(validationError.reason);
expectType<string>(validationError.message);
expectType<string>(validationError.name);

/**
 * Test error inheritance - both errors extend Error
 */
// parseError and validationError are instances of Error subclasses
// They can be used wherever Error is expected
const errorArray: Error[] = [parseError, validationError];
expectType<Error[]>(errorArray);

// ============================================================================
// Test 9: Real-world Usage Patterns
// ============================================================================

/**
 * Test skill discovery and loading workflow
 */
interface SkillManager {
  config: SkillConfig;
  discoverSkills(): Promise<Skill[]>;
  loadSkill(skill: Skill, context?: SkillLoadContext): Promise<SkillLoadResult>;
  matchSkills(query: string): SkillMatchResult[];
}

const mockSkillManager: SkillManager = {
  config: {
    paths: ["/skills"],
    autoScan: true,
  },
  async discoverSkills() {
    return [
      {
        metadata: { name: "skill1", description: "First skill" },
        path: "/skills/skill1",
      },
    ];
  },
  async loadSkill(skill, context) {
    return {
      success: true,
      content: skill.content,
      loadTime: 100,
    };
  },
  matchSkills(query) {
    return [
      {
        skill: { name: "matched", description: "Matched skill" },
        score: 0.8,
        reason: "Keyword match",
      },
    ];
  },
};

expectType<SkillManager>(mockSkillManager);

/**
 * Test skill validation function
 */
function validateSkill(skill: Skill): SkillLoadResult {
  if (!skill.metadata.name) {
    return {
      success: false,
      error: new SkillValidationError(skill.metadata.name || "unknown", "Missing name"),
    };
  }
  
  return {
    success: true,
    content: skill.content,
  };
}

const validationResult = validateSkill(minimalSkill);
expectType<SkillLoadResult>(validationResult);
expectType<boolean>(validationResult.success);

/**
 * Test skill filtering by allowed tools
 */
function filterSkillsByTools(skills: Skill[], allowedTools: string[]): Skill[] {
  return skills.filter((skill) => {
    if (!skill.metadata.allowedTools) {
      return true; // No restrictions
    }
    return skill.metadata.allowedTools.every((tool) => allowedTools.includes(tool));
  });
}

const filtered = filterSkillsByTools([completeSkill], ["readFile", "writeFile"]);
expectType<Skill[]>(filtered);

// ============================================================================
// Test 10: Type Assignability
// ============================================================================

/**
 * Test that SkillMetadata is assignable with minimal fields
 */
expectAssignable<SkillMetadata>({
  name: "test",
  description: "Test skill",
});

/**
 * Test that Skill is assignable with minimal fields
 */
expectAssignable<Skill>({
  metadata: {
    name: "test",
    description: "Test",
  },
  path: "/test",
});

/**
 * Test that SkillConfig is assignable
 */
expectAssignable<SkillConfig>({
  paths: ["/skills"],
});

/**
 * Test that SkillMatchResult is assignable
 */
expectAssignable<SkillMatchResult>({
  skill: {
    name: "test",
    description: "Test",
  },
  score: 0.5,
  reason: "Test match",
});

/**
 * Test that SkillLoadResult is assignable for success case
 */
expectAssignable<SkillLoadResult>({
  success: true,
  content: "Content",
});

/**
 * Test that SkillLoadResult is assignable for failure case
 */
expectAssignable<SkillLoadResult>({
  success: false,
  error: new Error("Failed"),
});
