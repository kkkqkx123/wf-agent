/**
 * Plugin Loader - Discovers and loads plugins from filesystem and packages.
 *
 * Supports:
 * - node_modules packages with "wfAgentPlugin" in package.json
 * - plugins/ directory with plugin.json files
 * - Explicit plugin paths from configuration
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { Plugin, PluginManifest, ValidationResult, DiscoveredPlugin } from "./types.js";
import semver from "semver";

/**
 * Plugin Loader
 */
export class PluginLoader {
  constructor(private options: { paths: string[] }) {}

  /**
   * Scan directories for plugin manifests.
   * Supports:
   * - Node modules packages with "wfAgentPlugin" in package.json
   * - Plugin directories with plugin.json files
   * - Explicit plugin paths from configuration
   */
  async scanPlugins(): Promise<DiscoveredPlugin[]> {
    const discovered: DiscoveredPlugin[] = [];

    for (const scanPath of this.options.paths) {
      try {
        const resolvedPath = path.resolve(scanPath);
        const stat = await fs.stat(resolvedPath);

        if (stat.isDirectory()) {
          // Check if it's a node_modules directory
          if (path.basename(resolvedPath) === 'node_modules') {
            const packages = await this.scanNodeModules(resolvedPath);
            discovered.push(...packages);
          } else {
            // Scan individual plugin directories
            const plugins = await this.scanPluginDirectory(resolvedPath);
            discovered.push(...plugins);
          }
        } else if (stat.isFile()) {
          // Single plugin file (plugin.json)
          const manifest = await this.loadManifestFromFile(resolvedPath);
          if (manifest) {
            discovered.push({ manifest, sourcePath: resolvedPath });
          }
        }
      } catch {
        // Path doesn't exist or can't be accessed - skip silently
        continue;
      }
    }

    return discovered;
  }

  /**
   * Load a plugin module by its manifest.
   * Uses dynamic import() and verifies the exports.
   * @param bustCache - If true, clears the module cache before loading (for hot-reload)
   */
  async loadModule(manifest: PluginManifest, bustCache = false): Promise<Plugin> {
    const entryPoint = manifest.entryPoint;
    const basePath = manifest._basePath || process.cwd();
    const modulePath = path.isAbsolute(entryPoint)
      ? entryPoint
      : path.resolve(basePath, entryPoint);

    if (bustCache) {
      // Cache busting is handled by the `?v=${Date.now()}` query param in the import() call below
    }

    let moduleExports: unknown;
    try {
      // Use cache-busting query param for dynamic imports (ESM-compatible, no CommonJS require.cache needed)
      const cacheBuster = bustCache ? `?v=${Date.now()}` : '';
      moduleExports = await import(`${modulePath}${cacheBuster}`);
    } catch (error) {
      throw new PluginLoadError(
        `Failed to load plugin '${manifest.id}' from '${modulePath}': ${error instanceof Error ? error.message : String(error)}`,
        manifest.id,
      );
    }

    // Extract default export (the plugin instance)
    const moduleExportsRecord = moduleExports as Record<string, unknown>;
    const plugin = moduleExportsRecord['default'] as Plugin | undefined;

    if (!plugin || typeof plugin !== 'object') {
      throw new PluginLoadError(
        `Plugin '${manifest.id}' does not have a valid default export`,
        manifest.id,
      );
    }

    // Verify minimal plugin interface
    if (!plugin.manifest) {
      throw new PluginLoadError(
        `Plugin '${manifest.id}' is missing 'manifest' property`,
        manifest.id,
      );
    }

    return plugin;
  }

  /**
   * Validate manifest against SDK version constraints.
   */
  validateManifest(manifest: PluginManifest, sdkVersion: string): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Validate required fields
    if (!manifest.id) {
      errors.push("Plugin manifest is missing 'id'");
    }
    if (!manifest.version) {
      errors.push("Plugin manifest is missing 'version'");
    }
    if (!manifest.entryPoint) {
      errors.push("Plugin manifest is missing 'entryPoint'");
    }
    if (!manifest.sdkVersion) {
      errors.push("Plugin manifest is missing 'sdkVersion'");
    }

    // Validate SDK version compatibility using semver
    if (manifest.sdkVersion && sdkVersion) {
      if (!semver.satisfies(sdkVersion, manifest.sdkVersion)) {
        warnings.push(
          `SDK version '${sdkVersion}' does not satisfy plugin '${manifest.id}' requirement '${manifest.sdkVersion}'`,
        );
      }
    }

    // Validate plugin version is valid semver
    if (manifest.version && !semver.valid(manifest.version)) {
      errors.push(`Plugin '${manifest.id}' has invalid version '${manifest.version}'`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings,
    };
  }

  /**
   * Scan a node_modules directory for packages with wfAgentPlugin in package.json.
   */
  private async scanNodeModules(nodeModulesPath: string): Promise<DiscoveredPlugin[]> {
    const discovered: DiscoveredPlugin[] = [];
    let entries: string[];

    try {
      entries = await fs.readdir(nodeModulesPath);
    } catch {
      return [];
    }

    // Support both @scope/package and package formats
    for (const entry of entries) {
      const entryPath = path.join(nodeModulesPath, entry);

      // Handle scoped packages (@scope)
      if (entry.startsWith('@')) {
        let scopedEntries: string[];
        try {
          scopedEntries = await fs.readdir(entryPath);
        } catch {
          continue;
        }
        for (const scopedEntry of scopedEntries) {
          const pkgPath = path.join(entryPath, scopedEntry);
          const manifest = await this.loadManifestFromPackageJson(pkgPath);
          if (manifest) {
            discovered.push({ manifest, sourcePath: pkgPath });
          }
        }
      } else {
        const manifest = await this.loadManifestFromPackageJson(entryPath);
        if (manifest) {
          discovered.push({ manifest, sourcePath: entryPath });
        }
      }
    }

    return discovered;
  }

  /**
   * Scan a plugin directory for plugin.json files.
   */
  private async scanPluginDirectory(dirPath: string): Promise<DiscoveredPlugin[]> {
    const discovered: DiscoveredPlugin[] = [];
    let entries: string[];

    try {
      entries = await fs.readdir(dirPath);
    } catch {
      return [];
    }

    for (const entry of entries) {
      const entryPath = path.join(dirPath, entry);
      let stat: Awaited<ReturnType<typeof fs.stat>>;
      try {
        stat = await fs.stat(entryPath);
      } catch {
        continue;
      }

      if (stat.isDirectory()) {
        // Check for plugin.json in directory
        const pluginJsonPath = path.join(entryPath, 'plugin.json');
        try {
          await fs.stat(pluginJsonPath);
          const manifest = await this.loadManifestFromFile(pluginJsonPath);
          if (manifest) {
            manifest._basePath = entryPath;
            discovered.push({ manifest, sourcePath: entryPath });
          }
        } catch {
          // No plugin.json, check package.json
          const manifest = await this.loadManifestFromPackageJson(entryPath);
          if (manifest) {
            discovered.push({ manifest, sourcePath: entryPath });
          }
        }
      } else if (entry === 'plugin.json') {
        const manifest = await this.loadManifestFromFile(entryPath);
        if (manifest) {
          manifest._basePath = dirPath;
          discovered.push({ manifest, sourcePath: dirPath });
        }
      }
    }

    return discovered;
  }

  /**
   * Load a manifest from a plugin.json file.
   */
  private async loadManifestFromFile(filePath: string): Promise<PluginManifest | null> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      const data = JSON.parse(content) as Partial<PluginManifest>;
      return {
        id: data.id || '',
        version: data.version || '',
        name: data.name,
        description: data.description,
        sdkVersion: data.sdkVersion || '',
        entryPoint: data.entryPoint || './dist/index.js',
        dependencies: data.dependencies,
        optionalDependencies: data.optionalDependencies,
        config: data.config,
        contributions: data.contributions,
        hooks: data.hooks,
        _basePath: data._basePath || path.dirname(filePath),
      };
    } catch {
      return null;
    }
  }

  /**
   * Load a manifest from a package.json file (checking wfAgentPlugin field).
   */
  private async loadManifestFromPackageJson(pkgDir: string): Promise<PluginManifest | null> {
    const pkgPath = path.join(pkgDir, 'package.json');
    try {
      const content = await fs.readFile(pkgPath, 'utf-8');
      const pkg = JSON.parse(content) as Record<string, unknown>;
      const pluginConfig = pkg['wfAgentPlugin'] as Record<string, unknown> | undefined;

      if (!pluginConfig) {
        return null;
      }

      return {
        id: (pkg['name'] as string) || '',
        version: (pkg['version'] as string) || '',
        name: (pkg['name'] as string),
        description: (pkg['description'] as string),
        sdkVersion: (pluginConfig['sdkVersion'] as string) || '',
        entryPoint: (pluginConfig['entryPoint'] as string) || './dist/index.js',
        dependencies: (pluginConfig['dependencies'] as Record<string, string>) || (pkg['dependencies'] as Record<string, string>),
        optionalDependencies: (pluginConfig['optionalDependencies'] as Record<string, string>) || (pkg['optionalDependencies'] as Record<string, string>),
        config: pluginConfig['config'] as Record<string, unknown>,
        contributions: (pluginConfig['contributions'] as string[]) as any,
        hooks: pluginConfig['hooks'] as Record<string, string>,
        _basePath: pkgDir,
      };
    } catch {
      return null;
    }
  }
}

/**
 * Error thrown when a plugin fails to load.
 */
export class PluginLoadError extends Error {
  constructor(
    message: string,
    public readonly pluginId: string,
  ) {
    super(message);
    this.name = 'PluginLoadError';
  }
}