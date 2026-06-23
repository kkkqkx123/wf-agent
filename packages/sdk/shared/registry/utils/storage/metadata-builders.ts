/**
 * Storage Metadata Builders
 *
 * Consolidates all StorageEntityInfo definitions for different entity types.
 * Each builder encapsulates entity-specific knowledge for persistence operations.
 */

import type {
  AgentProfileStorageMetadata,
  HookTemplateStorageMetadata,
  NodeTemplateStorageMetadata,
  ScriptStorageMetadata,
  ToolStorageMetadata,
  TriggerStorageMetadata,
  HookTemplate,
  NodeTemplate,
  Script,
  Tool,
  TriggerTemplate,
} from "@wf-agent/types";
import type { AgentProfileMeta } from "../../agent-profile-registry.js";
import type { StorageEntityInfo } from "./entity-storage.js";

// ==================== Agent Profile ====================

export const agentProfileMetadataBuilder: StorageEntityInfo<
  AgentProfileMeta,
  AgentProfileStorageMetadata
> = {
  getId: (profile) => profile.id,
  buildMetadata: (profile) => ({
    profileId: profile.id,
    name: profile.name,
    description: profile.description || "",
  }),
  entityName: "agent profile",
};

// ==================== Hook Template ====================

export const hookTemplateMetadataBuilder: StorageEntityInfo<
  HookTemplate,
  HookTemplateStorageMetadata
> = {
  getId: (template) => template.name,
  buildMetadata: (template) => ({
    name: template.name,
    hookType: template.hook.hookType,
    description: template.description || "",
    tags: (template.metadata?.["tags"] as string[]) || [],
    category: (template.metadata?.["category"] as string) || "",
    createdAt: template.createdAt,
    updatedAt: template.updatedAt,
  }),
  entityName: "hook template",
};

// ==================== Node Template ====================

export const nodeTemplateMetadataBuilder: StorageEntityInfo<
  NodeTemplate,
  NodeTemplateStorageMetadata
> = {
  getId: (template) => template.name,
  buildMetadata: (template) => ({
    name: template.name,
    type: template.type,
    description: template.description || "",
    tags: (template.metadata?.["tags"] as string[]) || [],
    category: (template.metadata?.["category"] as string) || "",
    createdAt: template.createdAt,
    updatedAt: template.updatedAt,
  }),
  entityName: "node template",
};

// ==================== Script ====================

export const scriptMetadataBuilder: StorageEntityInfo<Script, ScriptStorageMetadata> = {
  getId: (script) => script.name,
  buildMetadata: (script) => ({
    name: script.name,
    description: script.description || "",
    enabled: script.enabled ?? true,
    tags: script.metadata?.tags || [],
    category: script.metadata?.category || "",
    createdAt: 0,
    updatedAt: 0,
  }),
  entityName: "script",
};

// ==================== Tool ====================

export const toolMetadataBuilder: StorageEntityInfo<Tool, ToolStorageMetadata> = {
  getId: (tool) => tool.id,
  buildMetadata: (tool) => ({
    toolId: tool.id,
    type: tool.type,
    description: tool.description || "",
    tags: tool.metadata?.tags || [],
    category: tool.metadata?.category || "",
  }),
  entityName: "tool",
};

// ==================== Trigger Template ====================

export const triggerTemplateMetadataBuilder: StorageEntityInfo<
  TriggerTemplate,
  TriggerStorageMetadata
> = {
  getId: (template) => template.name,
  buildMetadata: (template) => ({
    name: template.name,
    description: template.description || "",
    enabled: template.enabled ?? true,
    createdAt: template.createdAt,
    updatedAt: template.updatedAt,
    tags: (template.metadata?.["tags"] as string[]) || [],
    category: (template.metadata?.["category"] as string) || "",
  }),
  entityName: "trigger template",
};
