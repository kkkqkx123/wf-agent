/**
 * Zod Schemas for Script Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { EXECUTOR_SHELL_CONFIGS } from "./script-executor.js";

// ============================================================================
// Sandbox Mode & Script Language Schemas
// ============================================================================

/**
 * Sandbox Mode Schema
 */
export const SandboxModeSchema = z.enum(["disabled", "lenient", "strict", "custom"]);

/**
 * Script Language Schema
 */
export const ScriptLanguageSchema = z.enum(["auto", "shell", "python", "javascript", "lua"]);

/**
 * Script Risk Level Schema
 */
export const ScriptRiskLevelSchema = z.enum(["none", "low", "medium", "high"]);

// ============================================================================
// Policy Sub-Schemas
// ============================================================================

/**
 * Filesystem Policy Schema
 */
export const FilesystemPolicySchema = z.object({
  allowedReadPaths: z.array(z.string()),
  allowedWritePaths: z.array(z.string()),
  allowedRemovePaths: z.array(z.string()),
  allowedExecutePaths: z.array(z.string()),
  copyOnWrite: z.boolean(),
  maxFileSize: z.number().nonnegative(),
});

/**
 * Process Policy Schema
 */
export const ProcessPolicySchema = z.object({
  allowedChildProcesses: z.array(z.string()),
  deniedChildProcesses: z.array(z.string()),
  maxChildProcesses: z.number().nonnegative(),
  allowFork: z.boolean(),
  allowExec: z.boolean(),
});

/**
 * Network Policy Schema
 */
export const NetworkPolicySchema = z.object({
  access: z.enum(["none", "localhost", "specific", "all"]),
  allowedDomains: z.array(z.string()).optional(),
  allowedPorts: z.array(z.tuple([z.number(), z.number()])).optional(),
  allowDns: z.boolean(),
});

/**
 * Resource Policy Schema
 */
export const ResourcePolicySchema = z.object({
  cpuLimit: z.number().nonnegative().optional(),
  memoryLimit: z.number().nonnegative().optional(),
  diskLimit: z.number().nonnegative().optional(),
  timeoutLimit: z.number().nonnegative(),
});

/**
 * Shell Policy Schema
 */
export const ShellPolicySchema = z.object({
  allowedCommands: z.array(z.string()),
  deniedCommands: z.array(z.string()),
  dangerousPatterns: z.array(z.string()),
  allowPipe: z.boolean(),
  allowRedirect: z.boolean(),
});

/**
 * Python Policy Schema
 */
export const PythonPolicySchema = z.object({
  allowedModules: z.array(z.string()),
  deniedModules: z.array(z.string()),
  allowSubprocess: z.boolean(),
  restrictBuiltinOpen: z.boolean(),
  allowDynamicEval: z.boolean(),
});

/**
 * JavaScript Policy Schema
 */
export const JavaScriptPolicySchema = z.object({
  allowedModules: z.array(z.string()),
  deniedModules: z.array(z.string()),
  allowChildProcess: z.boolean(),
  allowFSWrite: z.boolean(),
  allowDynamicEval: z.boolean(),
});

/**
 * Lua Policy Schema
 */
export const LuaPolicySchema = z.object({
  allowedModules: z.array(z.string()),
  deniedModules: z.array(z.string()),
  allowOsExecute: z.boolean(),
  restrictIoOpen: z.boolean(),
  allowDynamicLoad: z.boolean(),
});

/**
 * Sandbox Policy Schema
 */
export const SandboxPolicySchema = z.object({
  mode: SandboxModeSchema,
  filesystem: FilesystemPolicySchema.partial().optional(),
  process: ProcessPolicySchema.partial().optional(),
  network: NetworkPolicySchema.partial().optional(),
  resource: ResourcePolicySchema.partial().optional(),
  shell: ShellPolicySchema.partial().optional(),
  python: PythonPolicySchema.partial().optional(),
  javascript: JavaScriptPolicySchema.partial().optional(),
  lua: LuaPolicySchema.partial().optional(),
});

// ============================================================================
// VFS Config Schema
// ============================================================================

/**
 * VFS Config Schema
 */
export const VFSConfigSchema = z.object({
  enabled: z.boolean(),
  workspaceRoot: z.string(),
  pathPolicy: z
    .object({
      readable: z.array(z.string()).optional(),
      writable: z.array(z.string()).optional(),
    })
    .optional(),
});

// ============================================================================
// Sandbox Config Schema (Refactored with legacy compat)
// ============================================================================

/**
 * Sandbox Config Schema
 */
export const SandboxConfigSchema = z.object({
  // Profile reference
  profile: z.string().optional(),

  // New fields
  mode: SandboxModeSchema.optional(),
  policy: SandboxPolicySchema.partial().optional(),
  shellStrategy: z.array(z.string()).optional(),
  pythonStrategy: z.array(z.string()).optional(),
  javascriptStrategy: z.array(z.string()).optional(),
  luaStrategy: z.array(z.string()).optional(),
  vfs: VFSConfigSchema.optional(),

  // Legacy backward-compatible fields
  type: z.enum(["docker", "nodejs", "python", "custom"]).optional(),
  image: z.string().optional(),
  resourceLimits: z
    .object({
      memory: z.number().positive().optional(),
      cpu: z.number().positive().optional(),
      disk: z.number().positive().optional(),
    })
    .optional(),
  network: z
    .object({
      enabled: z.boolean(),
      allowedDomains: z.array(z.string()).optional(),
    })
    .optional(),
  filesystem: z
    .object({
      allowedPaths: z.array(z.string()).optional(),
      readOnly: z.boolean().optional(),
    })
    .optional(),
});

// ============================================================================
// Supporting Schemas (ScriptArgument, ScriptExecutorConfig)
// ============================================================================

/**
 * Script Argument Schema
 */
export const ScriptArgumentSchema = z.object({
  key: z.string().min(1),
  type: z.enum(["string", "number", "boolean", "file"]),
  label: z.string().optional(),
  description: z.string().optional(),
  default: z.any().optional(),
  required: z.boolean().optional(),
  source: z.enum(["static", "variable", "expression"]).optional(),
  options: z.array(z.any()).optional(),
  pattern: z.string().optional(),
});

/**
 * Script Executor Config Schema
 *
 * Validation rules:
 *   - runtime "docker" or "ssh" requires runtimeConfig to be set
 *   - runtime "native" or "wsl" must NOT have runtimeConfig
 */

/** Docker runtime connection config schema */
export const DockerConfigSchema = z.object({
  container: z.string().min(1, "Docker container name/ID is required"),
  host: z.string().optional(),
  tlsVerify: z.boolean().optional(),
  user: z.string().optional(),
  workdir: z.string().optional(),
  extraFlags: z.array(z.string()).optional(),
});

/** SSH runtime connection config schema */
export const SSHConfigSchema = z.object({
  host: z.string().min(1, "SSH host is required"),
  port: z.number().int().positive().optional(),
  user: z.string().optional(),
  identityFile: z.string().optional(),
  passphrase: z.string().optional(),
  password: z.string().optional(),
  forwardAgent: z.boolean().optional(),
  extraFlags: z.array(z.string()).optional(),
});

/** Runtime connection config schema (discriminated by runtime value) */
export const RuntimeConfigSchema = z.union([DockerConfigSchema, SSHConfigSchema]);

/**
 * Script Executor Config Schema
 *
 * Refine rules:
 *   - runtime=docker|ssh  → runtimeConfig is required
 *   - runtime=native|wsl  → runtimeConfig is not allowed
 */
export const ScriptExecutorConfigSchema = z
  .object({
    mode: z.enum([
      "direct",
      "shared",
      "pty",
      "sandbox-shell",
      "sandbox-python",
      "sandbox-javascript",
    ]),
    shell: z.enum(EXECUTOR_SHELL_CONFIGS),
    runtime: z.enum(["native", "wsl", "docker", "ssh"]).optional(),
    runtimeConfig: RuntimeConfigSchema.optional(),
    cwd: z.string().optional(),
    environment: z.record(z.string(), z.string()).optional(),
  })
  .refine(
    data => {
      if (data.runtime === "docker" || data.runtime === "ssh") {
        return data.runtimeConfig !== undefined;
      }
      return true;
    },
    {
      message: "runtimeConfig is required when runtime is 'docker' or 'ssh'",
      path: ["runtimeConfig"],
    },
  )
  .refine(
    data => {
      if (data.runtimeConfig && data.runtime !== "docker" && data.runtime !== "ssh") {
        return false;
      }
      return true;
    },
    {
      message: "runtimeConfig is only allowed when runtime is 'docker' or 'ssh'",
      path: ["runtimeConfig"],
    },
  );

// ============================================================================
// Sandbox Profile Schemas
// ============================================================================

/**
 * Sandbox Profile Schema
 */
export const SandboxProfileSchema = z.object({
  name: z.string().min(1),
  description: z.string().optional(),
  mode: SandboxModeSchema,
  shellStrategy: z.array(z.string()).optional(),
  pythonStrategy: z.array(z.string()).optional(),
  javascriptStrategy: z.array(z.string()).optional(),
  policy: SandboxPolicySchema.partial().optional(),
  vfs: VFSConfigSchema.optional(),
});

/**
 * Sandbox Profile Rule Schema
 */
export const SandboxProfileRuleSchema = z.object({
  language: z.union([ScriptLanguageSchema, z.array(ScriptLanguageSchema)]).optional(),
  riskLevel: z.union([ScriptRiskLevelSchema, z.array(ScriptRiskLevelSchema)]).optional(),
  profile: z.string().min(1),
});

/**
 * Sandbox Global Config Schema
 */
export const SandboxGlobalConfigSchema = z.object({
  mode: SandboxModeSchema.optional(),
  profiles: z.array(SandboxProfileSchema).optional(),
  rules: z.array(SandboxProfileRuleSchema).optional(),
  defaultProfile: z.string().optional(),
});

// ============================================================================
// Script Execution Options Schema
// ============================================================================

/**
 * Script Execution Options Schema
 */
export const ScriptExecutionOptionsSchema = z.object({
  timeout: z.number().positive().optional(),
  retries: z.number().nonnegative().optional(),
  retryDelay: z.number().nonnegative().optional(),
  exponentialBackoff: z.boolean().optional(),
  workingDirectory: z.string().optional(),
  environment: z.record(z.string(), z.string()).optional(),
  sandbox: z.boolean().optional(),
  sandboxConfig: SandboxConfigSchema.optional(),
  executorMode: z
    .enum(["direct", "shared", "pty", "sandbox-shell", "sandbox-python", "sandbox-javascript"])
    .optional(),
  language: ScriptLanguageSchema.optional(),
});

// ============================================================================
// Script Metadata Schema
// ============================================================================

/**
 * Script Metadata Schema
 */
export const ScriptMetadataSchema = z.object({
  category: z.string().optional(),
  tags: z.array(z.string()).optional(),
  author: z.string().optional(),
  version: z.string().optional(),
  documentationUrl: z.string().url().optional(),
  customFields: z.record(z.string(), z.any()).optional(),
});

// ============================================================================
// Script Schema
// ============================================================================

/**
 * Script Schema
 */
export const ScriptSchema = z
  .object({
    id: z.string().min(1, "Script ID is required"),
    name: z.string().min(1, "Script name is required"),
    description: z.string().min(1, "Script description is required"),
    content: z.string().optional(),
    filePath: z.string().optional(),
    template: z.string().optional(),
    arguments: z.array(ScriptArgumentSchema).optional(),
    executor: ScriptExecutorConfigSchema.optional(),
    options: ScriptExecutionOptionsSchema,
    language: ScriptLanguageSchema.optional(),
    metadata: ScriptMetadataSchema.optional(),
    enabled: z.boolean().optional(),
  })
  .refine(data => data.content || data.filePath || data.template, {
    message: "Script must have either content, filePath, or template",
    path: ["content"],
  });

// ============================================================================
// Script Security Related Schemas
// ============================================================================

/**
 * Validation Result Schema
 */
export const ValidationResultSchema = z.object({
  valid: z.boolean(),
  message: z.string().optional(),
  metadata: z.record(z.string(), z.any()).optional(),
});

/**
 * Security Violation Schema
 */
export const SecurityViolationSchema = z.object({
  type: z.enum([
    "risk_level",
    "forbidden_command",
    "forbidden_path",
    "size_exceeded",
    "blacklisted",
  ]),
  message: z.string(),
  severity: z.enum(["error", "warning"]),
  details: z.record(z.string(), z.any()).optional(),
});

/**
 * Security Check Result Schema
 */
export const SecurityCheckResultSchema = z.object({
  secure: z.boolean(),
  violations: z.array(SecurityViolationSchema),
  recommendations: z.array(z.string()).optional(),
});

/**
 * Script Security Policy Schema
 */
export const ScriptSecurityPolicySchema = z.object({
  allowedRiskLevels: z.array(ScriptRiskLevelSchema),
  whitelist: z.array(z.string()).optional(),
  blacklist: z.array(z.string()).optional(),
  forbiddenCommands: z.array(z.string()).optional(),
  forbiddenPathPatterns: z.array(z.instanceof(RegExp)).optional(),
  maxScriptSize: z.number().positive().optional(),
  allowDynamicScripts: z.boolean().optional(),
});

/**
 * Audit Event Schema
 */
export const AuditEventSchema = z.object({
  eventType: z.string(),
  timestamp: z.instanceof(Date),
  executionId: z.string(),
  nodeId: z.string(),
  nodeName: z.string().optional(),
  userId: z.string().optional(),
  scriptName: z.string().optional(),
  riskLevel: ScriptRiskLevelSchema.optional(),
  data: z.record(z.string(), z.any()).optional(),
});

// ============================================================================
// Script Execution Result Schema
// ============================================================================

/**
 * Script Execution Result Schema
 */
export const ScriptExecutionResultSchema = z.object({
  success: z.boolean(),
  scriptName: z.string(),
  stdout: z.string().optional(),
  stderr: z.string().optional(),
  exitCode: z.number().optional(),
  executionTime: z.number().nonnegative(),
  error: z.string().optional(),
  environment: z.record(z.string(), z.any()).optional(),
  retryCount: z.number().nonnegative().optional(),
});

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Type guard for SandboxConfig
 */
export const isSandboxConfig = (config: unknown): config is z.infer<typeof SandboxConfigSchema> => {
  return SandboxConfigSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptExecutionOptions
 */
export const isScriptExecutionOptions = (
  config: unknown,
): config is z.infer<typeof ScriptExecutionOptionsSchema> => {
  return ScriptExecutionOptionsSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptMetadata
 */
export const isScriptMetadata = (
  config: unknown,
): config is z.infer<typeof ScriptMetadataSchema> => {
  return ScriptMetadataSchema.safeParse(config).success;
};

/**
 * Type guard for Script
 */
export const isScript = (config: unknown): config is z.infer<typeof ScriptSchema> => {
  return ScriptSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptRiskLevel
 */
export const isScriptRiskLevel = (
  config: unknown,
): config is z.infer<typeof ScriptRiskLevelSchema> => {
  return ScriptRiskLevelSchema.safeParse(config).success;
};

/**
 * Type guard for ValidationResult
 */
export const isValidationResult = (
  config: unknown,
): config is z.infer<typeof ValidationResultSchema> => {
  return ValidationResultSchema.safeParse(config).success;
};

/**
 * Type guard for SecurityViolation
 */
export const isSecurityViolation = (
  config: unknown,
): config is z.infer<typeof SecurityViolationSchema> => {
  return SecurityViolationSchema.safeParse(config).success;
};

/**
 * Type guard for SecurityCheckResult
 */
export const isSecurityCheckResult = (
  config: unknown,
): config is z.infer<typeof SecurityCheckResultSchema> => {
  return SecurityCheckResultSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptSecurityPolicy
 */
export const isScriptSecurityPolicy = (
  config: unknown,
): config is z.infer<typeof ScriptSecurityPolicySchema> => {
  return ScriptSecurityPolicySchema.safeParse(config).success;
};

/**
 * Type guard for AuditEvent
 */
export const isAuditEvent = (config: unknown): config is z.infer<typeof AuditEventSchema> => {
  return AuditEventSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptExecutionResult
 */
export const isScriptExecutionResult = (
  config: unknown,
): config is z.infer<typeof ScriptExecutionResultSchema> => {
  return ScriptExecutionResultSchema.safeParse(config).success;
};

/**
 * Type guard for SandboxMode
 */
export const isSandboxMode = (config: unknown): config is z.infer<typeof SandboxModeSchema> => {
  return SandboxModeSchema.safeParse(config).success;
};

/**
 * Type guard for ScriptLanguage
 */
export const isScriptLanguage = (
  config: unknown,
): config is z.infer<typeof ScriptLanguageSchema> => {
  return ScriptLanguageSchema.safeParse(config).success;
};

/**
 * Type guard for SandboxPolicy
 */
export const isSandboxPolicy = (config: unknown): config is z.infer<typeof SandboxPolicySchema> => {
  return SandboxPolicySchema.safeParse(config).success;
};

/**
 * Type guard for SandboxProfile
 */
export const isSandboxProfile = (
  config: unknown,
): config is z.infer<typeof SandboxProfileSchema> => {
  return SandboxProfileSchema.safeParse(config).success;
};

/**
 * Type guard for SandboxGlobalConfig
 */
export const isSandboxGlobalConfig = (
  config: unknown,
): config is z.infer<typeof SandboxGlobalConfigSchema> => {
  return SandboxGlobalConfigSchema.safeParse(config).success;
};
