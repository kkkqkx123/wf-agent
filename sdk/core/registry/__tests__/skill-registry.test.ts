import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SkillRegistry } from '../skill-registry.js';
import type { SkillFileLoader } from '../../../services/skill-loader/types.js';

function createMockLoader(): SkillFileLoader {
  return {
    readDirectory: vi.fn().mockResolvedValue([]),
    listFiles: vi.fn().mockResolvedValue([]),
    exists: vi.fn().mockResolvedValue(false),
    readTextFile: vi.fn().mockResolvedValue(''),
    readBinaryFile: vi.fn().mockResolvedValue(Buffer.from('')),
    resolve: vi.fn((...s: string[]) => s.join('/')),
    join: vi.fn((...s: string[]) => s.join('/')),
    basename: vi.fn((p: string) => p.split('/').pop() || p),
  };
}

describe('SkillRegistry', () => {
  let registry: SkillRegistry;
  const mockSkillConfig = {
    paths: ['/mock/skills'],
    autoScan: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    registry = new SkillRegistry(mockSkillConfig, createMockLoader());
  });

  describe('constructor', () => {
    it('should create registry with config', () => {
      expect(registry).toBeDefined();
    });

    it('should apply default config values', () => {
      const r = new SkillRegistry({ paths: ['/test'] }, createMockLoader());
      expect((r as any).config.autoScan).toBe(true);
      expect((r as any).constructor.CACHE_ENABLED).toBe(true);
      expect((r as any).constructor.CACHE_TTL).toBe(300000);
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

    it('should parse when_to_use field', () => {
      const r = registry as any;
      const result = r.parseYamlFrontmatter(
        'name: my-skill\ndescription: A test skill\nwhen_to_use: Use this when reviewing PRs or checking code quality',
      );
      expect(result.name).toBe('my-skill');
      expect(result.description).toBe('A test skill');
      expect(result.when_to_use).toBe('Use this when reviewing PRs or checking code quality');
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