/**
 * Zod Schemas for Script Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import type { ScriptType } from "./script.js";

// ============================================================================
// Script Type Schema
// ============================================================================

/**
 * Script Type Schema
 */
export const ScriptTypeSchema = z.custom<ScriptType>((val): val is ScriptType =>
  ["SHELL", "CMD", "POWERSHELL", "PYTHON", "JAVASCRIPT"].includes(val as ScriptType),
);

// ============================================================================
// Sandbox Config Schema
// ============================================================================

/**
 * Sandbox Config Schema
 */
export const SandboxConfigSchema = z.object({
  type: z.enum(["docker", "nodejs", "python", "custom"]),
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
    type: ScriptTypeSchema,
    description: z.string().min(1, "Script description is required"),
    content: z.string().optional(),
    filePath: z.string().optional(),
    options: ScriptExecutionOptionsSchema,
    metadata: ScriptMetadataSchema.optional(),
    enabled: z.boolean().optional(),
  })
  .refine(data => data.content || data.filePath, {
    message: "Script must have either content or filePath",
    path: ["content"],
  });

// ============================================================================
// Script Security Related Schemas
// ============================================================================

/**
 * Script Risk Level Schema
 */
export const ScriptRiskLevelSchema = z.enum(["none", "low", "medium", "high"]);

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
  threadId: z.string(),
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
  scriptType: ScriptTypeSchema,
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
