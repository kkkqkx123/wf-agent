import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SkillRegistry } from '../skill-registry.js';

vi.mock('fs/promises', () => ({
  default: {
    readdir: vi.fn(),
    readFile: vi.fn(),
    access: vi.fn(),
  },
  readdir: vi.fn(),
  readFile: vi.fn(),
  access: vi.fn(),
}));

import * as fs from 'fs/promises';

describe('SkillRegistry', () => {
  let registry: SkillRegistry;
  const mockSkillConfig = {
    paths: ['/mock/skills'],
    autoScan: false,
    cacheEnabled: true,
    cacheTTL: 300000,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    registry = new SkillRegistry(mockSkillConfig);
  });

  describe('constructor', () => {
    it('should create registry with config', () => {
      expect(registry).toBeDefined();
    });

    it('should apply default config values', () => {
      const r = new SkillRegistry({ paths: ['/test'] });
      expect((r as any).config.autoScan).toBe(true);
      expect((r as any).config.cacheEnabled).toBe(true);
      expect((r as any).config.cacheTTL).toBe(300000);
    });
  });

  describe('parseYamlValue', () => {
    it('should parse quoted strings', () => {
      const r = registry as any;
      expect(r.parseYamlValue('"hello"')).toBe('hello');
      expect(r.parseYamlValue("'world'")).toBe('world');
    });

    it('should parse booleans', () => {
      const r = registry as any;
      expect(r.parseYamlValue('true')).toBe(true);
      expect(r.parseYamlValue('false')).toBe(false);
    });

    it('should parse numbers', () => {
      const r = registry as any;
      expect(r.parseYamlValue('42')).toBe(42);
      expect(r.parseYamlValue('3.14')).toBe(3.14);
    });

    it('should parse null values', () => {
      const r = registry as any;
      expect(r.parseYamlValue('null')).toBeNull();
      expect(r.parseYamlValue('~')).toBeNull();
    });

    it('should return string as default', () => {
      const r = registry as any;
      expect(r.parseYamlValue('hello-world')).toBe('hello-world');
    });
  });

  describe('parseYamlFrontmatter', () => {
    it('should parse simple key-value pairs', () => {
      const r = registry as any;
      const result = r.parseYamlFrontmatter('name: my-skill\ndescription: A test skill');
      expect(result.name).toBe('my-skill');
      expect(result.description).toBe('A test skill');
    });

    it('should parse array values', () => {
      const r = registry as any;
      const result = r.parseYamlFrontmatter(
        'allowedTools:\n- tool1\n- tool2',
      );
      expect(result.allowedTools).toEqual(['tool1', 'tool2']);
    });

    it('should skip empty lines and comments', () => {
      const r = registry as any;
      const result = r.parseYamlFrontmatter(
        'name: my-skill\n\n# this is a comment\ndescription: test',
      );
      expect(result.name).toBe('my-skill');
      expect(result.description).toBe('test');
    });
  });

  describe('extractKeywords', () => {
    it('should extract meaningful words', () => {
      const r = registry as any;
      const keywords = r.extractKeywords('process data files');
      expect(keywords).toContain('process');
      expect(keywords).toContain('data');
      expect(keywords).toContain('files');
    });

    it('should remove stopwords', () => {
      const r = registry as any;
      const keywords = r.extractKeywords('this is a test of the system');
      expect(keywords).not.toContain('this');
      expect(keywords).not.toContain('is');
      expect(keywords).not.toContain('a');
      expect(keywords).toContain('test');
      expect(keywords).toContain('system');
    });

    it('should remove short words', () => {
      const r = registry as any;
      const keywords = r.extractKeywords('go to the car');
      expect(keywords).not.toContain('go');
      expect(keywords).toContain('car');
    });

    it('should handle empty input', () => {
      const r = registry as any;
      expect(r.extractKeywords('')).toEqual([]);
    });
  });

  describe('calculateMatchScore', () => {
    const metadata = {
      name: 'data-processor',
      description: 'Process and transform data files efficiently',
    };

    it('should return 1.0 for exact name match', () => {
      const r = registry as any;
      const score = r.calculateMatchScore('data-processor', metadata);
      expect(score).toBe(1.0);
    });

    it('should return 0.8 for partial name match', () => {
      const r = registry as any;
      const score = r.calculateMatchScore('processor', metadata);
      expect(score).toBe(0.8);
    });

    it('should return 0 for no match', () => {
      const r = registry as any;
      const score = r.calculateMatchScore('xyzunknown', metadata);
      expect(score).toBe(0);
    });

    it('should return score based on keyword match', () => {
      const r = registry as any;
      const score = r.calculateMatchScore('transform', metadata);
      expect(score).toBeGreaterThan(0);
      expect(score).toBeLessThanOrEqual(0.7);
    });
  });

  describe('matchSkills', () => {
    beforeEach(() => {
      const r = registry as any;
      r.skills.set('data-processor', {
        metadata: { name: 'data-processor', description: 'Process data files' },
        path: '/mock/skills/data-processor',
      });
      r.skills.set('image-tool', {
        metadata: { name: 'image-tool', description: 'Resize and convert images' },
        path: '/mock/skills/image-tool',
      });
    });

    it('should return matching skills', () => {
      const results = registry.matchSkills('data');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0].skill.name).toBe('data-processor');
    });

    it('should return empty array for no match', () => {
      const results = registry.matchSkills('nonexistent');
      expect(results).toHaveLength(0);
    });

    it('should sort results by score descending', () => {
      const r = registry as any;
      r.skills.set('process-all', {
        metadata: { name: 'process-all', description: 'Process everything including data files' },
        path: '/mock/skills/process-all',
      });

      const results = registry.matchSkills('process');
      for (let i = 1; i < results.length; i++) {
        expect(results[i - 1].score).toBeGreaterThanOrEqual(results[i].score);
      }
    });
  });

  describe('getAllSkills', () => {
    it('should return empty array initially', () => {
      expect(registry.getAllSkills()).toEqual([]);
    });

    it('should return all skills metadata', () => {
      const r = registry as any;
      r.skills.set('skill1', { metadata: { name: 'skill1', description: 'desc1' }, path: '/p1' });
      r.skills.set('skill2', { metadata: { name: 'skill2', description: 'desc2' }, path: '/p2' });
      expect(registry.getAllSkills()).toHaveLength(2);
    });
  });

  describe('getSkill', () => {
    it('should return skill by name', () => {
      const r = registry as any;
      const skill = { metadata: { name: 'my-skill', description: 'desc' }, path: '/p' };
      r.skills.set('my-skill', skill);
      expect(registry.getSkill('my-skill')).toBe(skill);
    });

    it('should return undefined for non-existent skill', () => {
      expect(registry.getSkill('non-existent')).toBeUndefined();
    });
  });
});