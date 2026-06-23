/**
 * Vitest Base Configuration (ESM)
 * Shared configuration for all vitest configs in the monorepo
 */

export const baseTestConfig = {
  environment: "node",
  reporters: ["verbose"],
  clearMocks: true,
  restoreMocks: true,
  globals: true,
};

export const baseResolveAlias = {
  "@wf-agent/common-utils": (basePath) => `${basePath}/packages/common-utils/src`,
  "@wf-agent/types": (basePath) => `${basePath}/packages/types/src`,
  "@wf-agent/tool-executors": (basePath) => `${basePath}/packages/tool-executors/src`,
  "@wf-agent/storage": (basePath) => `${basePath}/packages/storage/src`,
};
